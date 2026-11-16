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

//! Google Gemini Decisions (`POST /v1beta/decisions`) diagnostic endpoint prober implementation.

use crate::caseload::{ProbeEndpointResult, ProbeStatus, ThinkingLevel};
use crate::probe::EndpointProber;
use crate::provider::{
    DEFAULT_GEMINI_BASE_URL, GeminiDecisionsRequest, GeminiDecisionsResponse, GeminiNoulQuestion,
    build_pooled_http_client, classify_gemini_decisions_http_error, extract_gemini_error_detail,
    post_gemini_decisions_json, strip_provider_prefix,
};
use std::collections::BTreeMap;

/// Decodes and classifies an HTTP status code and response payload from a Gemini `/v1beta/decisions` probe.
pub fn decode_gemini_decisions_probe_response(
    status_code: u16,
    body: &str,
    model_specifier: &str,
    latency_ms: u64,
) -> ProbeEndpointResult {
    if (200..300).contains(&status_code) {
        let resolved_model = serde_json::from_str::<GeminiDecisionsResponse>(body)
            .ok()
            .and_then(|resp| resp.model)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| model_specifier.to_string());

        return ProbeEndpointResult {
            status: ProbeStatus::Ok,
            latency_ms,
            model: resolved_model,
            error: None,
        };
    }

    let upstream_detail = extract_gemini_error_detail(body);
    let diagnostic =
        classify_gemini_decisions_http_error(status_code, model_specifier, &upstream_detail);
    ProbeEndpointResult {
        status: ProbeStatus::Error,
        latency_ms,
        model: model_specifier.to_string(),
        error: Some(diagnostic),
    }
}

/// Live Google Gemini Decisions HTTPS implementation of [`EndpointProber`] (`POST /v1beta/decisions`).
#[derive(Debug, Clone)]
pub struct GeminiDecisionsEndpointProber {
    base_url: String,
    client: reqwest::Client,
}

impl Default for GeminiDecisionsEndpointProber {
    fn default() -> Self {
        Self::new()
    }
}

impl GeminiDecisionsEndpointProber {
    /// Constructs a new live [`GeminiDecisionsEndpointProber`] targeting [`DEFAULT_GEMINI_BASE_URL`].
    pub fn new() -> Self {
        Self::with_base_url(DEFAULT_GEMINI_BASE_URL)
    }

    /// Constructs a [`GeminiDecisionsEndpointProber`] with a custom base URL.
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self::with_client_and_base_url(build_pooled_http_client(), base_url)
    }

    /// Constructs a [`GeminiDecisionsEndpointProber`] with a custom [`reqwest::Client`] and base URL.
    pub fn with_client_and_base_url(client: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client,
        }
    }
}

impl EndpointProber for GeminiDecisionsEndpointProber {
    fn probe_endpoint(
        &self,
        model_specifier: &str,
        api_key: &str,
        timeout_ms: u64,
        _thinking_level: Option<ThinkingLevel>,
        _reasoning_budget: Option<u32>,
    ) -> ProbeEndpointResult {
        let mut questions = BTreeMap::new();
        questions.insert(
            "ping".to_string(),
            GeminiNoulQuestion::new("Is the input non-empty?"),
        );
        let payload = GeminiDecisionsRequest {
            model: strip_provider_prefix(model_specifier).to_string(),
            input: "ping".to_string(),
            questions,
        };

        match post_gemini_decisions_json(
            &self.client,
            &self.base_url,
            model_specifier,
            api_key,
            timeout_ms,
            &payload,
        ) {
            Ok(outcome) => decode_gemini_decisions_probe_response(
                outcome.status_code,
                &outcome.body,
                model_specifier,
                outcome.latency_ms,
            ),
            Err((err, latency_ms)) => ProbeEndpointResult {
                status: ProbeStatus::Error,
                latency_ms,
                model: model_specifier.to_string(),
                error: Some(err.to_string()),
            },
        }
    }
}
