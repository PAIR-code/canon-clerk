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

//! Live Google Gemini `:generateContent` endpoint prober and multi-provider composite router.

use super::{
    DEFAULT_GEMINI_BASE_URL, EndpointProber, GeminiDecisionsEndpointProber, ProbeEndpointResult,
    TypesafeEndpointProber, strip_provider_prefix,
};
use crate::caseload::{ProbeStatus, TYPESAFE_PROVIDER, ThinkingLevel};
use crate::configure::{is_discriminative_model, model_provider};
use crate::provider::gemini::post_gemini_json;
use crate::provider::{
    GeminiThinkingConfig, build_pooled_http_client, build_probe_thinking_config,
    classify_gemini_http_error, extract_gemini_error_detail,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct GeminiGenerateContentRequest<'a> {
    contents: Vec<GeminiContent<'a>>,
    #[serde(rename = "generationConfig")]
    generation_config: GeminiGenerationConfig,
}

#[derive(Debug, Serialize)]
struct GeminiContent<'a> {
    parts: Vec<GeminiPart<'a>>,
}

#[derive(Debug, Serialize)]
struct GeminiPart<'a> {
    text: &'a str,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: u32,
    temperature: f32,
    #[serde(rename = "thinkingConfig", skip_serializing_if = "Option::is_none")]
    thinking_config: Option<GeminiThinkingConfig>,
}

#[derive(Debug, Deserialize)]
struct GeminiGenerateContentResponse {
    #[serde(rename = "modelVersion")]
    model_version: Option<String>,
}

/// Decodes and classifies an HTTP status code and response payload from a Gemini `:generateContent` probe.
pub fn decode_gemini_probe_response(
    status_code: u16,
    body: &str,
    model_specifier: &str,
    latency_ms: u64,
) -> ProbeEndpointResult {
    if (200..300).contains(&status_code) {
        let resolved_model = serde_json::from_str::<GeminiGenerateContentResponse>(body)
            .ok()
            .and_then(|resp| resp.model_version)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| strip_provider_prefix(model_specifier).to_string());

        return ProbeEndpointResult {
            status: ProbeStatus::Ok,
            latency_ms,
            resolved_model: Some(resolved_model),
            error: None,
        };
    }

    let upstream_detail = extract_gemini_error_detail(body);
    let diagnostic = classify_gemini_http_error(status_code, model_specifier, &upstream_detail);
    ProbeEndpointResult {
        status: ProbeStatus::Error,
        latency_ms,
        resolved_model: None,
        error: Some(diagnostic),
    }
}

/// Live Google Gemini HTTPS implementation of [`EndpointProber`] (`POST /v1beta/models/{model}:generateContent`).
#[derive(Debug, Clone)]
pub struct GeminiEndpointProber {
    base_url: String,
    client: reqwest::Client,
}

impl Default for GeminiEndpointProber {
    fn default() -> Self {
        Self::new()
    }
}

impl GeminiEndpointProber {
    /// Constructs a new live [`GeminiEndpointProber`] targeting [`DEFAULT_GEMINI_BASE_URL`].
    pub fn new() -> Self {
        Self::with_base_url(DEFAULT_GEMINI_BASE_URL)
    }

    /// Constructs a [`GeminiEndpointProber`] with a custom base URL.
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self::with_client_and_base_url(build_pooled_http_client(), base_url)
    }

    /// Constructs a [`GeminiEndpointProber`] with a custom [`reqwest::Client`] and base URL.
    pub fn with_client_and_base_url(client: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client,
        }
    }
}

impl EndpointProber for GeminiEndpointProber {
    fn probe_endpoint(
        &self,
        model_specifier: &str,
        api_key: &str,
        timeout_ms: u64,
        thinking_level: Option<ThinkingLevel>,
        reasoning_budget: Option<u32>,
    ) -> ProbeEndpointResult {
        let payload = GeminiGenerateContentRequest {
            contents: vec![GeminiContent {
                parts: vec![GeminiPart { text: "ping" }],
            }],
            generation_config: GeminiGenerationConfig {
                max_output_tokens: 16,
                temperature: 0.0,
                thinking_config: build_probe_thinking_config(
                    model_specifier,
                    thinking_level,
                    reasoning_budget,
                ),
            },
        };

        match post_gemini_json(
            &self.client,
            &self.base_url,
            model_specifier,
            api_key,
            timeout_ms,
            &payload,
        ) {
            Ok(outcome) => decode_gemini_probe_response(
                outcome.status_code,
                &outcome.body,
                model_specifier,
                outcome.latency_ms,
            ),
            Err((err, latency_ms)) => ProbeEndpointResult {
                status: ProbeStatus::Error,
                latency_ms,
                resolved_model: None,
                error: Some(err.to_string()),
            },
        }
    }
}

/// Composite live [`EndpointProber`] routing generative `google:*` specifiers to [`GeminiEndpointProber`],
/// `google:gemini-decision-*` specifiers to [`GeminiDecisionsEndpointProber`],
/// and `typesafe:*` specifiers to [`TypesafeEndpointProber`].
#[derive(Debug, Clone)]
pub struct MultiProviderEndpointProber {
    gemini: GeminiEndpointProber,
    gemini_decisions: GeminiDecisionsEndpointProber,
    typesafe: TypesafeEndpointProber,
}

impl Default for MultiProviderEndpointProber {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiProviderEndpointProber {
    /// Constructs a new [`MultiProviderEndpointProber`] targeting default Gemini and TypeSafe endpoints.
    pub fn new() -> Self {
        let client = build_pooled_http_client();
        Self {
            gemini: GeminiEndpointProber::with_client_and_base_url(
                client.clone(),
                DEFAULT_GEMINI_BASE_URL,
            ),
            gemini_decisions: GeminiDecisionsEndpointProber::with_client_and_base_url(
                client,
                DEFAULT_GEMINI_BASE_URL,
            ),
            typesafe: TypesafeEndpointProber::new(),
        }
    }

    /// Constructs a [`MultiProviderEndpointProber`] targeting custom base URLs (for hermetic loopback tests).
    pub fn with_base_urls(
        gemini_base_url: impl Into<String>,
        typesafe_base_url: impl Into<String>,
    ) -> Self {
        let gemini_base = gemini_base_url.into();
        let client = build_pooled_http_client();
        Self {
            gemini: GeminiEndpointProber::with_client_and_base_url(
                client.clone(),
                gemini_base.clone(),
            ),
            gemini_decisions: GeminiDecisionsEndpointProber::with_client_and_base_url(
                client,
                gemini_base,
            ),
            typesafe: TypesafeEndpointProber::with_base_url(typesafe_base_url),
        }
    }
}

impl EndpointProber for MultiProviderEndpointProber {
    fn probe_endpoint(
        &self,
        model_specifier: &str,
        api_key: &str,
        timeout_ms: u64,
        thinking_level: Option<ThinkingLevel>,
        reasoning_budget: Option<u32>,
    ) -> ProbeEndpointResult {
        if model_provider(model_specifier) == TYPESAFE_PROVIDER {
            self.typesafe.probe_endpoint(
                model_specifier,
                api_key,
                timeout_ms,
                thinking_level,
                reasoning_budget,
            )
        } else if is_discriminative_model(model_specifier) {
            self.gemini_decisions.probe_endpoint(
                model_specifier,
                api_key,
                timeout_ms,
                thinking_level,
                reasoning_budget,
            )
        } else {
            self.gemini.probe_endpoint(
                model_specifier,
                api_key,
                timeout_ms,
                thinking_level,
                reasoning_budget,
            )
        }
    }
}
