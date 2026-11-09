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

//! Google Gemini v1beta REST adapter implementing [`StructuredModelProvider`] (`POST /v1beta/models/{model}:generateContent`).

use super::gemini_wire::{JSON_MIME_TYPE, build_request_payload, decode_generate_response};
use crate::caseload::DEFAULT_PROVIDER;
use crate::configure::ENV_GEMINI_API_KEY;
use crate::provider::{
    GenerationRequest, GenerationResponse, ProviderError, StructuredModelProvider,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, Instant};

/// Default Google Generative Language API v1beta base URL.
pub const DEFAULT_GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

/// Strips the canonical `"google:"` provider prefix from a normalized model specifier.
pub fn strip_provider_prefix(model_specifier: &str) -> &str {
    model_specifier
        .strip_prefix("google:")
        .unwrap_or(model_specifier)
}

/// Constructs the Google Generative Language `POST :generateContent` URL for a model specifier.
pub fn build_generate_content_url(base_url: &str, model_specifier: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let model_id = strip_provider_prefix(model_specifier);
    format!("{base}/models/{model_id}:generateContent")
}

/// Extracts a human-readable upstream error detail from a Gemini JSON error body or raw text payload.
pub fn extract_gemini_error_detail(body: &str) -> String {
    serde_json::from_str::<GeminiErrorEnvelope>(body)
        .ok()
        .and_then(|env| env.error)
        .and_then(|err| err.message)
        .map(|msg| msg.trim().to_string())
        .filter(|msg| !msg.is_empty())
        .unwrap_or_else(|| {
            let trimmed = body.trim();
            if trimmed.is_empty() {
                "empty error response".to_string()
            } else {
                trimmed.to_string()
            }
        })
}

/// Formats an HTTP error status code from the Google Gemini API into an actionable diagnostic with remediation hints.
pub fn classify_gemini_http_error(status_code: u16, model_specifier: &str, detail: &str) -> String {
    match status_code {
        401 | 403 => format!(
            "HTTP {status_code}: Authentication or permission rejected for `{model_specifier}` ({detail}). \
             Hint: Verify `{ENV_GEMINI_API_KEY}` or `providers.google.apiKey` in `~/.config/canon-clerk/config.json`."
        ),
        404 => format!(
            "HTTP 404: Model endpoint `{model_specifier}` was not found ({detail}). \
             Hint: Specify a valid Gemini model via `--screener-model` or `--auditor-model` (e.g. `google:gemini-flash-lite-latest`)."
        ),
        429 => format!(
            "HTTP 429: Quota or rate limit exceeded for `{model_specifier}` ({detail}). \
             Hint: Check your Google AI Studio quota tier or wait before retrying."
        ),
        500..=599 => format!(
            "HTTP {status_code}: Upstream Google Gemini server error for `{model_specifier}` ({detail}). \
             Hint: Retry after a brief backoff or increase `--timeout <MS>`."
        ),
        _ => format!(
            "HTTP {status_code}: Request rejected for `{model_specifier}` ({detail}). \
             Hint: Check model compatibility and provider configuration."
        ),
    }
}

fn validate_gemini_model_specifier(raw: &str) -> Result<&str, ProviderError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ProviderError::InvalidModelSpecifier {
            model_specifier: raw.to_string(),
            reason: "model specifier cannot be empty".to_string(),
        });
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(ProviderError::InvalidModelSpecifier {
            model_specifier: raw.to_string(),
            reason: "model specifier must not contain whitespace".to_string(),
        });
    }

    if let Some((provider, model_id)) = trimmed.split_once(':') {
        if provider.is_empty() || model_id.is_empty() || model_id.contains(':') {
            return Err(ProviderError::InvalidModelSpecifier {
                model_specifier: raw.to_string(),
                reason: "expected `<provider>:<model>` with non-empty segments".to_string(),
            });
        }
        if provider != DEFAULT_PROVIDER {
            return Err(ProviderError::UnsupportedProvider {
                provider: provider.to_string(),
                model_specifier: trimmed.to_string(),
            });
        }
    }

    Ok(trimmed)
}

