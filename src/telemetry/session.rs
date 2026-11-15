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

//! Stage lifecycle instrumentation, token aggregation, and provider thought capture.

use super::clock::{NoopTelemetryClock, TelemetryClock};
use super::{NoopTelemetrySink, PipelineStage, TelemetryEvent, TelemetrySink};
use crate::provider::{
    DecisionRequest, DecisionResponse, GenerationRequest, GenerationResponse, ProviderError,
    StructuredModelProvider, TokenUsage,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

static STATIC_NOOP_SINK: NoopTelemetrySink = NoopTelemetrySink;
static STATIC_NOOP_CLOCK: NoopTelemetryClock = NoopTelemetryClock;

/// Scoped pipeline telemetry coordinator emitting `StageStart`, `Thought`, `StageFinish`,
/// and `PipelineFinish` events for a DAG closure execution.
pub struct TelemetrySession<'a> {
    sink: &'a dyn TelemetrySink,
    clock: &'a dyn TelemetryClock,
    pipeline_start_ms: u64,
    total_tokens: AtomicU64,
}

impl<'a> TelemetrySession<'a> {
    /// Initializes a pipeline telemetry session anchored to an explicit sink and clock source.
    pub fn new(sink: &'a dyn TelemetrySink, clock: &'a dyn TelemetryClock) -> Self {
        let pipeline_start_ms = if sink.is_enabled() {
            clock.monotonic_ms()
        } else {
            0
        };
        Self {
            sink,
            clock,
            pipeline_start_ms,
            total_tokens: AtomicU64::new(0),
        }
    }

    /// Returns `true` when an active sink is observing telemetry events.
    pub fn is_enabled(&self) -> bool {
        self.sink.is_enabled()
    }

    /// Executes a single pipeline stage closure, emitting `StageStart` prior to `f` and
    /// `StageFinish` (with accumulated [`TokenUsage`]) upon successful completion.
    pub fn run_stage<T, E>(
        &self,
        stage: PipelineStage,
        f: impl FnOnce(&StageScope<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        if !self.sink.is_enabled() {
            let disabled_scope = StageScope::new(stage, self.sink, None, false);
            return f(&disabled_scope);
        }

        let stage_start_ms = self.clock.monotonic_ms();
        let timestamp = self.clock.now_iso8601_utc();
        self.sink
            .emit(&TelemetryEvent::StageStart { stage, timestamp });

        let scope = StageScope::new(stage, self.sink, Some(&self.total_tokens), true);
        let result = f(&scope)?;

        let duration_ms = self.clock.monotonic_ms().saturating_sub(stage_start_ms);
        let usage = scope.into_usage();

        self.sink.emit(&TelemetryEvent::StageFinish {
            stage,
            duration_ms,
            usage,
        });

        Ok(result)
    }

    /// Emits the terminal `PipelineFinish` event summarizing elapsed duration and aggregate tokens.
    pub fn finish_pipeline(&self) {
        if !self.sink.is_enabled() {
            return;
        }
        let total_duration_ms = self
            .clock
            .monotonic_ms()
            .saturating_sub(self.pipeline_start_ms);
        let total_tokens = self.total_tokens.load(Ordering::SeqCst);
        self.sink.emit(&TelemetryEvent::PipelineFinish {
            total_duration_ms,
            total_tokens,
        });
    }
}

impl TelemetrySession<'static> {
    /// Constructs a zero-cost no-op telemetry session that performs no allocations or clock calls.
    pub fn noop() -> Self {
        Self {
            sink: &STATIC_NOOP_SINK,
            clock: &STATIC_NOOP_CLOCK,
            pipeline_start_ms: 0,
            total_tokens: AtomicU64::new(0),
        }
    }
}

/// Active stage telemetry context collecting reasoning thought chunks and multi-turn token usage.
pub struct StageScope<'a> {
    stage: PipelineStage,
    sink: &'a dyn TelemetrySink,
    total_tokens: Option<&'a AtomicU64>,
    enabled: bool,
    usage: Mutex<Option<TokenUsage>>,
}

impl<'a> StageScope<'a> {
    fn new(
        stage: PipelineStage,
        sink: &'a dyn TelemetrySink,
        total_tokens: Option<&'a AtomicU64>,
        enabled: bool,
    ) -> Self {
        Self {
            stage,
            sink,
            total_tokens,
            enabled,
            usage: Mutex::new(None),
        }
    }

    /// Returns the [`PipelineStage`] associated with this scope.
    pub const fn stage(&self) -> PipelineStage {
        self.stage
    }

