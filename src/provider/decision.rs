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

//! Hexagonal outbound port, protocol-bound decision budget profiles, and table-driven multi-profile routing.

use super::gemini::strip_provider_prefix;
use super::gemini_decisions::GeminiDecisionProvider;
use super::requester::{RetryObserver, SharedHttpRequester};
use super::retry::RetryPolicy;
pub use super::typesafe::TypesafeDecisionProvider;
use crate::caseload::{DEFAULT_PROVIDER, LOCAL_PROVIDER, TYPESAFE_PROVIDER};
use crate::configure::{
    DEFAULT_GEMINI_BASE_URL, DEFAULT_TYPESAFE_BASE_URL, ProtocolDriver, ProviderAuth,
    ProviderConnection, is_discriminative_model, is_gemini_decision_model_name, model_provider,
    provider_capability,
};
use crate::provider::{
    GeminiModelProvider, GenerationRequest, GenerationResponse, ProviderError,
    StructuredModelProvider, TokenUsage,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Default maximum concurrent in-flight heuristic calls on built-in `local:*` nodes (`configure.md` §4.3).
pub const DEFAULT_LOCAL_MAX_CONCURRENT_CALLS: usize = 1;

/// Resolves the profile-calibrated default stage concurrency (`1` for built-in `local:*`, else `cloud_default`).
pub fn default_concurrency_for_model(model_specifier: &str, cloud_default: usize) -> usize {
    if model_provider(model_specifier) == LOCAL_PROVIDER {
        DEFAULT_LOCAL_MAX_CONCURRENT_CALLS
    } else {
        cloud_default
    }
}

/// Per-protocol context character and `Noul` question-batching budget profile (`docs/architecture/nodes/docket.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecisionBudgetProfile {
    /// Maximum shared `state` / `input` character budget for macro discriminative screening.
    pub max_state_chars: usize,
    /// Maximum combined `state` + `questions` character budget per forward pass.
    pub max_batch_chars: usize,
    /// Maximum `Noul` questions packed into a single forward pass (including bifurcated `vet` span localization).
    pub max_nouls_per_call: usize,
    /// Default screening batch size for `apprise`, `docket`, and `admit` when no explicit batch size is configured.
    pub default_screening_batch_size: usize,
}

impl DecisionBudgetProfile {
    /// Expanded profile for Gemini Decisions (`<slug>:gemini-decision-*`: `262,144` input tokens, `1,000` questions/call).
    pub const GEMINI_DECISION: Self = Self {
        max_state_chars: 240_000,
        max_batch_chars: 480_000,
        max_nouls_per_call: 256,
        default_screening_batch_size: 256,
    };

    /// Calibrated profile for cloud System One (`typesafe:jev-*` and custom `systemone` profiles).
    pub const TYPESAFE_JEV: Self = Self {
        max_state_chars: 36_000,
        max_batch_chars: 64_000,
        max_nouls_per_call: 48,
        default_screening_batch_size: 48,
    };

    /// Calibrated profile for built-in local System One daemons (`local:*`, e.g. `llama-server` `Kev-4B` on `32k` ctx):
    /// keeps screening batches small (`4` canons/pairs per call) while preserving `16` nouls/call for single-batch `vet` span localization.
    pub const LOCAL_SYSTEMONE: Self = Self {
        max_state_chars: 24_000,
        max_batch_chars: 32_000,
        max_nouls_per_call: 16,
        default_screening_batch_size: 4,
    };

    /// Resolves the deterministic [`DecisionBudgetProfile`] for a discriminative model specifier.
    pub fn for_model(specifier: &str) -> Self {
        if model_provider(specifier) == LOCAL_PROVIDER {
            return Self::LOCAL_SYSTEMONE;
        }
        let model_id = strip_provider_prefix(specifier);
        if is_gemini_decision_model_name(model_id) {
            Self::GEMINI_DECISION
        } else {
            Self::TYPESAFE_JEV
        }
    }
}

const MIN_QUESTION_BUDGET_CHARS: usize = 4_096;
const WIRE_KEY_OVERHEAD_CHARS: usize = 32;

