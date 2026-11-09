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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
        }
    }
}

/// Live connectivity and latency diagnostic record for a single configured model endpoint (`docs/architecture/nodes/probe.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProbeEndpointResult {
    /// Connection and authorization status (`"ok"` or `"error"`).
    pub status: ProbeStatus,
    /// Roundtrip request latency in milliseconds.
    pub latency_ms: u64,
    /// Resolved model identifier reported by the provider (or configured specifier on failure).
    pub model: String,
    /// Optional diagnostic error message and remediation hint when `status == ProbeStatus::Error`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Live provider connectivity and endpoint latency diagnostics captured during the `probe` stage (`docs/architecture/nodes/probe.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CaseloadProbe {
    /// Diagnostic probe result for the configured screener model endpoint (`.config.screener_model`).
    pub screener: ProbeEndpointResult,
    /// Diagnostic probe result for the configured auditor model endpoint (`.config.auditor_model`).
    pub auditor: ProbeEndpointResult,
}

impl CaseloadProbe {
    /// Returns `true` when both `screener` and `auditor` endpoints succeeded (`status == ProbeStatus::Ok`).
    pub fn is_healthy(&self) -> bool {
        self.screener.status == ProbeStatus::Ok && self.auditor.status == ProbeStatus::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{Caseload, DEFAULT_AUDITOR_MODEL};

    #[test]
    fn test_caseload_probe_round_trip_and_optional_error_omission() {
        let probe = CaseloadProbe {
            screener: ProbeEndpointResult {
                status: ProbeStatus::Ok,
                latency_ms: 142,
                model: "gemini-2.5-flash-lite".to_string(),
                error: None,
            },
            auditor: ProbeEndpointResult {
                status: ProbeStatus::Error,
                latency_ms: 5001,
                model: DEFAULT_AUDITOR_MODEL.to_string(),
                error: Some("Request timed out after 5000ms".to_string()),
            },
        };
        assert!(!probe.is_healthy());
        assert_eq!(ProbeStatus::Ok.as_str(), "ok");
        assert_eq!(ProbeStatus::Error.as_str(), "error");

        let caseload = Caseload::new().with_probe(probe);
        let encoded = serde_json::to_string(&caseload).expect("serialize probe caseload");
        assert!(encoded.contains(r#""status":"ok""#));
        assert!(encoded.contains(r#""status":"error""#));
        assert!(encoded.contains(r#""Request timed out after 5000ms""#));

        let decoded: Caseload = serde_json::from_str(&encoded).expect("deserialize probe caseload");
        assert_eq!(decoded, caseload);
    }
}
