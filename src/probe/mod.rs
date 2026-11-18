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

//! Core domain execution engine and hexagonal endpoint prober for the deduplicated `probe`
//! diagnostic stage (`docs/architecture/nodes/probe.md`).

mod gemini;
mod gemini_decisions;
mod typesafe;

pub use gemini::{GeminiEndpointProber, MultiProviderEndpointProber, decode_gemini_probe_response};
pub use gemini_decisions::{GeminiDecisionsEndpointProber, decode_gemini_decisions_probe_response};
pub use typesafe::TypesafeEndpointProber;

use crate::caseload::{
    Caseload, CaseloadProbe, HeuristicNode, ProbeStatus, ProbeTargetResult, TYPESAFE_PROVIDER,
    ThinkingLevel,
};
use crate::configure::{ENV_GEMINI_API_KEY, ENV_TYPESAFE_API_KEY, model_provider};
pub use crate::provider::{
    DEFAULT_GEMINI_BASE_URL, DEFAULT_TYPESAFE_BASE_URL, build_generate_content_url,
    strip_provider_prefix,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// Default per-endpoint timeout in milliseconds for `canon-clerk probe` (`docs/architecture/nodes/probe.md` §5).
pub const DEFAULT_PROBE_TIMEOUT_MS: u64 = 5000;

/// Environment variable name for overriding the probe timeout in milliseconds.
pub const ENV_PROBE_TIMEOUT_MS: &str = "CANON_CLERK_PROBE_TIMEOUT_MS";

/// Domain input parameters for executing the `probe` Caseload DAG stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeOptions {
    /// Google Gemini API key used to authenticate `google:*` probe requests.
    pub api_key: String,
    /// Optional TypeSafe AI API key used to authenticate `typesafe:*` probe requests.
    pub typesafe_api_key: Option<String>,
    /// Per-target request timeout in milliseconds (defaults to [`DEFAULT_PROBE_TIMEOUT_MS`]).
    pub timeout_ms: u64,
}

impl Default for ProbeOptions {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            typesafe_api_key: None,
            timeout_ms: DEFAULT_PROBE_TIMEOUT_MS,
        }
    }
}

/// Single-endpoint probe outcome returned by [`EndpointProber::probe_endpoint`].
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProbeEndpointResult {
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

/// Pre-flight domain errors when `execute_probe` cannot be initiated due to missing prerequisites or invalid options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    /// Incoming `Caseload` is missing the `.config` prerequisite namespace (or has zero provisioned nodes).
    MissingConfig,
    /// Required Google Gemini API key in `ProbeOptions` is empty or whitespace.
    MissingApiKey,
    /// Required TypeSafe AI API key in `ProbeOptions` is missing, empty, or whitespace.
    MissingTypesafeApiKey,
    /// Timeout budget in `ProbeOptions` is zero (`0ms`).
    InvalidTimeout { timeout_ms: u64 },
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingConfig => write!(
                f,
                "Cannot execute `probe` without resolved node bindings in `.config` on the Caseload.\n  \
                 Hint: Run `canon-clerk configure` first or invoke `canon-clerk probe` directly to telescope `[configure, probe]`."
            ),
            Self::MissingApiKey => write!(
                f,
                "Missing required Google Gemini API key (`{ENV_GEMINI_API_KEY}`) for endpoint probing.\n  \
                 Hint: Export `{ENV_GEMINI_API_KEY}=\"<your-api-key>\"`, configure \
                 `providers.google.apiKey` in `~/.config/canon-clerk/config.json` (mode `0600`), \
                 or pass `--gemini-api-key <KEY>`."
            ),
            Self::MissingTypesafeApiKey => write!(
                f,
                "Missing required TypeSafe AI API key (`{ENV_TYPESAFE_API_KEY}`) for endpoint probing.\n  \
                 Hint: Export `{ENV_TYPESAFE_API_KEY}=\"<your-api-key>\"`, configure \
                 `providers.typesafe.apiKey` in `~/.config/canon-clerk/config.json` (mode `0600`), \
                 or pass `--typesafe-api-key <KEY>`."
            ),
            Self::InvalidTimeout { timeout_ms } => write!(
                f,
                "Invalid probe timeout `{timeout_ms}ms`: timeout must be greater than 0ms.\n  \
                 Hint: Pass a positive millisecond timeout via `--timeout <MS>` (default: `{DEFAULT_PROBE_TIMEOUT_MS}`)."
            ),
        }
    }
}