/// Partitions discriminative `Noul` items into deterministic contiguous batches bounded by both
/// `max_per_call` and `(max_batch_chars - state_chars)`.
pub(crate) fn partition_decision_items<T: Clone>(
    items: &[T],
    state_chars: usize,
    max_per_call: usize,
    max_batch_chars: usize,
    item_text_len: impl Fn(&T) -> usize,
) -> Vec<Vec<T>> {
    if items.is_empty() {
        return Vec::new();
    }
    let count_cap = max_per_call.max(1);
    let question_budget = max_batch_chars
        .saturating_sub(state_chars)
        .max(MIN_QUESTION_BUDGET_CHARS);
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut current_chars = 0usize;

    for item in items {
        let item_chars = item_text_len(item).saturating_add(WIRE_KEY_OVERHEAD_CHARS);
        if !current.is_empty()
            && (current.len() >= count_cap || current_chars + item_chars > question_budget)
        {
            batches.push(current);
            current = Vec::new();
            current_chars = 0;
        }
        current_chars += item_chars;
        current.push(item.clone());
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

/// Provider-neutral input specification for batched binary `Noul` probability evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRequest {
    /// Canonical provider-prefixed model identifier (e.g. `"typesafe:jev-latest"` or `"local-dev:kev-0.8b"`).
    pub model_specifier: String,
    /// Shared input state evaluated in a single forward pass across all `questions`.
    pub state: String,
    /// Deterministic map from domain subject identifier to closed yes/no `Noul` instruction text.
    pub questions: BTreeMap<String, String>,
    /// Per-request timeout ceiling in milliseconds.
    pub timeout_ms: u64,
    /// Optional concise work-unit identifier (e.g. `canon_ref` or batch label) for per-call telemetry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_label: Option<String>,
}

/// Normalized calibrated probability scores and operational telemetry from a decision model pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionResponse {
    /// Calibrated affirmative probabilities in `[0.0, 1.0]` keyed by domain subject identifier.
    pub scores: BTreeMap<String, f64>,
    /// Concrete model version identifier reported by the upstream provider endpoint.
    pub resolved_model: String,
    /// End-to-end request roundtrip latency in milliseconds.
    pub latency_ms: u64,
    /// Token accounting metrics for out-of-band telemetry reporting.
    pub usage: TokenUsage,
}

/// Hexagonal outbound port for discriminative decision models (`Noul` binary probability passes).
pub trait DecisionModelProvider: Send + Sync {
    /// Evaluates all `request.questions` in parallel against `request.state` and returns calibrated
    /// `[0.0, 1.0]` affirmative probabilities.
    fn evaluate_decisions(
        &self,
        request: &DecisionRequest,
    ) -> Result<DecisionResponse, ProviderError>;

    /// Evaluates `request.questions` while streaming real-time retry/cooldown notifications to `on_retry`.
    fn evaluate_decisions_observed(
        &self,
        request: &DecisionRequest,
        _on_retry: Option<RetryObserver<'_>>,
    ) -> Result<DecisionResponse, ProviderError> {
        self.evaluate_decisions(request)
    }
}

#[derive(Debug, Clone)]
enum ProtocolAdapterEntry {
    Gemini {
        generator: Option<GeminiModelProvider>,
        decisions: Option<GeminiDecisionProvider>,
    },
    SystemOne(TypesafeDecisionProvider),
}

impl ProtocolAdapterEntry {
    fn from_connection(conn: &ProviderConnection, requester: &SharedHttpRequester) -> Self {
        match conn.protocol {
            ProtocolDriver::Gemini => Self::Gemini {
                generator: Some(GeminiModelProvider::from_connection(
                    conn,
                    requester.clone(),
                )),
                decisions: Some(GeminiDecisionProvider::from_connection(
                    conn,
                    requester.clone(),
                )),
            },
            ProtocolDriver::SystemOne => Self::SystemOne(
                TypesafeDecisionProvider::from_connection(conn, requester.clone()),
            ),
        }
    }

    fn clear_abort(&self) {
        match self {
            Self::Gemini {
                generator,
                decisions,
            } => {
                if let Some(g) = generator {
                    g.requester().clear_abort();
                }
                if let Some(d) = decisions {
                    d.requester().clear_abort();
                }
            }
            Self::SystemOne(sys) => sys.requester().clear_abort(),
        }
    }

    fn notify_abort(&self) {
        match self {
            Self::Gemini {
                generator,
                decisions,
            } => {
                if let Some(g) = generator {
                    g.requester().notify_abort();
                }
                if let Some(d) = decisions {
                    d.requester().notify_abort();
                }
            }
            Self::SystemOne(sys) => sys.requester().notify_abort(),
        }
    }
}

