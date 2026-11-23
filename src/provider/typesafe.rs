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

//! System One (`POST /v1/systemone`) wire transport, `Noul` helpers, and [`TypesafeDecisionProvider`].

use super::decision::{DecisionModelProvider, DecisionRequest, DecisionResponse};
use super::gemini::strip_provider_prefix;
use super::requester::{RetryObserver, RetryablePostRequest, SharedHttpRequester};
use super::retry::RetryPolicy;
use super::runtime::{HttpAuth, JsonPostRequest, RawHttpOutcome, execute_json_post};
use super::typesafe_wire::{
    TypesafeNoulQuestion, TypesafeSystemOneRequest, TypesafeSystemOneResponse,
    build_typesafe_http_status_error,
};
#[cfg(test)]
use super::typesafe_wire::{classify_typesafe_http_error, extract_typesafe_error_detail};
use crate::caseload::TYPESAFE_PROVIDER;
pub use crate::configure::DEFAULT_TYPESAFE_BASE_URL;
use crate::configure::{AuthPolicy, ProviderAuth, ProviderConnection};
use crate::provider::{ProviderError, TokenUsage};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;

/// Constructs and normalizes a System One `POST .../systemone` URL from a base URL (`transport.md` §2.3):
/// - Bare origins (`http://127.0.0.1:8080` or `http://127.0.0.1:8080/`) normalize to `{origin}/v1/systemone`.
/// - Explicit paths (`https://api.typesafe.ai/v1` or `http://127.0.0.1:8080/v1`) normalize to `{base}/systemone`.
/// - URLs already ending in `/systemone` are preserved as-is.
pub fn build_systemone_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    let is_bare_origin = match base.split_once("://") {
        Some((_, rest)) => !rest.contains('/'),
        None => false,
    };
    if is_bare_origin {
        format!("{base}/v1/systemone")
    } else if base.ends_with("/systemone") {
        base.to_string()
    } else {
        format!("{base}/systemone")
    }
}

pub(crate) fn post_typesafe_json<T: Serialize>(
    client: &reqwest::Client,
    base_url: &str,
    model_specifier: &str,
    auth: HttpAuth<'_>,
    timeout_ms: u64,
    payload: &T,
) -> Result<RawHttpOutcome, (ProviderError, u64)> {
    let url = build_systemone_url(base_url);
    let req = JsonPostRequest {
        url: &url,
        model_specifier,
        auth,
        timeout_ms,
    };
    execute_json_post(client, &req, payload)
}

/// Live System One HTTP/HTTPS adapter implementing [`DecisionModelProvider`] (`POST {baseUrl}/systemone`).
#[derive(Clone)]
pub struct TypesafeDecisionProvider {
    slug: String,
    auth: ProviderAuth,
    auth_policy: AuthPolicy,
    base_url: String,
    requester: SharedHttpRequester,
}

