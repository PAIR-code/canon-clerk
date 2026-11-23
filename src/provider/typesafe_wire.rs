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

//! System One (`POST /v1/systemone`) JSON wire types and profile-aware HTTP error classification.

use super::gemini_decisions::is_decision_context_limit_error;
use crate::caseload::{LOCAL_PROVIDER, TYPESAFE_PROVIDER};
use crate::configure::{credential_verification_hint_for_slug, model_provider};
use crate::provider::ProviderError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Typed `noul` binary decision question in a System One `/v1/systemone` request.
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
    /// Unprefixed System One model identifier (e.g. `"jev-latest"` or `"kev-4b"`).
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

/// Token usage telemetry returned by `/v1/systemone`.
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
    /// Concrete model identifier reported by the upstream endpoint.
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

/// Extracts a human-readable upstream error detail from a System One JSON error body or raw text.
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

/// Formats an HTTP error status code from a System One (`/v1/systemone`) endpoint into an actionable,
/// profile-aware diagnostic with remediation hints (`docs/architecture/transport.md` §4).
pub(crate) fn classify_typesafe_http_error(
    status_code: u16,
    model_specifier: &str,
    detail: &str,
) -> String {
    let slug = model_provider(model_specifier);
    let is_typesafe = slug == TYPESAFE_PROVIDER;
    let is_local = slug == LOCAL_PROVIDER;

    match status_code {
        400 if is_decision_context_limit_error(detail) => {
            if is_typesafe {
                format!(
                    "HTTP 400: Input context or question batch exceeded TypeSafe Jev token limit for `{model_specifier}` ({detail}). \
                     Hint: Pair `--provider typesafe` with `--audit-model google:gemini-4-carbon`, narrow target scope, or reduce `--docket-batch-size` / `--apprise-batch-size`."
                )
            } else {
                format!(
                    "HTTP 400: Input context or question batch exceeded System One context limit for `{model_specifier}` ({detail}). \
                     Hint: Increase the server context size (e.g. `llama-server --ctx-size 32768 --kv-unified`), narrow target scope, or reduce `--docket-batch-size` / `--apprise-batch-size`."
                )
            }
        }
        401 | 403 => {
            let key_hint = credential_verification_hint_for_slug(slug);
            format!(
                "HTTP {status_code}: Authentication or permission rejected for `{model_specifier}` ({detail}). \
                 Hint: {key_hint}"
            )
        }
        404 => {
            if is_typesafe {
                format!(
                    "HTTP 404: Model or endpoint for `{model_specifier}` was not found ({detail}). \
                     Hint: Specify a valid TypeSafe Jev model via `--provider typesafe` or `--<node>-model` (e.g. `typesafe:jev-latest`)."
                )
            } else if is_local {
                format!(
                    "HTTP 404: System One endpoint or model for `{model_specifier}` was not found ({detail}). \
                     Hint: Verify your local server supports `POST /v1/systemone` and check `--local-base-url` (default: `http://127.0.0.1:8080/v1`)."
                )
            } else {
                format!(
                    "HTTP 404: System One endpoint or model for `{model_specifier}` was not found ({detail}). \
                     Hint: Verify `providers.{slug}.baseUrl` supports `POST /v1/systemone` and check the configured model ID."
                )
            }
        }
        429 => {
            if is_typesafe {
                format!(
                    "HTTP 429: Quota or rate limit exceeded for `{model_specifier}` ({detail}). \
                     Hint: Check your TypeSafe AI quota tier or wait before retrying."
                )
            } else {
                format!(
                    "HTTP 429: Rate or concurrency limit exceeded for `{model_specifier}` ({detail}). \
                     Hint: Reduce stage concurrency (`--<node>-concurrency 1`) or wait before retrying."
                )
            }
        }
        501 => format!(
            "HTTP 501: Endpoint rejected `/v1/systemone` for `{model_specifier}` ({detail}). \
             Hint: Ensure the target server is running a System One decision model (for `llama-server`, load a decision GGUF such as `ggml-org/Kev-4B-GGUF` rather than a standard chat model)."
        ),
        500..=599 => {
            let label = if is_typesafe {
                "Upstream TypeSafe AI server error"
            } else {
                "Upstream System One server error"
            };
            format!(
                "HTTP {status_code}: {label} for `{model_specifier}` ({detail}). \
                 Hint: Retry after a brief backoff or increase `--timeout <MS>`."
            )
        }
        _ => {
            if is_typesafe {
                format!(
                    "HTTP {status_code}: Request rejected for `{model_specifier}` ({detail}). \
                     Hint: Check Jev model compatibility and TypeSafe AI configuration."
                )
            } else {
                format!(
                    "HTTP {status_code}: Request rejected for `{model_specifier}` ({detail}). \
                     Hint: Check `/v1/systemone` model compatibility and `providers.{slug}` configuration."
                )
            }
        }
    }
}

pub(crate) fn build_typesafe_http_status_error(
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