/// Table-driven composite provider routing `<slug>:<model>` requests across registered [`ProtocolDriver`]s
/// (`gemini` `:generateContent` + `/decisions`, and authenticated or keyless `systemone` `/v1/systemone`).
#[derive(Debug, Clone)]
pub struct MultiModelProvider {
    adapters: BTreeMap<String, ProtocolAdapterEntry>,
}

impl MultiModelProvider {
    /// Constructs a [`MultiModelProvider`] from resolved [`ProviderConnection`]s sharing a single [`SharedHttpRequester`].
    pub fn from_connections(
        connections: BTreeMap<String, ProviderConnection>,
        retry_policy: RetryPolicy,
    ) -> Self {
        Self::from_connections_with_requester(connections, SharedHttpRequester::new(retry_policy))
    }

    /// Constructs a [`MultiModelProvider`] from resolved [`ProviderConnection`]s and a caller-supplied [`SharedHttpRequester`].
    pub fn from_connections_with_requester(
        connections: BTreeMap<String, ProviderConnection>,
        requester: SharedHttpRequester,
    ) -> Self {
        let mut adapters = BTreeMap::new();
        for (slug, conn) in connections {
            adapters.insert(
                slug,
                ProtocolAdapterEntry::from_connection(&conn, &requester),
            );
        }
        Self { adapters }
    }

    /// Constructs a [`MultiModelProvider`] from optional built-in Gemini and TypeSafe credentials.
    pub fn new(gemini_api_key: Option<String>, typesafe_api_key: Option<String>) -> Self {
        Self::with_retry_policy(gemini_api_key, typesafe_api_key, RetryPolicy::default())
    }

    /// Constructs a [`MultiModelProvider`] sharing a centralized [`SharedHttpRequester`] configured with `retry_policy`.
    pub fn with_retry_policy(
        gemini_api_key: Option<String>,
        typesafe_api_key: Option<String>,
        retry_policy: RetryPolicy,
    ) -> Self {
        Self::with_requester(
            gemini_api_key,
            typesafe_api_key,
            SharedHttpRequester::new(retry_policy),
        )
    }

    /// Constructs a [`MultiModelProvider`] sharing a caller-supplied [`SharedHttpRequester`].
    pub fn with_requester(
        gemini_api_key: Option<String>,
        typesafe_api_key: Option<String>,
        requester: SharedHttpRequester,
    ) -> Self {
        let mut connections = BTreeMap::new();
        if let Some(key) = gemini_api_key.filter(|k| !k.trim().is_empty()) {
            connections.insert(
                DEFAULT_PROVIDER.to_string(),
                ProviderConnection {
                    slug: DEFAULT_PROVIDER.to_string(),
                    protocol: ProtocolDriver::Gemini,
                    base_url: DEFAULT_GEMINI_BASE_URL.to_string(),
                    auth: ProviderAuth::GoogApiKey(key),
                },
            );
        }
        if let Some(key) = typesafe_api_key.filter(|k| !k.trim().is_empty()) {
            connections.insert(
                TYPESAFE_PROVIDER.to_string(),
                ProviderConnection {
                    slug: TYPESAFE_PROVIDER.to_string(),
                    protocol: ProtocolDriver::SystemOne,
                    base_url: DEFAULT_TYPESAFE_BASE_URL.to_string(),
                    auth: ProviderAuth::Bearer(key),
                },
            );
        }
        Self::from_connections_with_requester(connections, requester)
    }

    /// Constructs a [`MultiModelProvider`] from pre-configured built-in adapters (for loopback tests).
    pub fn with_providers(
        gemini: Option<GeminiModelProvider>,
        gemini_decisions: Option<GeminiDecisionProvider>,
        typesafe: Option<TypesafeDecisionProvider>,
    ) -> Self {
        let mut adapters = BTreeMap::new();
        if gemini.is_some() || gemini_decisions.is_some() {
            let slug = gemini
                .as_ref()
                .map(|g| g.slug().to_string())
                .or_else(|| gemini_decisions.as_ref().map(|d| d.slug().to_string()))
                .unwrap_or_else(|| DEFAULT_PROVIDER.to_string());
            adapters.insert(
                slug,
                ProtocolAdapterEntry::Gemini {
                    generator: gemini,
                    decisions: gemini_decisions,
                },
            );
        }
        if let Some(ts) = typesafe {
            adapters.insert(ts.slug().to_string(), ProtocolAdapterEntry::SystemOne(ts));
        }
        Self { adapters }
    }
}

