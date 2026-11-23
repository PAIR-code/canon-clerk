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

use super::gemini_wire::{build_request_payload, decode_generate_response};
use super::requester::{RetryObserver, RetryablePostRequest, SharedHttpRequester};
use super::retry::RetryPolicy;
use super::runtime::{HttpAuth, JsonPostRequest, RawHttpOutcome, execute_json_post};
use crate::caseload::DEFAULT_PROVIDER;
pub use crate::configure::DEFAULT_GEMINI_BASE_URL;
use crate::configure::{
    ProviderConnection, credential_verification_hint_for_slug, is_gemini_decision_model_name,
    model_provider,
};
use crate::provider::{
    GenerationRequest, GenerationResponse, ProviderError, StructuredModelProvider,
};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Strips any canonical `<provider>:` prefix from a normalized model specifier (`<provider>:<model>` -> `<model>`).
pub fn strip_provider_prefix(model_specifier: &str) -> &str {
    let trimmed = model_specifier.trim();
    match trimmed.split_once(':') {
        Some((prov, model_id)) if !prov.is_empty() && !model_id.is_empty() => model_id,
        _ => trimmed,
    }
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
        401 | 403 => {
            let key_hint = credential_verification_hint_for_slug(model_provider(model_specifier));
            format!(
                "HTTP {status_code}: Authentication or permission rejected for `{model_specifier}` ({detail}). \
                 Hint: {key_hint}"
            )
        }
        404 => format!(
            "HTTP 404: Model endpoint `{model_specifier}` was not found ({detail}). \
             Hint: Specify a valid `<provider>:<model>` Gemini identifier via `--<node>-model` (e.g. `google:gemini-4-carbon`)."
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

fn validate_gemini_model_specifier<'a>(
    raw: &'a str,
    expected_slug: &str,
) -> Result<&'a str, ProviderError> {
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
        if provider != expected_slug {
            return Err(ProviderError::UnsupportedProvider {
                provider: provider.to_string(),
                model_specifier: trimmed.to_string(),
            });
        }
    }

    if is_gemini_decision_model_name(strip_provider_prefix(trimmed)) {
        return Err(ProviderError::InvalidModelSpecifier {
            model_specifier: raw.to_string(),
            reason: "discriminative Gemini Decisions models use `/v1beta/decisions` rather than `:generateContent`".to_string(),
        });
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

pub(crate) fn post_gemini_json<T: Serialize>(
    client: &reqwest::Client,
    base_url: &str,
    model_specifier: &str,
    api_key: &str,
    timeout_ms: u64,
    payload: &T,
) -> Result<RawHttpOutcome, (ProviderError, u64)> {
    let url = build_generate_content_url(base_url, model_specifier);
    let req = JsonPostRequest {
        url: &url,
        model_specifier,
        auth: HttpAuth::GoogApiKey(api_key),
        timeout_ms,
    };
    execute_json_post(client, &req, payload)
}

fn build_gemini_http_status_error(
    status_code: u16,
    model_specifier: &str,
    body: &str,
) -> ProviderError {
    let detail = extract_gemini_error_detail(body);
    let diagnostic = classify_gemini_http_error(status_code, model_specifier, &detail);
    ProviderError::HttpStatus {
        status_code,
        model_specifier: model_specifier.to_string(),
        detail,
        diagnostic,
    }
}

/// Google Gemini v1beta HTTPS adapter implementing [`StructuredModelProvider`].
#[derive(Clone)]
pub struct GeminiModelProvider {
    slug: String,
    api_key: String,
    base_url: String,
    requester: SharedHttpRequester,
}

impl fmt::Debug for GeminiModelProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key_repr = if self.api_key.trim().is_empty() {
            "<empty>"
        } else {
            "<redacted>"
        };
        f.debug_struct("GeminiModelProvider")
            .field("slug", &self.slug)
            .field("api_key", &key_repr)
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl GeminiModelProvider {
    /// Constructs a [`GeminiModelProvider`] targeting [`DEFAULT_GEMINI_BASE_URL`].
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_base_url(api_key, DEFAULT_GEMINI_BASE_URL)
    }

    /// Constructs a [`GeminiModelProvider`] with a custom base URL (useful for local proxies or test harnesses).
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self::with_requester_and_base_url(api_key, SharedHttpRequester::default(), base_url)
    }

    /// Constructs a [`GeminiModelProvider`] with a caller-supplied reusable [`reqwest::Client`].
    pub fn with_client(api_key: impl Into<String>, client: reqwest::Client) -> Self {
        Self::with_client_and_base_url(api_key, client, DEFAULT_GEMINI_BASE_URL)
    }

    /// Constructs a [`GeminiModelProvider`] with a custom [`reqwest::Client`] and base URL.
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

    /// Constructs a [`GeminiModelProvider`] sharing an explicit [`SharedHttpRequester`].
    pub fn with_requester(api_key: impl Into<String>, requester: SharedHttpRequester) -> Self {
        Self::with_requester_and_base_url(api_key, requester, DEFAULT_GEMINI_BASE_URL)
    }

    /// Constructs a [`GeminiModelProvider`] sharing an explicit [`SharedHttpRequester`] and custom base URL.
    pub fn with_requester_and_base_url(
        api_key: impl Into<String>,
        requester: SharedHttpRequester,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            slug: DEFAULT_PROVIDER.to_string(),
            api_key: api_key.into(),
            base_url: base_url.into(),
            requester,
        }
    }

    /// Constructs a [`GeminiModelProvider`] from a resolved [`ProviderConnection`] and shared [`SharedHttpRequester`].
    pub fn from_connection(conn: &ProviderConnection, requester: SharedHttpRequester) -> Self {
        Self {
            slug: conn.slug.clone(),
            api_key: conn.auth.secret().unwrap_or_default().to_string(),
            base_url: conn.base_url.clone(),
            requester,
        }
    }

    /// Returns the canonical profile slug bound to this adapter.
    pub fn slug(&self) -> &str {
        &self.slug
    }

    /// Returns the configured Google Generative Language API base URL.
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