    /// Emits a `TelemetryEvent::Thought` chunk for this stage when non-empty.
    pub fn record_thought(&self, delta: &str) {
        if self.enabled && !delta.is_empty() {
            self.sink.emit(&TelemetryEvent::Thought {
                stage: self.stage,
                delta: delta.to_string(),
            });
        }
    }

    /// Emits a `TelemetryEvent::StageProgress` completion tick (`completed` of `total`) for this stage.
    pub fn record_progress(&self, completed: usize, total: usize, label: &str) {
        if self.enabled && total > 0 {
            self.sink.emit(&TelemetryEvent::StageProgress {
                stage: self.stage,
                completed,
                total,
                label: label.to_string(),
            });
        }
    }

    /// Accumulates [`TokenUsage`] from a completed model generation turn into this stage's summary.
    pub fn record_usage(&self, turn_usage: TokenUsage) {
        if !self.enabled {
            return;
        }
        if let Some(counter) = self.total_tokens {
            counter.fetch_add(turn_usage.total_tokens, Ordering::SeqCst);
        }
        let mut guard = self.usage.lock().unwrap_or_else(PoisonError::into_inner);
        match guard.as_mut() {
            Some(existing) => existing.accumulate(turn_usage),
            None => *guard = Some(turn_usage),
        }
    }

    /// Emits a `TelemetryEvent::ModelCall` record and accumulates `usage` immediately upon call completion.
    pub fn record_model_call(
        &self,
        label: Option<&str>,
        resolved_model: &str,
        latency_ms: u64,
        usage: TokenUsage,
    ) {
        if !self.enabled {
            return;
        }
        self.sink.emit(&TelemetryEvent::ModelCall {
            stage: self.stage,
            label: label.unwrap_or_default().trim().to_string(),
            resolved_model: resolved_model.to_string(),
            latency_ms,
            usage,
        });
        self.record_usage(usage);
    }

    /// Records extracted `thoughts`, per-call `ModelCall` telemetry, and stage `usage` from [`GenerationResponse`].
    pub fn record_generation_with_label(&self, label: Option<&str>, response: &GenerationResponse) {
        if !self.enabled {
            return;
        }
        for thought in &response.thoughts {
            self.record_thought(thought);
        }
        self.record_model_call(
            label,
            &response.resolved_model,
            response.latency_ms,
            response.usage,
        );
    }

    /// Records both extracted `thoughts` and `usage` from a completed [`GenerationResponse`].
    pub fn record_generation(&self, response: &GenerationResponse) {
        self.record_generation_with_label(None, response);
    }

    /// Records per-call `ModelCall` telemetry and `usage` from a completed discriminative [`DecisionResponse`].
    pub fn record_decision_with_label(&self, label: Option<&str>, response: &DecisionResponse) {
        if !self.enabled {
            return;
        }
        self.record_model_call(
            label,
            &response.resolved_model,
            response.latency_ms,
            response.usage,
        );
    }

    /// Records `usage` from a completed discriminative [`DecisionResponse`].
    pub fn record_decision(&self, response: &DecisionResponse) {
        self.record_decision_with_label(None, response);
    }

    /// Wraps a [`StructuredModelProvider`] so all generation and decision turns automatically emit
    /// `Thought` events and accumulate `TokenUsage` on this stage scope.
    pub fn instrument_provider<'p>(
        &'p self,
        inner: &'p dyn StructuredModelProvider,
    ) -> InstrumentedModelProvider<'p> {
        InstrumentedModelProvider { inner, scope: self }
    }

    fn into_usage(self) -> Option<TokenUsage> {
        self.usage
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Decorator around [`StructuredModelProvider`] capturing reasoning thoughts and token accounting
/// into an active [`StageScope`] without mutating domain return payloads.
pub struct InstrumentedModelProvider<'a> {
    inner: &'a dyn StructuredModelProvider,
    scope: &'a StageScope<'a>,
}