impl std::error::Error for ProbeError {}

/// Hexagonal port for executing a live or simulated connectivity health probe against a model endpoint.
pub trait EndpointProber: Send + Sync {
    /// Probes a single canonical `<provider>:<model>` specifier using the provided API key,
    /// timeout budget in milliseconds, and optional thinking overrides (`probe.md` §4).
    fn probe_endpoint(
        &self,
        model_specifier: &str,
        api_key: &str,
        timeout_ms: u64,
        thinking_level: Option<ThinkingLevel>,
        reasoning_budget: Option<u32>,
    ) -> ProbeEndpointResult;
}

struct DeduplicatedProbeSpec<'a> {
    model: String,
    nodes: Vec<HeuristicNode>,
    thinking_level: Option<ThinkingLevel>,
    reasoning_budget: Option<u32>,
    api_key: &'a str,
}

type TargetKey = (String, Option<ThinkingLevel>, Option<u32>);

/// Executes the `probe` diagnostic stage concurrently across deduplicated `(model, thinking_level, reasoning_budget)`
/// targets bound in `.config.nodes` (`std::thread::scope`) and populates `.probe` on the [`Caseload`].
pub fn execute_probe(
    options: &ProbeOptions,
    prober: &dyn EndpointProber,
    caseload: Caseload,
) -> Result<Caseload, ProbeError> {
    if caseload.probe.is_some() {
        return Ok(caseload);
    }

    if options.timeout_ms == 0 {
        return Err(ProbeError::InvalidTimeout {
            timeout_ms: options.timeout_ms,
        });
    }

    let config = caseload.config.as_ref().ok_or(ProbeError::MissingConfig)?;
    if config.nodes.is_empty() {
        return Err(ProbeError::MissingConfig);
    }

    let mut index_by_key: BTreeMap<TargetKey, usize> = BTreeMap::new();
    let mut specs: Vec<DeduplicatedProbeSpec<'_>> = Vec::new();
    for (&node, node_cfg) in &config.nodes {
        let key: TargetKey = (
            node_cfg.model.clone(),
            node_cfg.thinking_level,
            node_cfg.reasoning_budget,
        );
        if let Some(&idx) = index_by_key.get(&key) {
            specs[idx].nodes.push(node);
        } else {
            let api_key = resolve_endpoint_probe_key(&node_cfg.model, options)?;
            let idx = specs.len();
            index_by_key.insert(key, idx);
            specs.push(DeduplicatedProbeSpec {
                model: node_cfg.model.clone(),
                nodes: vec![node],
                thinking_level: node_cfg.thinking_level,
                reasoning_budget: node_cfg.reasoning_budget,
                api_key,
            });
        }
    }

    let timeout_ms = options.timeout_ms;
    let specs_ref = &specs;
    let endpoints: Vec<ProbeEndpointResult> = std::thread::scope(|scope| {
        let handles: Vec<_> = specs_ref
            .iter()
            .map(|spec| {
                scope.spawn(move || {
                    prober.probe_endpoint(
                        &spec.model,
                        spec.api_key,
                        timeout_ms,
                        spec.thinking_level,
                        spec.reasoning_budget,
                    )
                })
            })
            .collect();

        specs_ref
            .iter()
            .zip(handles)
            .map(|(spec, handle)| {
                handle.join().unwrap_or_else(|_| ProbeEndpointResult {
                    status: ProbeStatus::Error,
                    latency_ms: 0,
                    resolved_model: None,
                    error: Some(format!(
                        "Probe worker thread for `{}` terminated unexpectedly. Hint: Retry `canon-clerk probe`.",
                        spec.model
                    )),
                })
            })
            .collect()
    });

    let targets = specs
        .into_iter()
        .zip(endpoints)
        .map(|(spec, endpoint)| ProbeTargetResult {
            model: spec.model,
            nodes: spec.nodes,
            thinking_level: spec.thinking_level,
            reasoning_budget: spec.reasoning_budget,
            status: endpoint.status,
            latency_ms: endpoint.latency_ms,
            resolved_model: endpoint.resolved_model,
            error: endpoint.error,
        })
        .collect();

    Ok(caseload.with_probe(CaseloadProbe { targets }))
}