fn unconfigured_slug_error(slug: &str, model_specifier: &str) -> ProviderError {
    if provider_capability(slug).is_some() {
        ProviderError::missing_api_key_for_slug(slug)
    } else {
        ProviderError::UnsupportedProvider {
            provider: slug.to_string(),
            model_specifier: model_specifier.to_string(),
        }
    }
}

impl DecisionModelProvider for MultiModelProvider {
    fn evaluate_decisions(
        &self,
        request: &DecisionRequest,
    ) -> Result<DecisionResponse, ProviderError> {
        <Self as DecisionModelProvider>::evaluate_decisions_observed(self, request, None)
    }

    fn evaluate_decisions_observed(
        &self,
        request: &DecisionRequest,
        on_retry: Option<RetryObserver<'_>>,
    ) -> Result<DecisionResponse, ProviderError> {
        let slug = model_provider(&request.model_specifier);
        let Some(entry) = self.adapters.get(slug) else {
            return Err(unconfigured_slug_error(slug, &request.model_specifier));
        };
        match entry {
            ProtocolAdapterEntry::Gemini { decisions, .. } => {
                if !is_discriminative_model(&request.model_specifier) {
                    return Err(ProviderError::InvalidModelSpecifier {
                        model_specifier: request.model_specifier.clone(),
                        reason: "generative Gemini models do not support discriminative `Noul` evaluation"
                            .to_string(),
                    });
                }
                let adapter = decisions
                    .as_ref()
                    .ok_or_else(|| ProviderError::missing_api_key_for_slug(slug))?;
                adapter.evaluate_decisions_observed(request, on_retry)
            }
            ProtocolAdapterEntry::SystemOne(adapter) => {
                adapter.evaluate_decisions_observed(request, on_retry)
            }
        }
    }
}

impl StructuredModelProvider for MultiModelProvider {
    fn generate_structured(
        &self,
        request: &GenerationRequest,
    ) -> Result<GenerationResponse, ProviderError> {
        self.generate_structured_observed(request, None)
    }

    fn generate_structured_observed(
        &self,
        request: &GenerationRequest,
        on_retry: Option<RetryObserver<'_>>,
    ) -> Result<GenerationResponse, ProviderError> {
        let slug = model_provider(&request.model_specifier);
        let Some(entry) = self.adapters.get(slug) else {
            if provider_capability(slug).is_some_and(|c| !c.protocol.supports_generator()) {
                return Err(ProviderError::UnsupportedProvider {
                    provider: slug.to_string(),
                    model_specifier: request.model_specifier.clone(),
                });
            }
            return Err(unconfigured_slug_error(slug, &request.model_specifier));
        };
        match entry {
            ProtocolAdapterEntry::Gemini { generator, .. } => {
                if is_discriminative_model(&request.model_specifier) {
                    return Err(ProviderError::InvalidModelSpecifier {
                        model_specifier: request.model_specifier.clone(),
                        reason: "discriminative Gemini Decisions models use `/v1beta/decisions` (`evaluate_decisions`) rather than `:generateContent`".to_string(),
                    });
                }
                let adapter = generator
                    .as_ref()
                    .ok_or_else(|| ProviderError::missing_api_key_for_slug(slug))?;
                adapter.generate_structured_observed(request, on_retry)
            }
            ProtocolAdapterEntry::SystemOne(_) => Err(ProviderError::UnsupportedProvider {
                provider: slug.to_string(),
                model_specifier: request.model_specifier.clone(),
            }),
        }
    }

    fn evaluate_decisions(
        &self,
        request: &DecisionRequest,
    ) -> Result<DecisionResponse, ProviderError> {
        DecisionModelProvider::evaluate_decisions(self, request)
    }

    fn evaluate_decisions_observed(
        &self,
        request: &DecisionRequest,
        on_retry: Option<RetryObserver<'_>>,
    ) -> Result<DecisionResponse, ProviderError> {
        DecisionModelProvider::evaluate_decisions_observed(self, request, on_retry)
    }

    fn clear_abort(&self) {
        for entry in self.adapters.values() {
            entry.clear_abort();
        }
    }

