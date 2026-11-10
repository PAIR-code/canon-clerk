// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Out-of-band structured Telemetry Event Stream domain models and hexagonal sink ports
//! (`docs/architecture/overview.md` §8).

pub mod clock;
pub mod session;

pub use clock::{
    NoopTelemetryClock, SteppingTelemetryClock, SystemTelemetryClock, TelemetryClock,
    format_unix_millis_iso8601,
};
pub use session::{InstrumentedModelProvider, StageScope, TelemetrySession};

use crate::provider::TokenUsage;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::Write;
use std::sync::{Mutex, PoisonError};

/// Named functional node in the Caseload DAG (`docs/architecture/overview.md` §5).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStage {
    /// Branch A: Ingest diffs, target paths, design intent, or PR metadata.
    Intake,
    /// Branch A: Verify `exists:` preconditions, match `triggers:`, and prune exhibits.
    Discover,
    /// Branch A: Static AST and frontmatter schema validation of candidate canons.
    Validate,
    /// Branch B: Resolve provider credentials, model specifiers, and reasoning budgets.
    Configure,
    /// Diagnostic Leaf: Live provider connectivity and roundtrip latency check.
    Probe,
    /// Apprisal Track: Prospective statutory applicability screening against design intent.
    Apprise,
    /// Cascade Spine: Macro subject-matter jurisdiction screening (`colorability_score`).
    Docket,
    /// Cascade Spine: Micro evidentiary relevance screening (`admissibility_score`).
    Admit,
    /// Cascade Spine: Substantive per-case compliance adjudication (`compliance_score`).
    Audit,
}

impl PipelineStage {
    /// Returns the canonical `snake_case` stage identifier matching CLI subcommand verbs.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Intake => "intake",
            Self::Discover => "discover",
            Self::Validate => "validate",
            Self::Configure => "configure",
            Self::Probe => "probe",
            Self::Apprise => "apprise",
            Self::Docket => "docket",
            Self::Admit => "admit",
            Self::Audit => "audit",
        }
    }
}

impl fmt::Display for PipelineStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Structured out-of-band operational telemetry event (`docs/architecture/overview.md` §8).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "event")]
pub enum TelemetryEvent {
    /// Signals entry into an executed [`PipelineStage`] with an ISO-8601 UTC `timestamp`.
    StageStart {
        /// Active pipeline stage entering execution.
        stage: PipelineStage,
        /// ISO-8601 UTC wall-clock timestamp (`YYYY-MM-DDTHH:MM:SS.sssZ`).
        timestamp: String,
    },
    /// Streams an intermediate reasoning `delta` chunk associated with an active heuristic stage.
    Thought {
        /// Active heuristic stage emitting extended thinking or rationale chunks.
        stage: PipelineStage,
        /// Extracted reasoning text summary or delta chunk.
        delta: String,
    },
    /// Records stage completion with elapsed wall-clock `duration_ms` and optional token `usage`.
    StageFinish {
        /// Completed pipeline stage.
        stage: PipelineStage,
        /// Elapsed wall-clock duration for this stage in milliseconds.
        duration_ms: u64,
        /// Optional token accounting metrics when model generation occurred during the stage.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
    },
    /// Summarizes cumulative `total_duration_ms` and aggregate `total_tokens` across the DAG closure.
    PipelineFinish {
        /// Cumulative elapsed wall-clock duration across the pipeline closure in milliseconds.
        total_duration_ms: u64,
        /// Aggregate `total_tokens` expended across all executed stages in the closure.
        total_tokens: u64,
    },
}

/// Hexagonal outbound port receiving structured operational telemetry events.
pub trait TelemetrySink: Send + Sync {
    /// Returns `true` when this sink observes emitted events (`false` for no-op sinks).
    fn is_enabled(&self) -> bool {
        true
    }

    /// Records a single structured [`TelemetryEvent`].
    fn emit(&self, event: &TelemetryEvent);
}