impl fmt::Debug for TypesafeDecisionProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key_repr = if self.auth.has_secret() {
            "<redacted>"
        } else {
            "<empty>"
        };
        f.debug_struct("TypesafeDecisionProvider")
            .field("slug", &self.slug)
            .field("api_key", &key_repr)
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl TypesafeDecisionProvider {
    /// Constructs a [`TypesafeDecisionProvider`] targeting [`DEFAULT_TYPESAFE_BASE_URL`].
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_base_url(api_key, DEFAULT_TYPESAFE_BASE_URL)
    }

    /// Constructs a [`TypesafeDecisionProvider`] with a custom base URL (for loopback tests or proxies).
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self::with_requester_and_base_url(api_key, SharedHttpRequester::default(), base_url)
    }

    /// Constructs a [`TypesafeDecisionProvider`] with a caller-supplied reusable [`reqwest::Client`].
    pub fn with_client(api_key: impl Into<String>, client: reqwest::Client) -> Self {
        Self::with_client_and_base_url(api_key, client, DEFAULT_TYPESAFE_BASE_URL)
    }

    /// Constructs a [`TypesafeDecisionProvider`] with a custom [`reqwest::Client`] and base URL.
    pub fn with_client_and_base_url(
        api_key: impl Into<String>,
        client: reqwest::Client,
        base_url: impl Into<String>,
    ) -> Self {
        Self::with_requester_and_base_url(
            api_key,
            SharedHttpRequester::with_client(client, RetryPolicy::default()),
            base_url,
        )
    }

    /// Constructs a [`TypesafeDecisionProvider`] sharing an explicit [`SharedHttpRequester`].
    pub fn with_requester(api_key: impl Into<String>, requester: SharedHttpRequester) -> Self {
        Self::with_requester_and_base_url(api_key, requester, DEFAULT_TYPESAFE_BASE_URL)
    }

    /// Constructs a [`TypesafeDecisionProvider`] sharing an explicit [`SharedHttpRequester`] and custom base URL.
    pub fn with_requester_and_base_url(
        api_key: impl Into<String>,
        requester: SharedHttpRequester,
        base_url: impl Into<String>,
    ) -> Self {
        let raw_key = api_key.into();
        let trimmed = raw_key.trim();
        let auth = if trimmed.is_empty() {
            ProviderAuth::None
        } else {
            ProviderAuth::Bearer(trimmed.to_string())
        };
        Self {
            slug: TYPESAFE_PROVIDER.to_string(),
            auth,
            auth_policy: AuthPolicy::RequiredApiKey,
            base_url: base_url.into(),
            requester,
        }
    }

    /// Constructs a [`TypesafeDecisionProvider`] from a resolved [`ProviderConnection`] (supporting keyless `ProviderAuth::None`).
    pub fn from_connection(conn: &ProviderConnection, requester: SharedHttpRequester) -> Self {
        let auth_policy = if conn.auth == ProviderAuth::None {
            AuthPolicy::OptionalApiKey
        } else {
            AuthPolicy::RequiredApiKey
        };
        Self {
            slug: conn.slug.clone(),
            auth: conn.auth.clone(),
            auth_policy,
            base_url: conn.base_url.clone(),
            requester,
        }
    }

    /// Returns the canonical profile slug bound to this adapter.
    pub fn slug(&self) -> &str {
        &self.slug
    }

    /// Returns the configured System One base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Returns a reference to the reusable, pooled [`reqwest::Client`].
    pub fn client(&self) -> &reqwest::Client {
        self.requester.client()
    }

    /// Returns a reference to the shared [`SharedHttpRequester`].
    pub fn requester(&self) -> &SharedHttpRequester {
        &self.requester
    }
}

impl DecisionModelProvider for TypesafeDecisionProvider {
    fn evaluate_decisions(
        &self,
        request: &DecisionRequest,
    ) -> Result<DecisionResponse, ProviderError> {
        self.evaluate_decisions_observed(request, None)
    }

