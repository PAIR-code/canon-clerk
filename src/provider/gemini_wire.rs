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

//! Encapsulated Google Gemini v1beta `:generateContent` wire schemas, request serialization, and response decoding.

use crate::caseload::{DEFAULT_PROBE_THINKING_LEVEL, ThinkingLevel};
use crate::provider::gemini::{
    classify_gemini_http_error, extract_gemini_error_detail, strip_provider_prefix,
};
use crate::provider::{GenerationRequest, GenerationResponse, ProviderError, TokenUsage};
use serde::{Deserialize, Serialize};

pub(super) const JSON_MIME_TYPE: &str = "application/json";

/// Returns `true` when `model_specifier` targets Google's Gemini 4 model family (`gemini-4-*`),
/// which requires `thinkingConfig.thinkingLevel` (`"LOW"`, `"MEDIUM"`, `"HIGH"`) rather than
/// `thinkingBudget: 0` or `"MINIMAL"`.
pub(crate) fn is_gemini_4_model(model_specifier: &str) -> bool {
    strip_provider_prefix(model_specifier).starts_with("gemini-4")
}

#[derive(Debug, Serialize)]
pub(super) struct GeminiGenerateRequest<'a> {
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction<'a>>,
    contents: Vec<GeminiRequestContent<'a>>,
    #[serde(rename = "generationConfig")]
    generation_config: GeminiGenerationConfig<'a>,
}

#[derive(Debug, Serialize)]
struct GeminiSystemInstruction<'a> {
    parts: Vec<GeminiRequestPart<'a>>,
}

#[derive(Debug, Serialize)]
struct GeminiRequestContent<'a> {
    role: &'static str,
    parts: Vec<GeminiRequestPart<'a>>,
}