impl StructuredModelProvider for GeminiModelProvider {
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
        let api_key = self.api_key.trim();
        if api_key.is_empty() {
            return Err(ProviderError::missing_api_key_for_slug(&self.slug));
        }
        if request.timeout_ms == 0 {
            return Err(ProviderError::InvalidTimeout {
                timeout_ms: request.timeout_ms,
            });
        }

        let specifier = validate_gemini_model_specifier(&request.model_specifier, &self.slug)?;
        let payload = build_request_payload(request);
        let url = build_generate_content_url(&self.base_url, specifier);
        let outcome = self.requester.execute_json_post(
            &RetryablePostRequest {
                url: &url,
                model_specifier: specifier,
                auth: HttpAuth::GoogApiKey(api_key),
                timeout_ms: request.timeout_ms,
                call_label: request.call_label.as_deref(),
                format_http_error: build_gemini_http_status_error,
            },
            &payload,
            on_retry,
        )?;

        decode_generate_response(
            outcome.status_code,
            &outcome.body,
            specifier,
            outcome.latency_ms,
        )
    }

    fn clear_abort(&self) {
        self.requester.clear_abort();
    }

    fn notify_abort(&self) {
        self.requester.notify_abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, DEFAULT_GOOGLE_GENERATOR_MODEL};
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
            model_specifier: DEFAULT_GOOGLE_GENERATOR_MODEL.to_string(),
            system_instruction: "Adjudicate compliance with the governing canon.".to_string(),
            user_prompt: "Canon: flags must be kebab-case.\nDiff: +--foo-bar".to_string(),
            response_schema: sample_schema(),
            thinking_level: None,
            reasoning_budget,
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
            call_label: None,
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

            let mut req_zero_legacy = sample_request(Some(0));
            req_zero_legacy.model_specifier = "google:gemini-flash-lite-latest".to_string();
            let serialized_zero =
                serde_json::to_value(build_request_payload(&req_zero_legacy)).expect("serialize");
            assert_eq!(
                serialized_zero["generationConfig"]["thinkingConfig"],
                json!({
                    "includeThoughts": false,
                    "thinkingBudget": 0
                })
            );

            let req_zero_gemini_4 = sample_request(Some(0));
            let serialized_zero_g4 =
                serde_json::to_value(build_request_payload(&req_zero_gemini_4)).expect("serialize");
            assert_eq!(
                serialized_zero_g4["generationConfig"]["thinkingConfig"],
                json!({
                    "includeThoughts": false,
                    "thinkingLevel": "LOW"
                })
            );
        }

        #[test]
        fn test_serializes_thinking_levels_and_clamps_minimal_on_gemini_4() {
            use crate::caseload::ThinkingLevel;

            for (level, expected_wire) in [
                (ThinkingLevel::Low, "LOW"),
                (ThinkingLevel::Medium, "MEDIUM"),
                (ThinkingLevel::High, "HIGH"),
            ] {
                let mut req = sample_request(None);
                req.thinking_level = Some(level);
                let serialized =
                    serde_json::to_value(build_request_payload(&req)).expect("serialize level");
                assert_eq!(
                    serialized["generationConfig"]["thinkingConfig"],
                    json!({
                        "includeThoughts": true,
                        "thinkingLevel": expected_wire
                    })
                );
            }

            // Minimal on Gemini 4 clamps to LOW (includeThoughts: false); on Gemini 3 emits MINIMAL.
            let mut req_min_g4 = sample_request(None);
            req_min_g4.thinking_level = Some(ThinkingLevel::Minimal);
            let ser_min_g4 =
                serde_json::to_value(build_request_payload(&req_min_g4)).expect("serialize g4 min");
            assert_eq!(
                ser_min_g4["generationConfig"]["thinkingConfig"],
                json!({
                    "includeThoughts": false,
                    "thinkingLevel": "LOW"
                })
            );

            let mut req_min_g3 = sample_request(None);
            req_min_g3.model_specifier = "google:gemini-3-flash-preview".to_string();
            req_min_g3.thinking_level = Some(ThinkingLevel::Minimal);
            let ser_min_g3 =
                serde_json::to_value(build_request_payload(&req_min_g3)).expect("serialize g3 min");
            assert_eq!(
                ser_min_g3["generationConfig"]["thinkingConfig"],
                json!({
                    "includeThoughts": false,
                    "thinkingLevel": "MINIMAL"
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

            let resp =
                decode_generate_response(200, &raw_body, DEFAULT_GOOGLE_GENERATOR_MODEL, 840)
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

            let resp = decode_generate_response(
                200,
                &minimal_body,
                DEFAULT_GOOGLE_DISCRIMINATOR_MODEL,
                195,
            )
            .expect("decode minimal response");

            assert_eq!(resp.resolved_model, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL);
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
        use crate::configure::ENV_GEMINI_API_KEY;

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
                    "--<node>-model",
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
                let err =
                    decode_generate_response(status, body, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, 60)
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
                decode_generate_response(200, &prompt_blocked, DEFAULT_GOOGLE_GENERATOR_MODEL, 45)
                    .expect_err("prompt block must fail");
            assert!(matches!(err_prompt, ProviderError::BlockedResponse { .. }));
            assert!(err_prompt.to_string().contains("SAFETY"));

            let empty_candidates = json!({ "candidates": [] }).to_string();
            let err_empty = decode_generate_response(
                200,
                &empty_candidates,
                DEFAULT_GOOGLE_DISCRIMINATOR_MODEL,
                30,
            )
            .expect_err("empty candidates must fail");
            assert!(matches!(err_empty, ProviderError::BlockedResponse { .. }));

            let safety_candidate = json!({
                "candidates": [{
                    "content": { "parts": [{ "text": "{}" }] },
                    "finishReason": "RECITATION"
                }]
            })
            .to_string();
            let err_recitation = decode_generate_response(
                200,
                &safety_candidate,
                DEFAULT_GOOGLE_GENERATOR_MODEL,
                50,
            )
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
            let err_max_tokens = decode_generate_response(
                200,
                &exhausted_thinking,
                DEFAULT_GOOGLE_GENERATOR_MODEL,
                900,
            )
            .expect_err("MAX_TOKENS with zero output parts must fail");
            assert!(matches!(
                err_max_tokens,
                ProviderError::BlockedResponse { .. }
            ));
            assert!(err_max_tokens.to_string().contains("MAX_TOKENS"));
        }

        #[test]
        fn test_classifies_malformed_envelopes_and_schema_decoding_failures() {
            let err_html = decode_generate_response(
                200,
                "<html>gateway</html>",
                DEFAULT_GOOGLE_DISCRIMINATOR_MODEL,
                20,
            )
            .expect_err("non-JSON 200 envelope must fail");
            assert!(matches!(err_html, ProviderError::MalformedResponse { .. }));

            let no_parts = json!({
                "candidates": [{
                    "content": { "parts": [] },
                    "finishReason": "STOP"
                }]
            })
            .to_string();
            let err_no_parts =
                decode_generate_response(200, &no_parts, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, 20)
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
                decode_generate_response(200, &truncated_json, DEFAULT_GOOGLE_GENERATOR_MODEL, 120)
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
                decode_generate_response(200, &array_payload, DEFAULT_GOOGLE_GENERATOR_MODEL, 120)
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
                Err(ProviderError::missing_api_key_for_slug(DEFAULT_PROVIDER))
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
        use std::io::{ErrorKind, Read, Write};
        use std::net::TcpListener;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::sync::{Arc, Condvar, Mutex};
        use std::thread;
        use std::time::{Duration, Instant};

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
                    raw_req.starts_with("POST /v1beta/models/gemini-4-carbon:generateContent ")
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
                let mut held = Vec::new();
                for _ in 0..3 {
                    let (stream, _) = listener.accept().expect("accept client");
                    held.push(stream);
                }
                thread::sleep(Duration::from_millis(120));
            });

            let requester =
                SharedHttpRequester::new(RetryPolicy::new(2, 500).with_base_and_slot_ms(10, 5));
            let provider = GeminiModelProvider::with_requester_and_base_url(
                "synthetic-loopback-key",
                requester,
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
                    model_specifier: DEFAULT_GOOGLE_GENERATOR_MODEL.to_string(),
                    timeout_ms: 50,
                }
            );
        }

        #[test]
        fn test_connection_reuse_across_sequential_requests_on_shared_provider() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");

            let server = thread::spawn(move || {
                // Accept strictly a single TCP connection.
                let (mut stream, _) = listener.accept().expect("accept single client connection");

                for call_idx in 1..=2 {
                    let raw_req = read_complete_http_request(&mut stream);
                    assert!(
                        raw_req.starts_with("POST /v1beta/models/gemini-4-carbon:generateContent ")
                    );

                    let body = json!({
                        "candidates": [{
                            "content": {
                                "parts": [
                                    { "text": format!("{{\"call_idx\":{call_idx}}}") }
                                ]
                            },
                            "finishReason": "STOP"
                        }],
                        "modelVersion": "gemini-2.5-pro"
                    })
                    .to_string();

                    let http_resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream
                        .write_all(http_resp.as_bytes())
                        .expect("write HTTP response");
                    stream.flush().expect("flush stream");
                }
            });

            let provider = GeminiModelProvider::with_base_url(
                "synthetic-loopback-key",
                format!("http://{addr}/v1beta"),
            );

            let res1 = provider
                .generate_structured(&sample_request(None))
                .expect("first call succeeds over pooled client");
            assert_eq!(res1.payload["call_idx"], json!(1));

            let res2 = provider
                .generate_structured(&sample_request(None))
                .expect("second call succeeds on reused TCP stream");
            assert_eq!(res2.payload["call_idx"], json!(2));

            server.join().expect("server thread cleanly joined");
        }

        #[test]
        fn test_concurrent_worker_threads_share_gemini_provider() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");

            let server = thread::spawn(move || {
                for _ in 0..4 {
                    let (mut stream, _) = listener.accept().expect("accept connection");
                    let _raw_req = read_complete_http_request(&mut stream);
                    let body = json!({
                        "candidates": [{
                            "content": {
                                "parts": [
                                    { "text": "{\"status\":\"ok\"}" }
                                ]
                            },
                            "finishReason": "STOP"
                        }],
                        "modelVersion": "gemini-2.5-pro"
                    })
                    .to_string();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes());
                }
            });

            let provider = GeminiModelProvider::with_base_url(
                "synthetic-loopback-key",
                format!("http://{addr}/v1beta"),
            );

            std::thread::scope(|s| {
                let mut handles = Vec::new();
                for _ in 0..4 {
                    let prov = provider.clone();
                    handles.push(s.spawn(move || prov.generate_structured(&sample_request(None))));
                }
                for handle in handles {
                    let res = handle.join().expect("thread joined").expect("call ok");
                    assert_eq!(res.payload["status"], "ok");
                }
            });

            server.join().expect("server thread cleanly joined");
        }

        #[test]
        fn test_client_accessor_and_custom_client_constructor() {
            let custom_client = reqwest::Client::builder().build().expect("build client");
            let provider = GeminiModelProvider::with_client("test-key", custom_client.clone());
            assert_eq!(provider.base_url(), DEFAULT_GEMINI_BASE_URL);
            assert!(provider.client().post("http://127.0.0.1").build().is_ok());

            let custom_with_base = GeminiModelProvider::with_client_and_base_url(
                "test-key",
                custom_client,
                "http://custom-proxy:8080",
            );
            assert_eq!(custom_with_base.base_url(), "http://custom-proxy:8080");
        }

        fn serve_staged_keep_alive_connection(
            mut stream: std::net::TcpStream,
            served: Arc<AtomicUsize>,
            shutdown: Arc<AtomicBool>,
            stage2_latch: Arc<(Mutex<usize>, Condvar)>,
        ) {
            loop {
                if shutdown.load(Ordering::Acquire) {
                    break;
                }
                stream
                    .set_read_timeout(Some(Duration::from_millis(200)))
                    .expect("set read timeout");
                let mut probe = [0_u8; 1];
                match stream.peek(&mut probe) {
                    Ok(0) => break,
                    Err(err)
                        if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                    {
                        continue;
                    }
                    Err(_) => break,
                    Ok(_) => {}
                }
                let raw_req = read_complete_http_request(&mut stream);
                if raw_req.is_empty() {
                    break;
                }
                let n = served.fetch_add(1, Ordering::SeqCst) + 1;
                if n >= 2 {
                    let (lock, cvar) = &*stage2_latch;
                    let mut arrived = lock.lock().expect("stage2 latch lock");
                    *arrived += 1;
                    if *arrived >= 4 {
                        cvar.notify_all();
                    } else {
                        let deadline = Instant::now() + Duration::from_secs(2);
                        while *arrived < 4 && !shutdown.load(Ordering::Acquire) {
                            let remaining = deadline.saturating_duration_since(Instant::now());
                            if remaining.is_zero() {
                                break;
                            }
                            let (guard, wait_res) = cvar
                                .wait_timeout(arrived, remaining)
                                .expect("stage2 latch wait");
                            arrived = guard;
                            if wait_res.timed_out() {
                                break;
                            }
                        }
                    }
                }
                let body = json!({
                    "candidates": [{
                        "content": {
                            "parts": [{ "text": format!("{{\"req_num\":{n}}}") }]
                        },
                        "finishReason": "STOP"
                    }],
                    "modelVersion": "gemini-4-hydrogen"
                })
                .to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                if stream.write_all(resp.as_bytes()).is_err() {
                    break;
                }
                let _ = stream.flush();
            }
        }

        #[test]
        fn test_parked_warmer_thread_does_not_starve_downstream_concurrent_workers() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            listener
                .set_nonblocking(true)
                .expect("set nonblocking accept");
            let addr = listener.local_addr().expect("local addr");
            let accepted_sockets = Arc::new(AtomicUsize::new(0));
            let served_requests = Arc::new(AtomicUsize::new(0));
            let shutdown = Arc::new(AtomicBool::new(false));
            let stage2_arrived = Arc::new((Mutex::new(0_usize), Condvar::new()));

            let accepted_ref = Arc::clone(&accepted_sockets);
            let served_ref = Arc::clone(&served_requests);
            let shutdown_ref = Arc::clone(&shutdown);
            let stage2_ref = Arc::clone(&stage2_arrived);

            // Serve 5 total requests across 4 keep-alive sockets (1 warmed + reused + 3 fresh),
            // terminating cleanly via `shutdown` and synchronizing Stage 2's 4 workers on a
            // bounded latch so all 4 are simultaneously in-flight.
            let server = thread::spawn(move || {
                let mut conn_threads = Vec::new();
                while !shutdown_ref.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            stream
                                .set_nonblocking(false)
                                .expect("set accepted socket blocking");
                            accepted_ref.fetch_add(1, Ordering::SeqCst);
                            let served_inner = Arc::clone(&served_ref);
                            let shutdown_inner = Arc::clone(&shutdown_ref);
                            let stage2_inner = Arc::clone(&stage2_ref);
                            conn_threads.push(thread::spawn(move || {
                                serve_staged_keep_alive_connection(
                                    stream,
                                    served_inner,
                                    shutdown_inner,
                                    stage2_inner,
                                );
                            }));
                        }
                        Err(err) if err.kind() == ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(err) => panic!("accept pooled socket: {err}"),
                    }
                }
                for h in conn_threads {
                    let _ = h.join();
                }
            });

            let provider = GeminiModelProvider::with_base_url(
                "synthetic-loopback-key",
                format!("http://{addr}/v1beta"),
            );
            let mut req = sample_request(None);
            req.timeout_ms = 1_000;

            // Stage 1 (simulating 1-canon `docket` inline on caller thread): warm 1 pooled socket.
            let warm = provider
                .generate_structured(&req)
                .expect("warming request succeeds");
            assert_eq!(warm.payload["req_num"], json!(1));

            // Stage 2 (simulating 4-slice `admit` parking caller thread inside `std::thread::scope`):
            // One worker pops the warmed keep-alive socket while caller thread is parked on join.
            let worker_results: Vec<_> = std::thread::scope(|scope| {
                let mut handles = Vec::new();
                for _ in 0..4 {
                    let prov = provider.clone();
                    let worker_req = req.clone();
                    handles.push(scope.spawn(move || prov.generate_structured(&worker_req)));
                }
                handles
                    .into_iter()
                    .map(|handle| handle.join().expect("worker thread joined"))
                    .collect()
            });

            drop(provider);
            shutdown.store(true, Ordering::Release);
            stage2_arrived.1.notify_all();
            server.join().expect("server joined");

            for res in worker_results {
                let res =
                    res.expect("concurrent worker call must not stall on parked warmer thread");
                assert!(res.payload["req_num"].as_u64().is_some());
            }
            assert_eq!(served_requests.load(Ordering::SeqCst), 5);
            assert_eq!(
                accepted_sockets.load(Ordering::SeqCst),
                4,
                "5 total calls across 4 simultaneously in-flight workers must reuse the warmed socket"
            );
        }

        #[test]
        fn test_early_worker_exit_preserves_reused_connection_for_subsequent_worker() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");

            let server = thread::spawn(move || {
                // Strictly accept ONE TCP connection across two distinct short-lived OS threads.
                let (mut stream, _) = listener.accept().expect("accept single pooled socket");
                for call_idx in 1..=2 {
                    let raw_req = read_complete_http_request(&mut stream);
                    assert!(raw_req.starts_with("POST /v1beta/models/"));
                    if call_idx == 2 {
                        thread::sleep(Duration::from_millis(40));
                    }
                    let body = json!({
                        "candidates": [{
                            "content": {
                                "parts": [{ "text": format!("{{\"call_idx\":{call_idx}}}") }]
                            },
                            "finishReason": "STOP"
                        }],
                        "modelVersion": "gemini-4-hydrogen"
                    })
                    .to_string();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream.write_all(resp.as_bytes()).expect("write response");
                    stream.flush().expect("flush response");
                }
            });

            let provider = GeminiModelProvider::with_base_url(
                "synthetic-loopback-key",
                format!("http://{addr}/v1beta"),
            );

            // Worker 1 warms the pooled connection and immediately terminates its OS thread.
            let prov1 = provider.clone();
            let res1 = thread::spawn(move || prov1.generate_structured(&sample_request(None)))
                .join()
                .expect("worker 1 joined")
                .expect("worker 1 request succeeded");
            assert_eq!(res1.payload["call_idx"], json!(1));

            // Worker 2 reuses the exact same pooled TCP stream after Worker 1's OS thread exited.
            let res2 = thread::spawn(move || provider.generate_structured(&sample_request(None)))
                .join()
                .expect("worker 2 joined")
                .expect("worker 2 reused socket survived worker 1 exit");
            assert_eq!(res2.payload["call_idx"], json!(2));

            server.join().expect("server joined");
        }

        #[test]
        fn test_stalled_chunked_response_body_maps_to_timeout_instead_of_malformed_envelope() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");

            let server = thread::spawn(move || {
                let mut held = Vec::new();
                for _ in 0..3 {
                    let (mut stream, _) = listener.accept().expect("accept client");
                    let _raw_req = read_complete_http_request(&mut stream);
                    // Flush 200 OK headers and an incomplete chunked body, then stall past timeout_ms.
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n{\"can\r\n",
                        )
                        .expect("write partial chunked response");
                    stream.flush().expect("flush partial chunk");
                    held.push(stream);
                }
                thread::sleep(Duration::from_millis(120));
            });

            let requester =
                SharedHttpRequester::new(RetryPolicy::new(2, 500).with_base_and_slot_ms(10, 5));
            let provider = GeminiModelProvider::with_requester_and_base_url(
                "synthetic-loopback-key",
                requester,
                format!("http://{addr}/v1beta"),
            );
            let mut req = sample_request(None);
            req.timeout_ms = 60;

            let err = provider
                .generate_structured(&req)
                .expect_err("mid-body stall must fail with timeout");
            server.join().expect("server joined");

            assert_eq!(
                err,
                ProviderError::Timeout {
                    model_specifier: DEFAULT_GOOGLE_GENERATOR_MODEL.to_string(),
                    timeout_ms: 60,
                }
            );
        }
    }
}
