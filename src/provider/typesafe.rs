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

//! TypeSafe AI System One (`POST /v1/systemone`) wire transport, Jev `Noul` helpers, and [`TypesafeDecisionProvider`].

use super::decision::{DecisionModelProvider, DecisionRequest, DecisionResponse};
use super::requester::{RetryObserver, RetryablePostRequest, SharedHttpRequester};
use super::retry::RetryPolicy;
use super::runtime::{HttpAuth, JsonPostRequest, RawHttpOutcome, execute_json_post};
use crate::caseload::TYPESAFE_PROVIDER;
use crate::configure::ENV_TYPESAFE_API_KEY;
use crate::provider::{ProviderError, TokenUsage};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// Default TypeSafe AI v1 REST API base URL.
pub const DEFAULT_TYPESAFE_BASE_URL: &str = "https://api.typesafe.ai/v1";

/// Strips the canonical `"typesafe:"` provider prefix from a normalized model specifier.
pub(crate) fn strip_typesafe_prefix(model_specifier: &str) -> &str {
    match model_specifier.strip_prefix(TYPESAFE_PROVIDER) {
        Some(rest) => rest.strip_prefix(':').unwrap_or(model_specifier),
        None => model_specifier,
    }
}

/// Constructs the TypeSafe AI `POST /v1/systemone` URL from a base URL.
pub(crate) fn build_systemone_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    format!("{base}/systemone")
}

/// Typed `noul` binary decision question in a TypeSafe AI `/v1/systemone` request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TypesafeNoulQuestion {
    /// Question primitive discriminator (always `"noul"` for binary probability questions).
    #[serde(rename = "type")]
    pub(crate) kind: String,
    /// Closed yes/no instruction evaluated against `state`.
    pub(crate) instructions: String,
}

impl TypesafeNoulQuestion {
    /// Constructs a `"noul"` binary decision question with the given instructions.
    pub(crate) fn new(instructions: impl Into<String>) -> Self {
        Self {
            kind: "noul".to_string(),
            instructions: instructions.into(),
        }
    }
}

/// Request payload for `POST /v1/systemone`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TypesafeSystemOneRequest {
    /// Unprefixed Jev model identifier (e.g. `"jev-latest"`).
    pub(crate) model: String,
    /// Shared text state evaluated across all questions in a single forward pass.
    pub(crate) state: String,
    /// Deterministic map of question keys to `Noul` definitions.
    pub(crate) questions: BTreeMap<String, TypesafeNoulQuestion>,
}

/// Decoded answer entry for a single `noul` question in a `/v1/systemone` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct TypesafeNoulAnswer {
    /// Question primitive discriminator (`"noul"`).
    #[serde(rename = "type", default)]
    pub(crate) kind: Option<String>,
    /// Calibrated probability in `[0.0, 1.0]` that the answer is affirmative.
    pub(crate) noul: f64,
}

/// Token usage telemetry returned by TypeSafe AI `/v1/systemone`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TypesafeUsage {
    /// Prompt/input tokens billed for `state` and `questions`.
    #[serde(default)]
    pub(crate) input_tokens: u64,
    /// Output tokens reported by the endpoint (typically `0` or minimal framing).
    #[serde(default)]
    pub(crate) output_tokens: u64,
}

/// Response envelope returned by `POST /v1/systemone`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct TypesafeSystemOneResponse {
    /// Concrete Jev model identifier reported by the upstream endpoint.
    #[serde(default)]
    pub(crate) model: Option<String>,
    /// Keyed `noul` answers corresponding to `request.questions`.
    #[serde(default)]
    pub(crate) answers: BTreeMap<String, TypesafeNoulAnswer>,
    /// Optional token accounting metadata.
    #[serde(default)]
    pub(crate) usage: Option<TypesafeUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TypesafeErrorField {
    Object { message: Option<String> },
    Text(String),
}

#[derive(Debug, Deserialize)]
struct TypesafeErrorEnvelope {
    error: Option<TypesafeErrorField>,
    detail: Option<String>,
    message: Option<String>,
}