/// Zero-cost default [`TelemetrySink`] discarding all operational telemetry events.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopTelemetrySink;

impl TelemetrySink for NoopTelemetrySink {
    fn is_enabled(&self) -> bool {
        false
    }

    fn emit(&self, _event: &TelemetryEvent) {}
}

/// Thread-safe in-memory [`TelemetrySink`] recording emitted events for deterministic assertions.
#[derive(Debug, Default)]
pub struct RecordingTelemetrySink {
    events: Mutex<Vec<TelemetryEvent>>,
}

impl RecordingTelemetrySink {
    /// Constructs an empty in-memory telemetry recording sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a snapshot clone of all recorded [`TelemetryEvent`] records in emission order.
    pub fn events(&self) -> Vec<TelemetryEvent> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Serializes all recorded events as newline-delimited JSON Lines (`JSONL`).
    pub fn to_jsonl(&self) -> Result<String, serde_json::Error> {
        let snapshot = self.events();
        let mut out = String::new();
        for event in &snapshot {
            let line = serde_json::to_string(event)?;
            out.push_str(&line);
            out.push('\n');
        }
        Ok(out)
    }
}

impl TelemetrySink for RecordingTelemetrySink {
    fn emit(&self, event: &TelemetryEvent) {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event.clone());
    }
}

/// Multiplexing [`TelemetrySink`] forwarding each event to multiple downstream sinks in order.
#[derive(Default)]
pub struct CompositeTelemetrySink {
    sinks: Vec<Box<dyn TelemetrySink>>,
}

impl CompositeTelemetrySink {
    /// Constructs an empty composite sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a child sink if `sink.is_enabled()` is `true`.
    pub fn push(&mut self, sink: Box<dyn TelemetrySink>) {
        if sink.is_enabled() {
            self.sinks.push(sink);
        }
    }
}

impl TelemetrySink for CompositeTelemetrySink {
    fn is_enabled(&self) -> bool {
        !self.sinks.is_empty()
    }

    fn emit(&self, event: &TelemetryEvent) {
        for sink in &self.sinks {
            sink.emit(event);
        }
    }
}

/// Streaming JSON Lines (`JSONL`) [`TelemetrySink`] writing one serialized JSON object per line.
pub struct JsonlWriterSink<W: Write + Send> {
    writer: Mutex<W>,
}

impl<W: Write + Send> JsonlWriterSink<W> {
    /// Wraps an arbitrary `Write + Send` byte sink for streaming JSONL emission.
    pub fn new(writer: W) -> Self {
        Self {
            writer: Mutex::new(writer),
        }
    }