#[derive(Debug, Deserialize)]
struct GeminiErrorEnvelope {
    error: Option<GeminiErrorBody>,
}

#[derive(Debug, Deserialize)]
struct GeminiErrorBody {
    message: Option<String>,
}

pub(crate) struct RawHttpOutcome {
    pub(crate) status_code: u16,
    pub(crate) body: String,
    pub(crate) latency_ms: u64,
}

pub(crate) fn post_gemini_json<T: Serialize>(
    base_url: &str,
    model_specifier: &str,
    api_key: &str,
    timeout_ms: u64,
    payload: &T,
) -> Result<RawHttpOutcome, (ProviderError, u64)> {
    let url = build_generate_content_url(base_url, model_specifier);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| {
            (
                ProviderError::Transport {
                    model_specifier: model_specifier.to_string(),
                    message: format!("failed to initialize async HTTP runtime ({err})"),
                },
                0,
            )
        })?;

    let start = Instant::now();
    runtime.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|err| {
                let elapsed = start.elapsed().as_millis() as u64;
                (
                    ProviderError::Transport {
                        model_specifier: model_specifier.to_string(),
                        message: format!("failed to build HTTPS client ({err})"),
                    },
                    elapsed,
                )
            })?;

        let send_res = client
            .post(&url)
            .header("x-goog-api-key", api_key)
            .header("Content-Type", JSON_MIME_TYPE)
            .json(payload)
            .send()
            .await;
        let latency_ms = start.elapsed().as_millis() as u64;

        let response = send_res.map_err(|err| {
            let provider_err = if err.is_timeout() {
                ProviderError::Timeout {
                    model_specifier: model_specifier.to_string(),
                    timeout_ms,
                }
            } else {
                ProviderError::Transport {
                    model_specifier: model_specifier.to_string(),
                    message: err.to_string(),
                }
            };
            (provider_err, latency_ms)
        })?;

        let status_code = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        Ok(RawHttpOutcome {
            status_code,
            body,
            latency_ms,
        })
    })
}

/// Google Gemini v1beta HTTPS adapter implementing [`StructuredModelProvider`].
#[derive(Clone)]
pub struct GeminiModelProvider {
    api_key: String,
    base_url: String,
}

