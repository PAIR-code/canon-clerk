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

//! Data plane models for the `.probe` namespace (`docs/architecture/nodes/probe.md` §3).

use crate::caseload::{HeuristicNode, ThinkingLevel};
use serde::{Deserialize, Serialize};

/// Connectivity and health status of a probed model endpoint (`docs/architecture/nodes/probe.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStatus {
    /// Endpoint is reachable, authenticated, and responsive (`"ok"`).
    #[default]
    Ok,
    /// Endpoint timed out, rejected credentials, or returned an HTTP error (`"error"`).
    Error,
}

impl ProbeStatus {
    /// Returns the canonical wire representation (`"ok"` or `"error"`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
        }
    }
}

/// Live connectivity and latency diagnostic record for a single deduplicated `(model, thinking_level, reasoning_budget)`
/// probe target (`docs/architecture/nodes/probe.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProbeTargetResult {
    /// Configured canonical `<provider>:<model>` specifier probed.
    pub model: String,
    /// Ordered list of provisioned heuristic nodes bound to this target.
    pub nodes: Vec<HeuristicNode>,
    /// Optional qualitative reasoning effort tier forwarded on the probe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    /// Optional token ceiling forwarded on the probe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_budget: Option<u32>,
    /// Connection and authorization status (`"ok"` or `"error"`).
    pub status: ProbeStatus,
    /// Roundtrip request latency in milliseconds.
    pub latency_ms: u64,
    /// Concrete model identifier reported by the upstream provider when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_model: Option<String>,
    /// Optional diagnostic error message and remediation hint when `status == ProbeStatus::Error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Live deduplicated provider connectivity and endpoint latency diagnostics captured during `probe` (`probe.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct CaseloadProbe {
    /// Deduplicated target probe results in canonical DAG stage encounter order.
    pub targets: Vec<ProbeTargetResult>,
}

impl CaseloadProbe {
    /// Returns `true` when at least one target was probed and every target succeeded (`status == ProbeStatus::Ok`).
    pub fn is_healthy(&self) -> bool {
        !self.targets.is_empty() && self.targets.iter().all(|t| t.status == ProbeStatus::Ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{
        Caseload, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, DEFAULT_GOOGLE_GENERATOR_MODEL,
    };

    mod serialization {
        use super::*;

        #[test]
        fn test_caseload_probe_round_trip_and_optional_field_omission() {
            let probe = CaseloadProbe {
                targets: vec![
                    ProbeTargetResult {
                        model: DEFAULT_GOOGLE_DISCRIMINATOR_MODEL.to_string(),
                        nodes: vec![
                            HeuristicNode::Apprise,
                            HeuristicNode::Docket,
                            HeuristicNode::Admit,
                            HeuristicNode::Vet,
                        ],
                        thinking_level: None,
                        reasoning_budget: None,
                        status: ProbeStatus::Ok,
                        latency_ms: 88,
                        resolved_model: Some("gemini-decision-1".to_string()),
                        error: None,
                    },
                    ProbeTargetResult {
                        model: DEFAULT_GOOGLE_GENERATOR_MODEL.to_string(),
                        nodes: vec![HeuristicNode::Audit],
                        thinking_level: Some(ThinkingLevel::Low),
                        reasoning_budget: None,
                        status: ProbeStatus::Error,
                        latency_ms: 5001,
                        resolved_model: None,
                        error: Some("Request timed out after 5000ms".to_string()),
                    },
                ],
            };
            assert!(!probe.is_healthy());
            assert_eq!(ProbeStatus::Ok.as_str(), "ok");
            assert_eq!(ProbeStatus::Error.as_str(), "error");

            let caseload = Caseload::new().with_probe(probe);
            let encoded = serde_json::to_string(&caseload).expect("serialize probe caseload");
            assert!(encoded.contains(r#""status":"ok""#));
            assert!(encoded.contains(r#""status":"error""#));
            assert!(encoded.contains(r#""resolved_model":"gemini-decision-1""#));
            assert!(encoded.contains(r#""Request timed out after 5000ms""#));

            let decoded: Caseload =
                serde_json::from_str(&encoded).expect("deserialize probe caseload");
            assert_eq!(decoded, caseload);
        }
    }
}
