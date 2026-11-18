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

//! TypeSafe AI (`/v1/systemone`) diagnostic endpoint prober implementation.

use crate::caseload::{ProbeStatus, ThinkingLevel};
use crate::probe::{EndpointProber, ProbeEndpointResult};
use crate::provider::{
    DEFAULT_TYPESAFE_BASE_URL, TypesafeNoulQuestion, TypesafeSystemOneRequest,
    TypesafeSystemOneResponse, build_pooled_http_client, classify_typesafe_http_error,
    extract_typesafe_error_detail, post_typesafe_json, strip_typesafe_prefix,
};
use std::collections::BTreeMap;

/// Decodes and classifies an HTTP status code and response payload from a TypeSafe AI `/v1/systemone` probe.
pub(super) fn decode_typesafe_probe_response(
    status_code: u16,
    body: &str,
    model_specifier: &str,
    latency_ms: u64,
) -> ProbeEndpointResult {
    if (200..300).contains(&status_code) {
        let resolved_model = serde_json::from_str::<TypesafeSystemOneResponse>(body)
            .ok()
            .and_then(|resp| resp.model)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| strip_typesafe_prefix(model_specifier).to_string());

        return ProbeEndpointResult {
            status: ProbeStatus::Ok,
            latency_ms,
            resolved_model: Some(resolved_model),
            error: None,
        };
    }

    let upstream_detail = extract_typesafe_error_detail(body);
    let diagnostic = classify_typesafe_http_error(status_code, model_specifier, &upstream_detail);
    ProbeEndpointResult {
        status: ProbeStatus::Error,
        latency_ms,
        resolved_model: None,
        error: Some(diagnostic),
    }
}

/// Live TypeSafe AI HTTPS implementation of [`EndpointProber`] (`POST /v1/systemone`).
#[derive(Debug, Clone)]
pub struct TypesafeEndpointProber {
    base_url: String,
    client: reqwest::Client,
}

impl Default for TypesafeEndpointProber {
    fn default() -> Self {
        Self::new()
    }
}

impl TypesafeEndpointProber {
    /// Constructs a new live [`TypesafeEndpointProber`] targeting [`DEFAULT_TYPESAFE_BASE_URL`].
    pub fn new() -> Self {
        Self::with_base_url(DEFAULT_TYPESAFE_BASE_URL)
    }

    /// Constructs a [`TypesafeEndpointProber`] with a custom base URL.
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self::with_client_and_base_url(build_pooled_http_client(), base_url)
    }

    /// Constructs a [`TypesafeEndpointProber`] with a custom [`reqwest::Client`] and base URL.
    pub fn with_client_and_base_url(client: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client,
        }
    }
}

impl EndpointProber for TypesafeEndpointProber {
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
            TypesafeNoulQuestion::new("Is the input non-empty?"),
        );
        let payload = TypesafeSystemOneRequest {
            model: strip_typesafe_prefix(model_specifier).to_string(),
            state: "ping".to_string(),
            questions,
        };

        match post_typesafe_json(
            &self.client,
            &self.base_url,
            model_specifier,
            api_key,
            timeout_ms,
            &payload,
        ) {
            Ok(outcome) => decode_typesafe_probe_response(
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