    /// Consumes the sink and returns the underlying writer.
    pub fn into_inner(self) -> W {
        self.writer
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl<W: Write + Send> TelemetrySink for JsonlWriterSink<W> {
    fn emit(&self, event: &TelemetryEvent) {
        if let Ok(line) = serde_json::to_string(event) {
            let mut guard = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
            let _ = writeln!(guard, "{line}");
            let _ = guard.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod wire_serialization {
        use super::*;

        #[test]
        fn test_all_pipeline_stages_round_trip_snake_case() {
            let stages = [
                (PipelineStage::Intake, "intake"),
                (PipelineStage::Discover, "discover"),
                (PipelineStage::Validate, "validate"),
                (PipelineStage::Configure, "configure"),
                (PipelineStage::Probe, "probe"),
                (PipelineStage::Apprise, "apprise"),
                (PipelineStage::Docket, "docket"),
                (PipelineStage::Admit, "admit"),
                (PipelineStage::Audit, "audit"),
            ];

            for (stage, expected) in stages {
                assert_eq!(stage.as_str(), expected);
                assert_eq!(stage.to_string(), expected);
                let encoded = serde_json::to_string(&stage).expect("serialize stage");
                assert_eq!(encoded, format!("\"{expected}\""));
                let decoded: PipelineStage =
                    serde_json::from_str(&encoded).expect("deserialize stage");
                assert_eq!(decoded, stage);
            }
        }

        #[test]
        fn test_all_four_telemetry_event_categories_serialize_deterministically() {
            let events = [
                TelemetryEvent::StageStart {
                    stage: PipelineStage::Intake,
                    timestamp: "2026-11-09T20:05:18.000Z".to_string(),
                },
                TelemetryEvent::StageFinish {
                    stage: PipelineStage::Intake,
                    duration_ms: 4,
                    usage: None,
                },
                TelemetryEvent::Thought {
                    stage: PipelineStage::Apprise,
                    delta: "Canon governs CLI flag naming.".to_string(),
                },
                TelemetryEvent::StageFinish {
                    stage: PipelineStage::Apprise,
                    duration_ms: 318,
                    usage: Some(TokenUsage {
                        prompt_tokens: 410,
                        completion_tokens: 52,
                        reasoning_tokens: 18,
                        total_tokens: 480,
                    }),
                },
                TelemetryEvent::PipelineFinish {
                    total_duration_ms: 325,
                    total_tokens: 480,
                },
            ];

            let serialized: Vec<String> = events
                .iter()
                .map(|ev| serde_json::to_string(ev).expect("serialize telemetry event"))
                .collect();

            assert_eq!(
                serialized[0],
                r#"{"event":"StageStart","stage":"intake","timestamp":"2026-11-09T20:05:18.000Z"}"#
            );
            assert_eq!(
                serialized[1],
                r#"{"event":"StageFinish","stage":"intake","duration_ms":4}"#
            );
            assert_eq!(
                serialized[2],
                r#"{"event":"Thought","stage":"apprise","delta":"Canon governs CLI flag naming."}"#
            );
            assert_eq!(
                serialized[3],
                r#"{"event":"StageFinish","stage":"apprise","duration_ms":318,"usage":{"prompt_tokens":410,"completion_tokens":52,"reasoning_tokens":18,"total_tokens":480}}"#
            );
            assert_eq!(
                serialized[4],
                r#"{"event":"PipelineFinish","total_duration_ms":325,"total_tokens":480}"#
            );

            for (idx, line) in serialized.iter().enumerate() {
                let decoded: TelemetryEvent =
                    serde_json::from_str(line).expect("deserialize telemetry event");
                assert_eq!(decoded, events[idx]);
            }
        }
    }

    mod sink_contracts {
        use super::*;

        #[test]
        fn test_recording_jsonl_and_composite_sinks() {
            let rec = RecordingTelemetrySink::new();
            let buf: Vec<u8> = Vec::new();
            let jsonl_sink = JsonlWriterSink::new(buf);

            let mut composite = CompositeTelemetrySink::new();
            assert!(!composite.is_enabled());
            composite.push(Box::new(NoopTelemetrySink));
            assert!(!composite.is_enabled());

            let shared_buf = JsonlWriterSink::new(Vec::<u8>::new());
            composite.push(Box::new(shared_buf));
            assert!(composite.is_enabled());

            let start = TelemetryEvent::StageStart {
                stage: PipelineStage::Configure,
                timestamp: "2026-11-09T20:00:00.000Z".to_string(),
            };
            let finish = TelemetryEvent::PipelineFinish {
                total_duration_ms: 3,
                total_tokens: 0,
            };

            rec.emit(&start);
            rec.emit(&finish);
            jsonl_sink.emit(&start);
            jsonl_sink.emit(&finish);
            composite.emit(&start);

            let rec_jsonl = rec.to_jsonl().expect("render jsonl");
            let writer_jsonl =
                String::from_utf8(jsonl_sink.into_inner()).expect("valid utf8 jsonl");
            assert_eq!(rec_jsonl, writer_jsonl);
            assert_eq!(rec_jsonl.lines().count(), 2);
        }
    }
}