impl fmt::Debug for GeminiModelProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key_repr = if self.api_key.trim().is_empty() {
            "<empty>"
        } else {
            "<redacted>"
        };
        f.debug_struct("GeminiModelProvider")
            .field("api_key", &key_repr)
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl GeminiModelProvider {
    /// Constructs a [`GeminiModelProvider`] targeting [`DEFAULT_GEMINI_BASE_URL`].
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: DEFAULT_GEMINI_BASE_URL.to_string(),
        }
    }

    /// Constructs a [`GeminiModelProvider`] with a custom base URL (useful for local proxies or test harnesses).
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
        }
    }

    /// Returns the configured Google Generative Language API base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl StructuredModelProvider for GeminiModelProvider {
    fn generate_structured(
        &self,
        request: &GenerationRequest,
    ) -> Result<GenerationResponse, ProviderError> {
        let api_key = self.api_key.trim();
        if api_key.is_empty() {
            return Err(ProviderError::MissingApiKey);
        }
        if request.timeout_ms == 0 {
            return Err(ProviderError::InvalidTimeout {
                timeout_ms: request.timeout_ms,
            });
        }

        let specifier = validate_gemini_model_specifier(&request.model_specifier)?;
        let payload = build_request_payload(request);
        let outcome = post_gemini_json(
            &self.base_url,
            specifier,
            api_key,
            request.timeout_ms,
            &payload,
        )
        .map_err(|(err, _latency)| err)?;

        decode_generate_response(
            outcome.status_code,
            &outcome.body,
            specifier,
            outcome.latency_ms,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{DEFAULT_AUDITOR_MODEL, DEFAULT_SCREENER_MODEL};
    use crate::provider::{DEFAULT_GENERATION_TIMEOUT_MS, TokenUsage};
    use serde_json::json;

    fn sample_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "compliance_summary": { "type": "string" },
                "compliance_score": { "type": "number" }
            },
            "propertyOrdering": ["compliance_summary", "compliance_score"],
            "required": ["compliance_summary", "compliance_score"]
        })
    }

    fn sample_request(reasoning_budget: Option<u32>) -> GenerationRequest {
        GenerationRequest {
            model_specifier: DEFAULT_AUDITOR_MODEL.to_string(),
            system_instruction: "Adjudicate compliance with the governing canon.".to_string(),
            user_prompt: "Canon: flags must be kebab-case.\nDiff: +--foo-bar".to_string(),
            response_schema: sample_schema(),
            reasoning_budget,
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
        }
    }

    mod request_serialization {
        use super::*;

        #[test]
        fn test_serializes_temperature_zero_response_schema_and_omits_thinking_when_none() {
            let req = sample_request(None);
            let wire = build_request_payload(&req);
            let serialized = serde_json::to_value(&wire).expect("serialize wire request");

            assert_eq!(
                serialized["systemInstruction"]["parts"][0]["text"],
                json!("Adjudicate compliance with the governing canon.")
            );
            assert_eq!(serialized["contents"][0]["role"], json!("user"));
            assert_eq!(
                serialized["contents"][0]["parts"][0]["text"],
                json!("Canon: flags must be kebab-case.\nDiff: +--foo-bar")
            );
            assert_eq!(serialized["generationConfig"]["temperature"], json!(0.0));
            assert_eq!(
                serialized["generationConfig"]["responseMimeType"],
                json!("application/json")
            );
            assert_eq!(
                serialized["generationConfig"]["responseSchema"],
                sample_schema()
            );
            assert!(
                serialized["generationConfig"]
                    .get("thinkingConfig")
                    .is_none()
            );
        }

        #[test]
        fn test_omits_system_instruction_when_blank_and_maps_reasoning_budgets() {
            let mut req_positive = sample_request(Some(4096));
            req_positive.system_instruction = "   ".to_string();
            let serialized_pos =
                serde_json::to_value(build_request_payload(&req_positive)).expect("serialize");

            assert!(serialized_pos.get("systemInstruction").is_none());
            assert_eq!(
                serialized_pos["generationConfig"]["thinkingConfig"],
                json!({
                    "includeThoughts": true,
                    "thinkingBudget": 4096
                })
            );

            let req_zero = sample_request(Some(0));
            let serialized_zero =
                serde_json::to_value(build_request_payload(&req_zero)).expect("serialize");
            assert_eq!(
                serialized_zero["generationConfig"]["thinkingConfig"],
                json!({
                    "includeThoughts": false,
                    "thinkingBudget": 0
                })
            );
        }
    }

    mod response_decoding {
        use super::*;

        #[test]
        fn test_decodes_structured_json_separates_thought_parts_and_maps_usage_metadata() {
            let raw_body = json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [
                            {
                                "thought": true,
                                "text": "Step 1: Checking flag naming convention."
                            },
                            {
                                "thought": true,
                                "text": "Step 2: `--foo-bar` uses kebab-case; compliant."
                            },
                            {
                                "text": "{\"compliance_summary\":\"All added CLI flags use kebab-case.\",\"compliance_score\":1.0}"
                            }
                        ]
                    },
                    "finishReason": "STOP"
                }],
                "usageMetadata": {
                    "promptTokenCount": 512,
                    "candidatesTokenCount": 34,
                    "thoughtsTokenCount": 128,
                    "totalTokenCount": 674
                },
                "modelVersion": "gemini-2.5-pro-preview-06-05"
            })
            .to_string();

            let resp = decode_generate_response(200, &raw_body, DEFAULT_AUDITOR_MODEL, 840)
                .expect("decode valid response");

            assert_eq!(resp.resolved_model, "gemini-2.5-pro-preview-06-05");
            assert_eq!(resp.latency_ms, 840);
            assert_eq!(
                resp.thoughts,
                vec![
                    "Step 1: Checking flag naming convention.".to_string(),
                    "Step 2: `--foo-bar` uses kebab-case; compliant.".to_string()
                ]
            );
            assert_eq!(
                resp.payload,
                json!({
                    "compliance_summary": "All added CLI flags use kebab-case.",
                    "compliance_score": 1.0
                })
            );
            assert_eq!(
                resp.usage,
                TokenUsage {
                    prompt_tokens: 512,
                    completion_tokens: 34,
                    reasoning_tokens: 128,
                    total_tokens: 674,
                }
            );
        }

        #[test]
        fn test_falls_back_to_specifier_and_computes_total_tokens_when_optional_fields_omitted() {
            let minimal_body = json!({
                "candidates": [{
                    "content": {
                        "parts": [{
                            "text": "{\"apprisal_summary\":\"Applicable.\",\"apprisal_score\":0.8}"
                        }]
                    }
                }],
                "usageMetadata": {
                    "promptTokenCount": 100,
                    "candidatesTokenCount": 25
                }
            })
            .to_string();

            let resp = decode_generate_response(200, &minimal_body, DEFAULT_SCREENER_MODEL, 195)
                .expect("decode minimal response");

            assert_eq!(resp.resolved_model, DEFAULT_SCREENER_MODEL);
            assert!(resp.thoughts.is_empty());
            assert_eq!(
                resp.usage,
                TokenUsage {
                    prompt_tokens: 100,
                    completion_tokens: 25,
                    reasoning_tokens: 0,
                    total_tokens: 125,
                }
            );
        }
    }

    mod error_classification {
        use super::*;

        #[test]
        fn test_classifies_http_401_403_404_429_and_5xx_errors_with_remediation_hints() {
            let cases = [
                (
                    401,
                    r#"{"error":{"message":"API key not valid."}}"#,
                    "HTTP 401",
                    ENV_GEMINI_API_KEY,
                ),
                (403, "Permission denied", "HTTP 403", "config.json"),
                (
                    404,
                    r#"{"error":{"message":"models/gemini-missing not found"}}"#,
                    "HTTP 404",
                    "--screener-model",
                ),
                (
                    429,
                    r#"{"error":{"message":"Resource has been exhausted."}}"#,
                    "HTTP 429",
                    "quota",
                ),
                (503, "Service Unavailable", "HTTP 503", "--timeout"),
                (400, "", "HTTP 400", "empty error response"),
            ];

            for (status, body, expected_status, expected_substr) in cases {
                let err = decode_generate_response(status, body, DEFAULT_SCREENER_MODEL, 60)
                    .expect_err("non-2xx status must return error");
                match &err {
                    ProviderError::HttpStatus { status_code, .. } => {
                        assert_eq!(*status_code, status);
                    }
                    other => panic!("expected HttpStatus variant, got {other:?}"),
                }
                let rendered = err.to_string();
                assert!(rendered.contains(expected_status), "got: {rendered}");
                assert!(rendered.contains(expected_substr), "got: {rendered}");
                assert!(rendered.contains("Hint:"), "got: {rendered}");
            }
        }

        #[test]
        fn test_classifies_prompt_blocks_empty_candidates_and_halted_finish_reasons() {
            let prompt_blocked = json!({
                "promptFeedback": { "blockReason": "SAFETY" }
            })
            .to_string();
            let err_prompt =
                decode_generate_response(200, &prompt_blocked, DEFAULT_AUDITOR_MODEL, 45)
                    .expect_err("prompt block must fail");
            assert!(matches!(err_prompt, ProviderError::BlockedResponse { .. }));
            assert!(err_prompt.to_string().contains("SAFETY"));

            let empty_candidates = json!({ "candidates": [] }).to_string();
            let err_empty =
                decode_generate_response(200, &empty_candidates, DEFAULT_SCREENER_MODEL, 30)
                    .expect_err("empty candidates must fail");
            assert!(matches!(err_empty, ProviderError::BlockedResponse { .. }));

            let safety_candidate = json!({
                "candidates": [{
                    "content": { "parts": [{ "text": "{}" }] },
                    "finishReason": "RECITATION"
                }]
            })
            .to_string();
            let err_recitation =
                decode_generate_response(200, &safety_candidate, DEFAULT_AUDITOR_MODEL, 50)
                    .expect_err("RECITATION finishReason must fail");
            assert!(matches!(
                err_recitation,
                ProviderError::BlockedResponse { .. }
            ));

            let exhausted_thinking = json!({
                "candidates": [{
                    "content": {
                        "parts": [{ "thought": true, "text": "Still reasoning..." }]
                    },
                    "finishReason": "MAX_TOKENS"
                }]
            })
            .to_string();
            let err_max_tokens =
                decode_generate_response(200, &exhausted_thinking, DEFAULT_AUDITOR_MODEL, 900)
                    .expect_err("MAX_TOKENS with zero output parts must fail");
            assert!(matches!(
                err_max_tokens,
                ProviderError::BlockedResponse { .. }
            ));
            assert!(err_max_tokens.to_string().contains("MAX_TOKENS"));
        }

        #[test]
        fn test_classifies_malformed_envelopes_and_schema_decoding_failures() {
            let err_html =
                decode_generate_response(200, "<html>gateway</html>", DEFAULT_SCREENER_MODEL, 20)
                    .expect_err("non-JSON 200 envelope must fail");
            assert!(matches!(err_html, ProviderError::MalformedResponse { .. }));

            let no_parts = json!({
                "candidates": [{
                    "content": { "parts": [] },
                    "finishReason": "STOP"
                }]
            })
            .to_string();
            let err_no_parts = decode_generate_response(200, &no_parts, DEFAULT_SCREENER_MODEL, 20)
                .expect_err("STOP candidate with empty parts must fail");
            assert!(matches!(
                err_no_parts,
                ProviderError::MalformedResponse { .. }
            ));

            let truncated_json = json!({
                "candidates": [{
                    "content": { "parts": [{ "text": "{\"compliance_summary\": \"unterminated" }] },
                    "finishReason": "STOP"
                }]
            })
            .to_string();
            let err_trunc =
                decode_generate_response(200, &truncated_json, DEFAULT_AUDITOR_MODEL, 120)
                    .expect_err("truncated JSON text must fail SchemaDecode");
            assert!(matches!(err_trunc, ProviderError::SchemaDecode { .. }));

            let array_payload = json!({
                "candidates": [{
                    "content": { "parts": [{ "text": "[1, 2, 3]" }] },
                    "finishReason": "STOP"
                }]
            })
            .to_string();
            let err_array =
                decode_generate_response(200, &array_payload, DEFAULT_AUDITOR_MODEL, 120)
                    .expect_err("non-object JSON payload must fail SchemaDecode");
            assert!(matches!(err_array, ProviderError::SchemaDecode { .. }));
        }
    }

    mod provider_validation_and_security {
        use super::*;

        #[test]
        fn test_validates_credentials_provider_prefix_specifier_and_timeout_offline() {
            let blank_provider = GeminiModelProvider::new("   ");
            assert_eq!(
                blank_provider.generate_structured(&sample_request(None)),
                Err(ProviderError::MissingApiKey)
            );

            let provider =
                GeminiModelProvider::with_base_url("synthetic-test-key", "http://127.0.0.1:9");
            assert_eq!(provider.base_url(), "http://127.0.0.1:9");

            let mut zero_timeout = sample_request(None);
            zero_timeout.timeout_ms = 0;
            assert_eq!(
                provider.generate_structured(&zero_timeout),
                Err(ProviderError::InvalidTimeout { timeout_ms: 0 })
            );

            let mut wrong_provider = sample_request(None);
            wrong_provider.model_specifier = "anthropic:claude-sonnet".to_string();
            assert_eq!(
                provider.generate_structured(&wrong_provider),
                Err(ProviderError::UnsupportedProvider {
                    provider: "anthropic".to_string(),
                    model_specifier: "anthropic:claude-sonnet".to_string(),
                })
            );

            for bad_spec in ["", "   ", "google:", ":gemini-2.5-pro", "google:bad spec"] {
                let mut bad_req = sample_request(None);
                bad_req.model_specifier = bad_spec.to_string();
                let err = provider
                    .generate_structured(&bad_req)
                    .expect_err("invalid specifier must fail preflight");
                assert!(matches!(err, ProviderError::InvalidModelSpecifier { .. }));
            }
        }

        #[test]
        fn test_debug_format_redacts_secret_api_key() {
            let secret = "super-secret-synthetic-credential";
            let provider = GeminiModelProvider::new(secret);
            let debug_output = format!("{provider:?}");
            assert!(!debug_output.contains(secret));
            assert!(debug_output.contains("<redacted>"));

            let empty_provider = GeminiModelProvider::new("");
            assert!(format!("{empty_provider:?}").contains("<empty>"));
        }
    }

    mod loopback_http_transport {
        use super::*;
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;

        fn read_complete_http_request(stream: &mut std::net::TcpStream) -> String {
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");
            let mut raw = Vec::new();
            let mut chunk = [0_u8; 4096];
            loop {
                let n = stream.read(&mut chunk).expect("read request bytes");
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&raw);
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let expected_len = head
                        .lines()
                        .find_map(|line| {
                            let (k, v) = line.split_once(':')?;
                            k.eq_ignore_ascii_case("content-length")
                                .then(|| v.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                    if body.len() >= expected_len {
                        return text.into_owned();
                    }
                }
            }
            String::from_utf8_lossy(&raw).into_owned()
        }

        #[test]
        fn test_generate_structured_executes_http_post_with_api_key_header_and_decodes_json() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");

            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept client");
                let raw_req = read_complete_http_request(&mut stream);
                let lower = raw_req.to_ascii_lowercase();
                assert!(
                    raw_req.starts_with("POST /v1beta/models/gemini-pro-latest:generateContent ")
                );
                assert!(lower.contains("x-goog-api-key: synthetic-loopback-key"));
                assert!(raw_req.contains("\"responseMimeType\":\"application/json\""));

                let body = json!({
                    "candidates": [{
                        "content": {
                            "parts": [
                                { "thought": true, "text": "Verified flag syntax." },
                                { "text": "{\"compliance_summary\":\"Compliant.\",\"compliance_score\":0.98}" }
                            ]
                        },
                        "finishReason": "STOP"
                    }],
                    "usageMetadata": {
                        "promptTokenCount": 40,
                        "candidatesTokenCount": 12,
                        "thoughtsTokenCount": 9,
                        "totalTokenCount": 61
                    },
                    "modelVersion": "gemini-2.5-pro"
                })
                .to_string();

                let http_resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream
                    .write_all(http_resp.as_bytes())
                    .expect("write HTTP 200");
            });

            let provider = GeminiModelProvider::with_base_url(
                "synthetic-loopback-key",
                format!("http://{addr}/v1beta"),
            );
            let result = provider
                .generate_structured(&sample_request(Some(1024)))
                .expect("loopback structured generation must succeed");

            server.join().expect("server thread cleanly joined");
            assert_eq!(result.resolved_model, "gemini-2.5-pro");
            assert_eq!(result.thoughts, vec!["Verified flag syntax.".to_string()]);
            assert_eq!(result.payload["compliance_score"], json!(0.98));
            assert_eq!(result.usage.total_tokens, 61);
        }

        #[test]
        fn test_generate_structured_maps_stalled_upstream_socket_to_timeout_error() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");

            let server = thread::spawn(move || {
                let (_stream, _) = listener.accept().expect("accept client");
                thread::sleep(Duration::from_millis(350));
            });

            let provider = GeminiModelProvider::with_base_url(
                "synthetic-loopback-key",
                format!("http://{addr}/v1beta"),
            );
            let mut req = sample_request(None);
            req.timeout_ms = 50;

            let err = provider
                .generate_structured(&req)
                .expect_err("stalled socket must time out");
            server.join().expect("server thread joined");

            assert_eq!(
                err,
                ProviderError::Timeout {
                    model_specifier: DEFAULT_AUDITOR_MODEL.to_string(),
                    timeout_ms: 50,
                }
            );
        }
    }
}