    fn evaluate_decisions_observed(
        &self,
        request: &DecisionRequest,
        on_retry: Option<RetryObserver<'_>>,
    ) -> Result<DecisionResponse, ProviderError> {
        if request.timeout_ms == 0 {
            return Err(ProviderError::InvalidTimeout { timeout_ms: 0 });
        }

        if self.auth_policy == AuthPolicy::RequiredApiKey && !self.auth.has_secret() {
            return Err(ProviderError::missing_api_key_for_slug(&self.slug));
        }

        validate_systemone_specifier(&request.model_specifier, &self.slug)?;
        if request.questions.is_empty() {
            return Ok(DecisionResponse {
                scores: BTreeMap::new(),
                resolved_model: request.model_specifier.clone(),
                latency_ms: 0,
                usage: TokenUsage::default(),
            });
        }

        let mut wire_questions = BTreeMap::new();
        let mut wire_to_domain = Vec::with_capacity(request.questions.len());
        for (idx, (domain_key, instructions)) in request.questions.iter().enumerate() {
            let wire_key = format!("q_{idx}");
            wire_questions.insert(wire_key.clone(), TypesafeNoulQuestion::new(instructions));
            wire_to_domain.push((wire_key, domain_key.clone()));
        }

        let payload = TypesafeSystemOneRequest {
            model: strip_provider_prefix(request.model_specifier.trim()).to_string(),
            state: request.state.clone(),
            questions: wire_questions,
        };
        let url = build_systemone_url(&self.base_url);
        let outcome = self.requester.execute_json_post(
            &RetryablePostRequest {
                url: &url,
                model_specifier: &request.model_specifier,
                auth: HttpAuth::from_provider_auth(&self.auth),
                timeout_ms: request.timeout_ms,
                call_label: request.call_label.as_deref(),
                format_http_error: build_typesafe_http_status_error,
            },
            &payload,
            on_retry,
        )?;

        decode_systemone_response(
            &outcome.body,
            outcome.latency_ms,
            &request.model_specifier,
            &wire_to_domain,
        )
    }
}

fn validate_systemone_specifier(raw: &str, expected_slug: &str) -> Result<(), ProviderError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return Err(ProviderError::InvalidModelSpecifier {
            model_specifier: raw.to_string(),
            reason: "model specifier must be non-empty and contain no whitespace".to_string(),
        });
    }
    if let Some((prov, model_id)) = trimmed.split_once(':') {
        if prov.is_empty() || model_id.is_empty() || model_id.contains(':') {
            return Err(ProviderError::InvalidModelSpecifier {
                model_specifier: raw.to_string(),
                reason: "expected `<provider>:<model>` with non-empty segments".to_string(),
            });
        }
        if prov != expected_slug {
            return Err(ProviderError::UnsupportedProvider {
                provider: prov.to_string(),
                model_specifier: trimmed.to_string(),
            });
        }
    }
    Ok(())
}