    fn notify_abort(&self) {
        for entry in self.adapters.values() {
            entry.notify_abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    mod budget_profiles_and_partitioning {
        use super::*;

        #[test]
        fn test_partition_decision_items_and_protocol_budget_profiles() {
            assert_eq!(
                DecisionBudgetProfile::for_model("google-corp:gemini-decision-1"),
                DecisionBudgetProfile::GEMINI_DECISION
            );
            assert_eq!(
                DecisionBudgetProfile::for_model("local-dev:kev-0.8b"),
                DecisionBudgetProfile::TYPESAFE_JEV
            );
            assert_eq!(
                DecisionBudgetProfile::for_model("local:kev-4b"),
                DecisionBudgetProfile::LOCAL_SYSTEMONE
            );
            assert_eq!(default_concurrency_for_model("local:kev-4b", 4), 1);
            assert_eq!(default_concurrency_for_model("typesafe:jev-latest", 4), 4);
            assert_eq!(
                default_concurrency_for_model("google:gemini-decision-1", 4),
                4
            );

            let items: Vec<(String, String)> = (0..5)
                .map(|i| (format!("c_{i}"), "x".repeat(2_900)))
                .collect();
            let by_chars = partition_decision_items(&items, 90_000, 48, 96_000, |(_, q)| q.len());
            assert_eq!(by_chars.len(), 3);
            assert_eq!(by_chars[0].len(), 2);
            assert_eq!(by_chars[1].len(), 2);
            assert_eq!(by_chars[2].len(), 1);

            let by_count = partition_decision_items(&items, 1_000, 2, 96_000, |(_, q)| q.len());
            assert_eq!(by_count.len(), 3);
            assert_eq!(by_count[0].len(), 2);
            assert_eq!(by_count[1].len(), 2);
            assert_eq!(by_count[2].len(), 1);
        }
    }

    mod custom_profile_loopback_routing {
        use super::*;
        use serde_json::json;

        fn read_full_http_request(stream: &mut TcpStream) -> (String, String) {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut raw = Vec::new();
            let mut buf = [0_u8; 4096];
            loop {
                let n = stream.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&raw);
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let expected = head
                        .lines()
                        .find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            k.eq_ignore_ascii_case("content-length")
                                .then(|| v.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                    if body.len() >= expected {
                        return (head.to_string(), body[..expected].to_string());
                    }
                }
            }
            (String::new(), String::new())
        }

        fn write_json_200(stream: &mut TcpStream, body: &str) {
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }

        fn spawn_single_json_200_server(
            body: &'static str,
        ) -> (std::net::SocketAddr, thread::JoinHandle<(String, String)>) {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("addr");
            let handle = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept");
                let req = read_full_http_request(&mut stream);
                write_json_200(&mut stream, body);
                req
            });
            (addr, handle)
        }

        #[test]
        fn test_multi_model_provider_routes_keyless_and_authenticated_custom_systemone() {
            let (local_addr, local_srv) = spawn_single_json_200_server(
                r#"{"model":"kev-0.8b-q8_0","answers":{"q_0":{"type":"noul","noul":0.93}},"usage":{"input_tokens":42,"output_tokens":0}}"#,
            );
            let (lan_addr, lan_srv) = spawn_single_json_200_server(
                r#"{"model":"clef-flash","answers":{"q_0":{"type":"noul","noul":0.88}},"usage":{"input_tokens":50,"output_tokens":0}}"#,
            );

            let connections = BTreeMap::from([
                (
                    "local-dev".to_string(),
                    ProviderConnection {
                        slug: "local-dev".to_string(),
                        protocol: ProtocolDriver::SystemOne,
                        base_url: format!("http://{local_addr}"),
                        auth: ProviderAuth::None,
                    },
                ),
                (
                    "lan-gpu".to_string(),
                    ProviderConnection {
                        slug: "lan-gpu".to_string(),
                        protocol: ProtocolDriver::SystemOne,
                        base_url: format!("http://{lan_addr}/v1"),
                        auth: ProviderAuth::Bearer("lan-secret-token".to_string()),
                    },
                ),
            ]);
            let provider =
                MultiModelProvider::from_connections(connections, RetryPolicy::new(0, 0));

            let local_res = DecisionModelProvider::evaluate_decisions(
                &provider,
                &DecisionRequest {
                    model_specifier: "local-dev:kev-0.8b".to_string(),
                    state: "diff".to_string(),
                    questions: BTreeMap::from([("rule.md".to_string(), "Applies?".to_string())]),
                    timeout_ms: 2_000,
                    call_label: None,
                },
            )
            .expect("keyless local-dev systemone succeeds");
            assert_eq!(local_res.scores["rule.md"], 0.93);
            assert_eq!(local_res.resolved_model, "kev-0.8b-q8_0");

            let lan_res = DecisionModelProvider::evaluate_decisions(
                &provider,
                &DecisionRequest {
                    model_specifier: "lan-gpu:clef-flash".to_string(),
                    state: "diff".to_string(),
                    questions: BTreeMap::from([("rule.md".to_string(), "Applies?".to_string())]),
                    timeout_ms: 2_000,
                    call_label: None,
                },
            )
            .expect("authenticated lan-gpu systemone succeeds");
            assert_eq!(lan_res.scores["rule.md"], 0.88);

            let (local_head, local_body) = local_srv.join().expect("local joined");
            assert!(local_head.starts_with("POST /v1/systemone HTTP/1.1"));
            assert!(!local_head.to_ascii_lowercase().contains("authorization:"));
            assert!(local_body.contains("\"model\":\"kev-0.8b\""));

            let (lan_head, lan_body) = lan_srv.join().expect("lan joined");
            assert!(lan_head.starts_with("POST /v1/systemone HTTP/1.1"));
            assert!(
                lan_head
                    .to_ascii_lowercase()
                    .contains("authorization: bearer lan-secret-token")
            );
            assert!(lan_body.contains("\"model\":\"clef-flash\""));
        }