/// Extracts a human-readable upstream error detail from a TypeSafe AI JSON error body or raw text.
pub(crate) fn extract_typesafe_error_detail(body: &str) -> String {
    if let Ok(env) = serde_json::from_str::<TypesafeErrorEnvelope>(body) {
        let extracted = match env.error {
            Some(TypesafeErrorField::Object { message }) => message,
            Some(TypesafeErrorField::Text(text)) => Some(text),
            None => None,
        }
        .or(env.detail)
        .or(env.message)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

        if let Some(msg) = extracted {
            return msg;
        }
    }

    let trimmed = body.trim();
    if trimmed.is_empty() {
        "empty error response".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Formats an HTTP error status code from the TypeSafe AI API into an actionable diagnostic with remediation hints.
pub(crate) fn classify_typesafe_http_error(
    status_code: u16,
    model_specifier: &str,
    detail: &str,
) -> String {
    match status_code {
        400 if detail.contains("max_tokens_exceeded") => format!(
            "HTTP 400: Input context exceeded TypeSafe Jev token limit for `{model_specifier}` ({detail}). \
             Hint: Pair `--provider typesafe` with `--audit-model google:gemini-4-carbon` or narrow target scope."
        ),
        401 | 403 => format!(
            "HTTP {status_code}: Authentication or permission rejected for `{model_specifier}` ({detail}). \
             Hint: Verify `{ENV_TYPESAFE_API_KEY}`, `providers.typesafe.apiKey` in `~/.config/canon-clerk/config.json`, or `--typesafe-api-key`."
        ),
        404 => format!(
            "HTTP 404: Model or endpoint for `{model_specifier}` was not found ({detail}). \
             Hint: Specify a valid TypeSafe Jev model via `--provider typesafe` or `--<node>-model` (e.g. `typesafe:jev-latest`)."
        ),
        429 => format!(
            "HTTP 429: Quota or rate limit exceeded for `{model_specifier}` ({detail}). \
             Hint: Check your TypeSafe AI quota tier or wait before retrying."
        ),
        500..=599 => format!(
            "HTTP {status_code}: Upstream TypeSafe AI server error for `{model_specifier}` ({detail}). \
             Hint: Retry after a brief backoff or increase `--timeout <MS>`."
        ),
        _ => format!(
            "HTTP {status_code}: Request rejected for `{model_specifier}` ({detail}). \
             Hint: Check Jev model compatibility and TypeSafe AI configuration."
        ),
    }
}

fn build_typesafe_http_status_error(
    status_code: u16,
    model_specifier: &str,
    body: &str,
) -> ProviderError {
    let detail = extract_typesafe_error_detail(body);
    let diagnostic = classify_typesafe_http_error(status_code, model_specifier, &detail);
    ProviderError::HttpStatus {
        status_code,
        model_specifier: model_specifier.to_string(),
        detail,
        diagnostic,
    }
}

pub(crate) fn post_typesafe_json<T: Serialize>(
    client: &reqwest::Client,
    base_url: &str,
    model_specifier: &str,
    api_key: &str,
    timeout_ms: u64,
    payload: &T,
) -> Result<RawHttpOutcome, (ProviderError, u64)> {
    let url = build_systemone_url(base_url);
    let req = JsonPostRequest {
        url: &url,
        model_specifier,
        auth: HttpAuth::Bearer(api_key),
        timeout_ms,
    };
    execute_json_post(client, &req, payload)
}

/// Live TypeSafe AI HTTPS adapter implementing [`DecisionModelProvider`] (`POST /v1/systemone`).
#[derive(Clone)]
pub struct TypesafeDecisionProvider {
    api_key: String,
    base_url: String,
    requester: SharedHttpRequester,
}

impl fmt::Debug for TypesafeDecisionProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key_repr = if self.api_key.trim().is_empty() {
            "<empty>"
        } else {
            "<redacted>"
        };
        f.debug_struct("TypesafeDecisionProvider")
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
        Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
            requester,
        }
    }

    /// Returns the configured TypeSafe AI v1 base URL.
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

        let api_key = self.api_key.trim();
        if api_key.is_empty() {
            return Err(ProviderError::MissingTypesafeApiKey);
        }

        validate_typesafe_specifier(&request.model_specifier)?;
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
            model: strip_typesafe_prefix(request.model_specifier.trim()).to_string(),
            state: request.state.clone(),
            questions: wire_questions,
        };
        let url = build_systemone_url(&self.base_url);
        let outcome = self.requester.execute_json_post(
            &RetryablePostRequest {
                url: &url,
                model_specifier: &request.model_specifier,
                auth: HttpAuth::Bearer(api_key),
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

fn validate_typesafe_specifier(raw: &str) -> Result<(), ProviderError> {
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
                reason: "expected `typesafe:<model>` with non-empty segments".to_string(),
            });
        }
        if prov != TYPESAFE_PROVIDER {
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
            reason: format!("invalid TypeSafe `/v1/systemone` JSON envelope ({err})"),
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
        assert_eq!(strip_typesafe_prefix("typesafe:jev-latest"), "jev-latest");
        assert_eq!(strip_typesafe_prefix("jev-1.14.0"), "jev-1.14.0");
        assert_eq!(
            build_systemone_url("https://api.typesafe.ai/v1/"),
            "https://api.typesafe.ai/v1/systemone"
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
            Err(ProviderError::MissingTypesafeApiKey)
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