#[derive(Debug, Serialize)]
struct GeminiRequestPart<'a> {
    text: &'a str,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig<'a> {
    temperature: f32,
    #[serde(rename = "responseMimeType")]
    response_mime_type: &'static str,
    #[serde(rename = "responseSchema")]
    response_schema: &'a serde_json::Value,
    #[serde(rename = "thinkingConfig", skip_serializing_if = "Option::is_none")]
    thinking_config: Option<GeminiThinkingConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct GeminiThinkingConfig {
    #[serde(rename = "includeThoughts")]
    pub(crate) include_thoughts: bool,
    #[serde(rename = "thinkingLevel", skip_serializing_if = "Option::is_none")]
    pub(crate) thinking_level: Option<&'static str>,
    #[serde(rename = "thinkingBudget", skip_serializing_if = "Option::is_none")]
    pub(crate) thinking_budget: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct GeminiGenerateResponse {
    candidates: Option<Vec<GeminiCandidate>>,
    #[serde(rename = "promptFeedback")]
    prompt_feedback: Option<GeminiPromptFeedback>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
    #[serde(rename = "modelVersion")]
    model_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiResponseContent>,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponseContent {
    parts: Option<Vec<GeminiResponsePart>>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponsePart {
    text: Option<String>,
    thought: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct GeminiPromptFeedback {
    #[serde(rename = "blockReason")]
    block_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<u64>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u64>,
    #[serde(rename = "thoughtsTokenCount")]
    thoughts_token_count: Option<u64>,
    #[serde(rename = "totalTokenCount")]
    total_token_count: Option<u64>,
}

fn build_request_thinking_config(request: &GenerationRequest) -> Option<GeminiThinkingConfig> {
    let gemini_4 = is_gemini_4_model(&request.model_specifier);
    if let Some(level) = request.thinking_level {
        if level == ThinkingLevel::Minimal && gemini_4 {
            return Some(GeminiThinkingConfig {
                include_thoughts: false,
                thinking_level: Some(ThinkingLevel::Low.gemini_wire_str()),
                thinking_budget: None,
            });
        }
        return Some(GeminiThinkingConfig {
            include_thoughts: level != ThinkingLevel::Minimal,
            thinking_level: Some(level.gemini_wire_str()),
            thinking_budget: None,
        });
    }

    request.reasoning_budget.map(|budget| {
        if budget == 0 && gemini_4 {
            GeminiThinkingConfig {
                include_thoughts: false,
                thinking_level: Some(ThinkingLevel::Low.gemini_wire_str()),
                thinking_budget: None,
            }
        } else {
            GeminiThinkingConfig {
                include_thoughts: budget > 0,
                thinking_level: None,
                thinking_budget: Some(budget),
            }
        }
    })
}

/// Builds the minimal `thinkingConfig` for `canon-clerk probe` health pings (`includeThoughts: false`),
/// defaulting Gemini 4 models to `"LOW"` when neither `thinking_level` nor `reasoning_budget` is specified.
pub(crate) fn build_probe_thinking_config(
    model_specifier: &str,
    thinking_level: Option<ThinkingLevel>,
    reasoning_budget: Option<u32>,
) -> Option<GeminiThinkingConfig> {
    let gemini_4 = is_gemini_4_model(model_specifier);
    if let Some(level) = thinking_level {
        let wire_level = if level == ThinkingLevel::Minimal && gemini_4 {
            ThinkingLevel::Low.gemini_wire_str()
        } else {
            level.gemini_wire_str()
        };
        return Some(GeminiThinkingConfig {
            include_thoughts: false,
            thinking_level: Some(wire_level),
            thinking_budget: None,
        });
    }
    if let Some(budget) = reasoning_budget {
        if budget == 0 && gemini_4 {
            return Some(GeminiThinkingConfig {
                include_thoughts: false,
                thinking_level: Some(ThinkingLevel::Low.gemini_wire_str()),
                thinking_budget: None,
            });
        }
        return Some(GeminiThinkingConfig {
            include_thoughts: false,
            thinking_level: None,
            thinking_budget: Some(budget),
        });
    }
    if gemini_4 {
        return Some(GeminiThinkingConfig {
            include_thoughts: false,
            thinking_level: Some(DEFAULT_PROBE_THINKING_LEVEL.gemini_wire_str()),
            thinking_budget: None,
        });
    }
    None
}

pub(super) fn build_request_payload(request: &GenerationRequest) -> GeminiGenerateRequest<'_> {
    let system_instruction = if request.system_instruction.trim().is_empty() {
        None
    } else {
        Some(GeminiSystemInstruction {
            parts: vec![GeminiRequestPart {
                text: &request.system_instruction,
            }],
        })
    };

    let thinking_config = build_request_thinking_config(request);

    GeminiGenerateRequest {
        system_instruction,
        contents: vec![GeminiRequestContent {
            role: "user",
            parts: vec![GeminiRequestPart {
                text: &request.user_prompt,
            }],
        }],
        generation_config: GeminiGenerationConfig {
            temperature: 0.0,
            response_mime_type: JSON_MIME_TYPE,
            response_schema: &request.response_schema,
            thinking_config,
        },
    }
}

fn is_blocking_finish_reason(reason: &str) -> bool {
    matches!(
        reason,
        "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" | "IMAGE_SAFETY"
    )
}

fn select_first_candidate(
    candidates: Option<Vec<GeminiCandidate>>,
    prompt_feedback: Option<GeminiPromptFeedback>,
    model_specifier: &str,
) -> Result<GeminiCandidate, ProviderError> {
    if let Some(reason) = prompt_feedback
        .and_then(|fb| fb.block_reason)
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty())
    {
        return Err(ProviderError::BlockedResponse {
            model_specifier: model_specifier.to_string(),
            reason: format!("prompt blocked by upstream filter (`{reason}`)"),
        });
    }

    let Some(candidate) = candidates.and_then(|mut list| {
        if list.is_empty() {
            None
        } else {
            Some(list.swap_remove(0))
        }
    }) else {
        return Err(ProviderError::BlockedResponse {
            model_specifier: model_specifier.to_string(),
            reason: "response contained zero candidates".to_string(),
        });
    };

    if let Some(reason) = candidate.finish_reason.as_deref().map(str::trim)
        && is_blocking_finish_reason(reason)
    {
        return Err(ProviderError::BlockedResponse {
            model_specifier: model_specifier.to_string(),
            reason: format!("candidate generation blocked with finishReason `{reason}`"),
        });
    }

    Ok(candidate)
}

fn extract_thoughts_and_json_text(
    candidate: GeminiCandidate,
    model_specifier: &str,
) -> Result<(Vec<String>, String), ProviderError> {
    let finish_reason = candidate.finish_reason;
    let parts = candidate
        .content
        .and_then(|content| content.parts)
        .unwrap_or_default();

    let mut thoughts = Vec::new();
    let mut json_chunks = Vec::new();

    for part in parts {
        if part.thought.unwrap_or(false) {
            if let Some(text) = part.text
                && !text.is_empty()
            {
                thoughts.push(text);
            }
        } else if let Some(text) = part.text
            && !text.trim().is_empty()
        {
            json_chunks.push(text);
        }
    }

    if json_chunks.is_empty() {
        if let Some(reason) = finish_reason
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty() && *r != "STOP")
        {
            return Err(ProviderError::BlockedResponse {
                model_specifier: model_specifier.to_string(),
                reason: format!("candidate emitted no structured output (finishReason `{reason}`)"),
            });
        }
        return Err(ProviderError::MalformedResponse {
            model_specifier: model_specifier.to_string(),
            reason: "candidate contained no structured JSON text part".to_string(),
        });
    }

    Ok((thoughts, json_chunks.join("")))
}

fn decode_json_object_payload(
    raw_json: &str,
    model_specifier: &str,
) -> Result<serde_json::Value, ProviderError> {
    let payload: serde_json::Value =
        serde_json::from_str(raw_json).map_err(|err| ProviderError::SchemaDecode {
            model_specifier: model_specifier.to_string(),
            message: format!("invalid JSON in model response ({err})"),
        })?;

    if !payload.is_object() {
        return Err(ProviderError::SchemaDecode {
            model_specifier: model_specifier.to_string(),
            message: "expected top-level JSON object matching responseSchema".to_string(),
        });
    }

    Ok(payload)
}

fn map_usage_metadata(metadata: Option<GeminiUsageMetadata>) -> TokenUsage {
    let Some(meta) = metadata else {
        return TokenUsage::default();
    };
    let prompt_tokens = meta.prompt_token_count.unwrap_or(0);
    let completion_tokens = meta.candidates_token_count.unwrap_or(0);
    let reasoning_tokens = meta.thoughts_token_count.unwrap_or(0);
    let total_tokens = meta.total_token_count.unwrap_or_else(|| {
        prompt_tokens
            .saturating_add(completion_tokens)
            .saturating_add(reasoning_tokens)
    });

    TokenUsage {
        prompt_tokens,
        completion_tokens,
        reasoning_tokens,
        total_tokens,
    }
}

pub(super) fn decode_generate_response(
    status_code: u16,
    body: &str,
    model_specifier: &str,
    latency_ms: u64,
) -> Result<GenerationResponse, ProviderError> {
    if !(200..300).contains(&status_code) {
        let detail = extract_gemini_error_detail(body);
        let diagnostic = classify_gemini_http_error(status_code, model_specifier, &detail);
        return Err(ProviderError::HttpStatus {
            status_code,
            model_specifier: model_specifier.to_string(),
            detail,
            diagnostic,
        });
    }

    let envelope: GeminiGenerateResponse =
        serde_json::from_str(body).map_err(|err| ProviderError::MalformedResponse {
            model_specifier: model_specifier.to_string(),
            reason: format!("invalid Gemini response JSON ({err})"),
        })?;

    let resolved_model = envelope
        .model_version
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| model_specifier.to_string());
    let usage = map_usage_metadata(envelope.usage_metadata);
    let candidate = select_first_candidate(
        envelope.candidates,
        envelope.prompt_feedback,
        model_specifier,
    )?;
    let (thoughts, raw_json) = extract_thoughts_and_json_text(candidate, model_specifier)?;
    let payload = decode_json_object_payload(&raw_json, model_specifier)?;

    Ok(GenerationResponse {
        payload,
        resolved_model,
        latency_ms,
        usage,
        thoughts,
    })
}