fn resolve_endpoint_probe_key<'a>(
    model_specifier: &str,
    options: &'a ProbeOptions,
) -> Result<&'a str, ProbeError> {
    if model_provider(model_specifier) == TYPESAFE_PROVIDER {
        options
            .typesafe_api_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or(ProbeError::MissingTypesafeApiKey)
    } else {
        let key = options.api_key.trim();
        if key.is_empty() {
            Err(ProbeError::MissingApiKey)
        } else {
            Ok(key)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{
        CaseloadConfig, CaseloadDiscovery, CaseloadIntake, CaseloadValidation,
        DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, DEFAULT_GOOGLE_GENERATOR_MODEL,
        DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL, ResolvedNodeConfig,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct StubEndpointProber {
        responses: BTreeMap<String, ProbeEndpointResult>,
        call_count: AtomicUsize,
    }

    impl StubEndpointProber {
        fn new(responses: BTreeMap<String, ProbeEndpointResult>) -> Self {
            Self {
                responses,
                call_count: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    impl EndpointProber for StubEndpointProber {
        fn probe_endpoint(
            &self,
            model_specifier: &str,
            _api_key: &str,
            _timeout_ms: u64,
            _thinking_level: Option<ThinkingLevel>,
            _reasoning_budget: Option<u32>,
        ) -> ProbeEndpointResult {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            self.responses
                .get(model_specifier)
                .cloned()
                .unwrap_or_else(|| ProbeEndpointResult {
                    status: ProbeStatus::Ok,
                    latency_ms: 42,
                    resolved_model: Some(strip_provider_prefix(model_specifier).to_string()),
                    error: None,
                })
        }
    }

    mod url_and_response_decoding {
        use super::*;

        #[test]
        fn test_decode_200_ok_and_http_errors() {
            let body = r#"{"candidates":[{"content":{"parts":[{"text":"ok"}]}}],"modelVersion":"gemini-2.5-flash-lite-preview-06-17"}"#;
            let res =
                decode_gemini_probe_response(200, body, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, 118);
            assert_eq!(res.status, ProbeStatus::Ok);
            assert_eq!(res.latency_ms, 118);
            assert_eq!(
                res.resolved_model.as_deref(),
                Some("gemini-2.5-flash-lite-preview-06-17")
            );
            assert_eq!(res.error, None);

            let fallback =
                decode_gemini_probe_response(200, r#"{}"#, DEFAULT_GOOGLE_GENERATOR_MODEL, 95);
            assert_eq!(fallback.resolved_model.as_deref(), Some("gemini-4-carbon"));

            let r404 = decode_gemini_probe_response(
                404,
                r#"{"error":{"message":"models/gemini-nonexistent is not found"}}"#,
                "google:gemini-nonexistent",
                40,
            );
            assert_eq!(r404.status, ProbeStatus::Error);
            assert_eq!(r404.resolved_model, None);
            assert!(r404.error.expect("404").contains("--<node>-model"));
        }
    }

    mod deduplication_and_concurrent_execution {
        use super::*;

        #[test]
        fn test_deduplicates_five_google_nodes_into_two_concurrent_targets_in_canonical_order() {
            let mut responses = BTreeMap::new();
            responses.insert(
                DEFAULT_GOOGLE_DISCRIMINATOR_MODEL.to_string(),
                ProbeEndpointResult {
                    status: ProbeStatus::Ok,
                    latency_ms: 88,
                    resolved_model: Some("gemini-decision-1".to_string()),
                    error: None,
                },
            );
            responses.insert(
                DEFAULT_GOOGLE_GENERATOR_MODEL.to_string(),
                ProbeEndpointResult {
                    status: ProbeStatus::Ok,
                    latency_ms: 195,
                    resolved_model: Some("gemini-4-carbon".to_string()),
                    error: None,
                },
            );
            let prober = StubEndpointProber::new(responses);
            let caseload = Caseload::new().with_config(CaseloadConfig::default());
            let opts = ProbeOptions {
                api_key: "synthetic-test-key".to_string(),
                typesafe_api_key: None,
                timeout_ms: DEFAULT_PROBE_TIMEOUT_MS,
            };

            let result = execute_probe(&opts, &prober, caseload).expect("execute_probe");
            assert_eq!(prober.calls(), 2);
            let probe = result.probe.expect("probe populated");
            assert!(probe.is_healthy());
            assert_eq!(probe.targets.len(), 2);
            assert_eq!(probe.targets[0].model, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL);
            assert_eq!(
                probe.targets[0].nodes,
                vec![
                    HeuristicNode::Apprise,
                    HeuristicNode::Docket,
                    HeuristicNode::Admit,
                    HeuristicNode::Vet,
                ]
            );
            assert_eq!(probe.targets[1].model, DEFAULT_GOOGLE_GENERATOR_MODEL);
            assert_eq!(probe.targets[1].nodes, vec![HeuristicNode::Audit]);
        }

        #[test]
        fn test_typesafe_sole_provider_deduplicates_four_nodes_into_single_target() {
            let prober = StubEndpointProber::new(BTreeMap::new());
            let mut nodes = BTreeMap::new();
            for node in [
                HeuristicNode::Apprise,
                HeuristicNode::Docket,
                HeuristicNode::Admit,
                HeuristicNode::Vet,
            ] {
                nodes.insert(
                    node,
                    ResolvedNodeConfig::new(DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL),
                );
            }
            let cfg = CaseloadConfig {
                provider: "typesafe".to_string(),
                nodes,
            };
            let result = execute_probe(
                &ProbeOptions {
                    api_key: String::new(),
                    typesafe_api_key: Some("ts-key".to_string()),
                    timeout_ms: 2000,
                },
                &prober,
                Caseload::new().with_config(cfg),
            )
            .expect("typesafe-only probe succeeds without google key");

            assert_eq!(prober.calls(), 1);
            let probe = result.probe.expect("probe");
            assert!(probe.is_healthy());
            assert_eq!(probe.targets.len(), 1);
            assert_eq!(probe.targets[0].model, DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL);
            assert_eq!(probe.targets[0].nodes.len(), 4);
        }

        #[test]
        fn test_non_throwing_capture_and_fast_forward_invariants() {
            let mut responses = BTreeMap::new();
            responses.insert(
                DEFAULT_GOOGLE_GENERATOR_MODEL.to_string(),
                ProbeEndpointResult {
                    status: ProbeStatus::Error,
                    latency_ms: 5001,
                    resolved_model: None,
                    error: Some(
                        "Request timed out after 5000ms. Hint: Increase `--timeout`.".to_string(),
                    ),
                },
            );
            let prober = StubEndpointProber::new(responses);
            let enriched = execute_probe(
                &ProbeOptions {
                    api_key: "key".to_string(),
                    typesafe_api_key: None,
                    timeout_ms: DEFAULT_PROBE_TIMEOUT_MS,
                },
                &prober,
                Caseload::new()
                    .with_intake(CaseloadIntake::default())
                    .with_discovery(CaseloadDiscovery::default())
                    .with_validation(CaseloadValidation::default())
                    .with_config(CaseloadConfig::default()),
            )
            .expect("non-throwing capture");

            assert_eq!(prober.calls(), 2);
            let probe = enriched.probe.clone().expect("probe");
            assert!(!probe.is_healthy());
            assert_eq!(probe.targets[0].status, ProbeStatus::Ok);
            assert_eq!(probe.targets[1].status, ProbeStatus::Error);

            // Fast-forward when .probe is already present
            let ff = execute_probe(&ProbeOptions::default(), &prober, enriched.clone())
                .expect("fast-forward");
            assert_eq!(prober.calls(), 2);
            assert_eq!(ff, enriched);
        }

        #[test]
        fn test_multi_provider_prober_routes_gemini_decision_to_decisions_and_carbon_to_generate_content()
         {
            use std::io::{Read, Write};
            use std::net::TcpListener;
            use std::sync::{Arc, Mutex};
            use std::thread;
            use std::time::Duration;

            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");
            let seen_paths = Arc::new(Mutex::new(Vec::new()));
            let seen_for_server = Arc::clone(&seen_paths);

            let server = thread::spawn(move || {
                for _ in 0..2 {
                    let (mut stream, _) = listener.accept().expect("accept probe connection");
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .expect("read timeout");
                    let mut raw = Vec::new();
                    let mut buf = [0_u8; 4096];
                    let head = loop {
                        let n = stream.read(&mut buf).expect("read");
                        assert!(n > 0);
                        raw.extend_from_slice(&buf[..n]);
                        let text = String::from_utf8_lossy(&raw);
                        if let Some((h, body)) = text.split_once("\r\n\r\n") {
                            let expected_len = h
                                .lines()
                                .find_map(|l| {
                                    let (k, v) = l.split_once(':')?;
                                    k.eq_ignore_ascii_case("content-length")
                                        .then(|| v.trim().parse::<usize>().ok())
                                        .flatten()
                                })
                                .unwrap_or(0);
                            if body.len() >= expected_len {
                                break h.to_string();
                            }
                        }
                    };
                    let req_line = head.lines().next().unwrap_or("").to_string();
                    seen_for_server.lock().unwrap().push(req_line.clone());

                    let resp_body = if req_line.contains("/v1beta/decisions") {
                        r#"{"model":"gemini-decision-1","answers":{"ping":{"type":"noul","noul":0.99}}}"#
                    } else {
                        r#"{"modelVersion":"gemini-4-carbon"}"#
                    };
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        resp_body.len(),
                        resp_body
                    );
                    stream.write_all(resp.as_bytes()).expect("write");
                    stream.flush().expect("flush");
                }
            });

            let prober = MultiProviderEndpointProber::with_base_urls(
                format!("http://{addr}/v1beta"),
                format!("http://{addr}/v1"),
            );
            let out = execute_probe(
                &ProbeOptions {
                    api_key: "synthetic-gemini-key".to_string(),
                    typesafe_api_key: None,
                    timeout_ms: 2000,
                },
                &prober,
                Caseload::new().with_config(CaseloadConfig::default()),
            )
            .expect("live loopback probe succeeds");

            server.join().expect("server joined");
            let probe = out.probe.expect("probe populated");
            assert!(probe.is_healthy());
            assert_eq!(
                probe.targets[0].resolved_model.as_deref(),
                Some("gemini-decision-1")
            );
            assert_eq!(
                probe.targets[1].resolved_model.as_deref(),
                Some("gemini-4-carbon")
            );

            let mut paths = seen_paths.lock().unwrap().clone();
            paths.sort();
            assert_eq!(
                paths,
                vec![
                    "POST /v1beta/decisions HTTP/1.1".to_string(),
                    "POST /v1beta/models/gemini-4-carbon:generateContent HTTP/1.1".to_string(),
                ]
            );
        }
    }
}