        #[test]
        fn test_multi_model_provider_routes_custom_gemini_decisions_and_generator() {
            let corp_listener = TcpListener::bind("127.0.0.1:0").expect("bind corp");
            let corp_addr = corp_listener.local_addr().expect("addr");
            let corp_srv = thread::spawn(move || {
                let mut records = Vec::new();
                for _ in 0..2 {
                    let (mut stream, _) = corp_listener.accept().expect("accept corp");
                    let (head, body) = read_full_http_request(&mut stream);
                    let resp = if head.contains("/v1beta/decisions") {
                        r#"{"model":"gemini-decision-1","answers":{"q_0":{"type":"noul","noul":0.97}}}"#
                    } else {
                        r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"{\"compliance_score\":0.96}"}]},"finishReason":"STOP"}],"modelVersion":"gemini-4-carbon"}"#
                    };
                    write_json_200(&mut stream, resp);
                    records.push((head, body));
                }
                records
            });

            let connections = BTreeMap::from([(
                "google-corp".to_string(),
                ProviderConnection {
                    slug: "google-corp".to_string(),
                    protocol: ProtocolDriver::Gemini,
                    base_url: format!("http://{corp_addr}/v1beta"),
                    auth: ProviderAuth::GoogApiKey("corp-gemini-secret".to_string()),
                },
            )]);
            let provider =
                MultiModelProvider::from_connections(connections, RetryPolicy::new(0, 0));

            let corp_dec = DecisionModelProvider::evaluate_decisions(
                &provider,
                &DecisionRequest {
                    model_specifier: "google-corp:gemini-decision-1".to_string(),
                    state: "diff".to_string(),
                    questions: BTreeMap::from([("rule.md".to_string(), "Complies?".to_string())]),
                    timeout_ms: 2_000,
                    call_label: None,
                },
            )
            .expect("google-corp gemini-decision-1 succeeds");
            assert_eq!(corp_dec.scores["rule.md"], 0.97);

            let corp_gen = provider
                .generate_structured(&GenerationRequest {
                    model_specifier: "google-corp:gemini-4-carbon".to_string(),
                    system_instruction: "Judge".to_string(),
                    user_prompt: "Exhibit".to_string(),
                    response_schema: json!({"type": "object"}),
                    thinking_level: None,
                    reasoning_budget: None,
                    timeout_ms: 2_000,
                    call_label: None,
                })
                .expect("google-corp gemini-4-carbon succeeds");
            assert_eq!(corp_gen.payload["compliance_score"], json!(0.96));

            let corp_records = corp_srv.join().expect("corp joined");
            assert_eq!(corp_records.len(), 2);
            assert!(
                corp_records[0]
                    .0
                    .starts_with("POST /v1beta/decisions HTTP/1.1")
            );
            assert!(
                corp_records[0]
                    .0
                    .to_ascii_lowercase()
                    .contains("x-goog-api-key: corp-gemini-secret")
            );
            assert!(
                corp_records[1]
                    .0
                    .starts_with("POST /v1beta/models/gemini-4-carbon:generateContent HTTP/1.1")
            );
        }
    }
}