impl StructuredModelProvider for InstrumentedModelProvider<'_> {
    fn generate_structured(
        &self,
        request: &GenerationRequest,
    ) -> Result<GenerationResponse, ProviderError> {
        let response = self.inner.generate_structured(request)?;
        self.scope
            .record_generation_with_label(request.call_label.as_deref(), &response);
        Ok(response)
    }

    fn evaluate_decisions(
        &self,
        request: &DecisionRequest,
    ) -> Result<DecisionResponse, ProviderError> {
        let response = self.inner.evaluate_decisions(request)?;
        self.scope
            .record_decision_with_label(request.call_label.as_deref(), &response);
        Ok(response)
    }

    fn record_progress(&self, completed: usize, total: usize, label: &str) {
        self.scope.record_progress(completed, total, label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::DEFAULT_GENERATION_TIMEOUT_MS;
    use crate::telemetry::{RecordingTelemetrySink, SteppingTelemetryClock};
    use serde_json::json;

    struct StubProvider {
        responses: Mutex<Vec<Result<GenerationResponse, ProviderError>>>,
    }

    impl StructuredModelProvider for StubProvider {
        fn generate_structured(
            &self,
            _request: &GenerationRequest,
        ) -> Result<GenerationResponse, ProviderError> {
            let mut guard = self.responses.lock().unwrap();
            guard.remove(0)
        }
    }

    mod lifecycle_and_aggregation {
        use super::*;

        fn two_turn_stub_provider() -> StubProvider {
            StubProvider {
                responses: Mutex::new(vec![
                    Ok(GenerationResponse {
                        payload: json!({"ok": true}),
                        resolved_model: "gemini-4-carbon".to_string(),
                        latency_ms: 120,
                        usage: TokenUsage {
                            prompt_tokens: 100,
                            completion_tokens: 20,
                            reasoning_tokens: 30,
                            total_tokens: 150,
                        },
                        thoughts: vec![
                            "Evaluating invariant clause.".to_string(),
                            String::new(),
                            "Checking exception applicability.".to_string(),
                        ],
                    }),
                    Ok(GenerationResponse {
                        payload: json!({"ok": true}),
                        resolved_model: "gemini-4-carbon".to_string(),
                        latency_ms: 95,
                        usage: TokenUsage {
                            prompt_tokens: 80,
                            completion_tokens: 15,
                            reasoning_tokens: 25,
                            total_tokens: 120,
                        },
                        thoughts: vec!["Second trial rationale.".to_string()],
                    }),
                ]),
            }
        }

        #[test]
        fn test_session_records_ordered_events_thoughts_and_aggregates_multi_turn_tokens() {
            let sink = RecordingTelemetrySink::new();
            let clock = SteppingTelemetryClock::new(1_767_225_600_000, 10);
            let session = TelemetrySession::new(&sink, &clock);

            session
                .run_stage(PipelineStage::Intake, |_scope| Ok::<_, ()>(()))
                .expect("intake stage");

            let provider = two_turn_stub_provider();
            let req1 = GenerationRequest {
                model_specifier: "google:gemini-4-carbon".to_string(),
                system_instruction: "sys".to_string(),
                user_prompt: "usr".to_string(),
                response_schema: json!({"type": "object"}),
                thinking_level: None,
                reasoning_budget: Some(512),
                timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
                call_label: Some(".canons/cli/first.md".to_string()),
            };
            let mut req2 = req1.clone();
            req2.call_label = Some(".canons/cli/second.md".to_string());

            session
                .run_stage(PipelineStage::Audit, |scope| {
                    let instrumented = scope.instrument_provider(&provider);
                    instrumented.generate_structured(&req1)?;
                    instrumented.generate_structured(&req2)?;
                    Ok::<_, ProviderError>(())
                })
                .expect("audit stage");

            session.finish_pipeline();

            let events = sink.events();
            assert_eq!(
                events,
                vec![
                    TelemetryEvent::StageStart {
                        stage: PipelineStage::Intake,
                        timestamp: "2026-01-01T00:00:00.020Z".to_string(),
                    },
                    TelemetryEvent::StageFinish {
                        stage: PipelineStage::Intake,
                        duration_ms: 10,
                        usage: None,
                    },
                    TelemetryEvent::StageStart {
                        stage: PipelineStage::Audit,
                        timestamp: "2026-01-01T00:00:00.040Z".to_string(),
                    },
                    TelemetryEvent::Thought {
                        stage: PipelineStage::Audit,
                        delta: "Evaluating invariant clause.".to_string(),
                    },
                    TelemetryEvent::Thought {
                        stage: PipelineStage::Audit,
                        delta: "Checking exception applicability.".to_string(),
                    },
                    TelemetryEvent::ModelCall {
                        stage: PipelineStage::Audit,
                        label: ".canons/cli/first.md".to_string(),
                        resolved_model: "gemini-4-carbon".to_string(),
                        latency_ms: 120,
                        usage: TokenUsage {
                            prompt_tokens: 100,
                            completion_tokens: 20,
                            reasoning_tokens: 30,
                            total_tokens: 150,
                        },
                    },
                    TelemetryEvent::Thought {
                        stage: PipelineStage::Audit,
                        delta: "Second trial rationale.".to_string(),
                    },
                    TelemetryEvent::ModelCall {
                        stage: PipelineStage::Audit,
                        label: ".canons/cli/second.md".to_string(),
                        resolved_model: "gemini-4-carbon".to_string(),
                        latency_ms: 95,
                        usage: TokenUsage {
                            prompt_tokens: 80,
                            completion_tokens: 15,
                            reasoning_tokens: 25,
                            total_tokens: 120,
                        },
                    },
                    TelemetryEvent::StageFinish {
                        stage: PipelineStage::Audit,
                        duration_ms: 10,
                        usage: Some(TokenUsage {
                            prompt_tokens: 180,
                            completion_tokens: 35,
                            reasoning_tokens: 55,
                            total_tokens: 270,
                        }),
                    },
                    TelemetryEvent::PipelineFinish {
                        total_duration_ms: 50,
                        total_tokens: 270,
                    },
                ]
            );
        }

        #[test]
        fn test_per_call_telemetry_survives_when_subsequent_call_times_out() {
            let sink = RecordingTelemetrySink::new();
            let clock = SteppingTelemetryClock::new(1_767_225_600_000, 5);
            let session = TelemetrySession::new(&sink, &clock);

            let provider = StubProvider {
                responses: Mutex::new(vec![
                    Ok(GenerationResponse {
                        payload: json!({"ok": true}),
                        resolved_model: "gemini-4-carbon".to_string(),
                        latency_ms: 1410,
                        usage: TokenUsage {
                            prompt_tokens: 500,
                            completion_tokens: 60,
                            reasoning_tokens: 1920,
                            total_tokens: 2480,
                        },
                        thoughts: vec!["Completed case reasoning.".to_string()],
                    }),
                    Err(ProviderError::Timeout {
                        model_specifier: "google:gemini-4-carbon".to_string(),
                        timeout_ms: 60_000,
                    }),
                ]),
            };

            let mut req1 = GenerationRequest {
                model_specifier: "google:gemini-4-carbon".to_string(),
                system_instruction: "sys".to_string(),
                user_prompt: "usr".to_string(),
                response_schema: json!({"type": "object"}),
                thinking_level: None,
                reasoning_budget: None,
                timeout_ms: 60_000,
                call_label: Some(".canons/cli/fast-case.md".to_string()),
            };
            let mut req2 = req1.clone();
            req2.call_label = Some(".canons/cli/stalled-case.md".to_string());

            let err = session
                .run_stage(PipelineStage::Audit, |scope| {
                    let instrumented = scope.instrument_provider(&provider);
                    instrumented.generate_structured(&req1)?;
                    req1.timeout_ms = 60_000;
                    instrumented.generate_structured(&req2)?;
                    Ok::<_, ProviderError>(())
                })
                .expect_err("second call times out");
            assert!(matches!(err, ProviderError::Timeout { .. }));

            let events = sink.events();
            assert!(events.iter().any(|ev| matches!(
                ev,
                TelemetryEvent::ModelCall {
                    stage: PipelineStage::Audit,
                    label,
                    latency_ms: 1410,
                    usage: TokenUsage {
                        reasoning_tokens: 1920,
                        total_tokens: 2480,
                        ..
                    },
                    ..
                } if label == ".canons/cli/fast-case.md"
            )));
        }

        #[test]
        fn test_failed_stage_omits_stage_finish() {
            let sink = RecordingTelemetrySink::new();
            let clock = SteppingTelemetryClock::new(1_767_225_600_000, 5);
            let session = TelemetrySession::new(&sink, &clock);

            let res: Result<(), &str> =
                session.run_stage(PipelineStage::Apprise, |_scope| Err("boom"));
            assert_eq!(res, Err("boom"));

            let events = sink.events();
            assert_eq!(events.len(), 1);
            assert!(matches!(
                events[0],
                TelemetryEvent::StageStart {
                    stage: PipelineStage::Apprise,
                    ..
                }
            ));
        }

        #[test]
        fn test_noop_session_is_disabled_and_executes_closure() {
            let session = TelemetrySession::noop();
            assert!(!session.is_enabled());
            let val: usize = session
                .run_stage(PipelineStage::Discover, |_scope| Ok::<_, ()>(42))
                .expect("noop stage");
            session.finish_pipeline();
            assert_eq!(val, 42);
        }
    }
}