fn decode_systemone_response(
    body: &str,
    latency_ms: u64,
    model_specifier: &str,
    wire_to_domain: &[(String, String)],
) -> Result<DecisionResponse, ProviderError> {
    let parsed: TypesafeSystemOneResponse =
        serde_json::from_str(body).map_err(|err| ProviderError::MalformedResponse {
            model_specifier: model_specifier.to_string(),
            reason: format!("invalid `/v1/systemone` JSON envelope ({err})"),
        })?;

    let mut scores = BTreeMap::new();
    for (wire_key, domain_key) in wire_to_domain {
        let answer = parsed.answers.get(wire_key).ok_or_else(|| {
            ProviderError::SchemaDecode {
                model_specifier: model_specifier.to_string(),
                message: format!(
                    "missing `noul` answer `{wire_key}` for subject `{domain_key}` in `/v1/systemone` response"
                ),
            }
        })?;
        if !answer.noul.is_finite() || !(-1e-6..=1.0 + 1e-6).contains(&answer.noul) {
            return Err(ProviderError::SchemaDecode {
                model_specifier: model_specifier.to_string(),
                message: format!(
                    "out-of-range `noul` probability `{}` for subject `{domain_key}`",
                    answer.noul
                ),
            });
        }
        scores.insert(domain_key.clone(), answer.noul.clamp(0.0, 1.0));
    }

    let resolved_model = parsed
        .model
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| model_specifier.to_string());

    let usage = match parsed.usage {
        Some(u) => TokenUsage {
            prompt_tokens: u.input_tokens,
            completion_tokens: u.output_tokens,
            reasoning_tokens: 0,
            total_tokens: u.input_tokens.saturating_add(u.output_tokens),
        },
        None => TokenUsage::default(),
    };

    Ok(DecisionResponse {
        scores,
        resolved_model,
        latency_ms,
        usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::runtime::build_pooled_http_client;

    #[test]
    fn test_url_prefix_and_error_classification_helpers() {
        assert_eq!(strip_provider_prefix("typesafe:jev-latest"), "jev-latest");
        assert_eq!(strip_provider_prefix("local-dev:kev-0.8b"), "kev-0.8b");
        assert_eq!(strip_provider_prefix("jev-1.14.0"), "jev-1.14.0");
        assert_eq!(
            build_systemone_url("https://api.typesafe.ai/v1/"),
            "https://api.typesafe.ai/v1/systemone"
        );
        assert_eq!(
            build_systemone_url("http://127.0.0.1:8080"),
            "http://127.0.0.1:8080/v1/systemone"
        );
        assert_eq!(
            build_systemone_url("http://127.0.0.1:8080/"),
            "http://127.0.0.1:8080/v1/systemone"
        );
        assert_eq!(
            build_systemone_url("http://127.0.0.1:8080/v1/systemone"),
            "http://127.0.0.1:8080/v1/systemone"
        );

        assert_eq!(
            extract_typesafe_error_detail(r#"{"error":{"message":"Invalid token"}}"#),
            "Invalid token"
        );
        assert_eq!(
            extract_typesafe_error_detail(r#"{"detail":"Unauthorized"}"#),
            "Unauthorized"
        );

        let diag401 = classify_typesafe_http_error(401, "typesafe:jev-latest", "Invalid token");
        assert!(diag401.contains("HTTP 401"));
        assert!(diag401.contains("TYPESAFE_API_KEY"));
        assert!(diag401.contains("Hint:"));
    }

    #[test]
    fn test_protocol_neutral_systemone_http_error_classification() {
        let diag501 =
            classify_typesafe_http_error(501, "local:kev-4b", "This model is not a decision model");
        assert!(diag501.contains("HTTP 501"));
        assert!(diag501.contains("decision GGUF"));
        assert!(!diag501.contains("TypeSafe AI"));

        let diag400_local = classify_typesafe_http_error(
            400,
            "local:kev-4b",
            "request (40000 tokens) exceeds the available context size (32768 tokens)",
        );
        assert!(diag400_local.contains("System One context limit"));
        assert!(diag400_local.contains("--ctx-size"));
        assert!(!diag400_local.contains("TypeSafe"));

        let diag503_local =
            classify_typesafe_http_error(503, "local:kev-4b", "Service Unavailable");
        assert!(diag503_local.contains("Upstream System One server error"));
        assert!(!diag503_local.contains("TypeSafe AI"));
    }

    #[test]
    fn test_systemone_noul_request_and_response_wire_round_trip() {
        let mut questions = BTreeMap::new();
        questions.insert(
            "ping".to_string(),
            TypesafeNoulQuestion::new("Is the input non-empty?"),
        );
        let req = TypesafeSystemOneRequest {
            model: "jev-latest".to_string(),
            state: "ping".to_string(),
            questions,
        };
        let encoded = serde_json::to_value(&req).expect("serialize request");
        assert_eq!(encoded["questions"]["ping"]["type"], "noul");

        let resp_raw = r#"{
            "model": "jev-1.14.0",
            "answers": {
                "ping": { "type": "noul", "noul": 0.99 }
            },
            "usage": { "input_tokens": 18, "output_tokens": 0 }
        }"#;
        let decoded: TypesafeSystemOneResponse =
            serde_json::from_str(resp_raw).expect("deserialize response");
        assert_eq!(decoded.model.as_deref(), Some("jev-1.14.0"));
        assert_eq!(decoded.answers["ping"].noul, 0.99);
    }

    #[test]
    fn test_decode_systemone_response_maps_wire_keys_to_domain_subjects() {
        let raw = r#"{
            "model": "jev-1.14.0",
            "answers": {
                "q_0": { "type": "noul", "noul": 0.94 },
                "q_1": { "type": "noul", "noul": 0.08 }
            },
            "usage": { "input_tokens": 410, "output_tokens": 0 }
        }"#;
        let mapping = vec![
            ("q_0".to_string(), ".canons/cli/rule-a.md".to_string()),
            ("q_1".to_string(), ".canons/cli/rule-b.md".to_string()),
        ];
        let decoded = decode_systemone_response(raw, 105, "typesafe:jev-latest", &mapping)
            .expect("decode valid systemone response");
        assert_eq!(decoded.resolved_model, "jev-1.14.0");
        assert_eq!(decoded.latency_ms, 105);
        assert_eq!(decoded.scores[".canons/cli/rule-a.md"], 0.94);
        assert_eq!(decoded.scores[".canons/cli/rule-b.md"], 0.08);
        assert_eq!(decoded.usage.prompt_tokens, 410);
        assert_eq!(decoded.usage.total_tokens, 410);
    }

    #[test]
    fn test_typesafe_provider_debug_redacts_secret_and_validates_inputs() {
        let secret = "super-secret-typesafe-token";
        let provider = TypesafeDecisionProvider::new(secret);
        let dbg = format!("{provider:?}");
        assert!(!dbg.contains(secret));
        assert!(dbg.contains("<redacted>"));

        let empty_provider = TypesafeDecisionProvider::new("  ");
        let req = DecisionRequest {
            model_specifier: "typesafe:jev-latest".to_string(),
            state: "diff".to_string(),
            questions: BTreeMap::from([("k".to_string(), "q?".to_string())]),
            timeout_ms: 5000,
            call_label: None,
        };
        assert_eq!(
            empty_provider.evaluate_decisions(&req),
            Err(ProviderError::missing_api_key_for_slug(TYPESAFE_PROVIDER))
        );
    }

    #[test]
    fn test_typesafe_provider_reuses_pooled_connection_across_distinct_threads() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;
        use std::time::Duration;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept single pooled socket");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");
            for idx in 1..=2 {
                let mut raw = Vec::new();
                let mut buf = [0_u8; 4096];
                loop {
                    let n = stream.read(&mut buf).expect("read request");
                    if n == 0 {
                        break;
                    }
                    raw.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&raw);
                    if let Some((head, body)) = text.split_once("\r\n\r\n") {
                        let expected_len = head
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        if body.len() >= expected_len {
                            break;
                        }
                    }
                }
                let score = if idx == 1 { 0.92 } else { 0.14 };
                let body = format!(
                    r#"{{"model":"jev-1.14.0","answers":{{"q_0":{{"type":"noul","noul":{score}}}}},"usage":{{"input_tokens":20,"output_tokens":0}}}}"#
                );
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(resp.as_bytes()).expect("write response");
                stream.flush().expect("flush response");
            }
        });

        let custom_client = build_pooled_http_client();
        let provider = TypesafeDecisionProvider::with_client_and_base_url(
            "synthetic-typesafe-key",
            custom_client,
            format!("http://{addr}/v1"),
        );
        assert!(provider.client().post("http://127.0.0.1").build().is_ok());

        let req = DecisionRequest {
            model_specifier: "typesafe:jev-latest".to_string(),
            state: "diff".to_string(),
            questions: BTreeMap::from([("rule".to_string(), "Is it colorable?".to_string())]),
            timeout_ms: 500,
            call_label: None,
        };

        let prov1 = provider.clone();
        let req1 = req.clone();
        let res1 = thread::spawn(move || prov1.evaluate_decisions(&req1))
            .join()
            .expect("worker 1 joined")
            .expect("call 1 succeeded");
        assert_eq!(res1.scores["rule"], 0.92);

        let res2 = thread::spawn(move || provider.evaluate_decisions(&req))
            .join()
            .expect("worker 2 joined")
            .expect("call 2 reused pooled socket after worker 1 exited");
        assert_eq!(res2.scores["rule"], 0.14);

        server.join().expect("server joined");
    }
}
