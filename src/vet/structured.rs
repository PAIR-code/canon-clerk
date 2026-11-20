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

//! Standalone zero-thinking structured generative vetting (`google:*`, `docs/architecture/nodes/vet.md` §4.3).

use super::{PreparedVettingCase, VetError};
use crate::caseload::{
    CanonVettingAssessment, ResolvedNodeConfig, VettingStatus, normalize_vetting_score,
};
use crate::concurrency::execute_bounded_stage_work;
use crate::provider::{GenerationRequest, ProviderError, StructuredModelProvider};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const VET_SYSTEM_INSTRUCTION: &str = "\
You are the Clerk of the Court for Canon Clerk performing Prima Facie Compliance Vetting (`vet`).
Given ONE active Case's Governing Canon Statute, any Statutory Grounding References, and strictly the Admitted Exhibits, assess whether the filing clearly complies on its face (`vetting_summary` followed by `vetting_score` in `[0.0, 1.0]`).

Evaluation Rules:
1. Evaluate the Admitted Exhibits strictly against the Governing Canon's Normative Invariant (`What`) and any declared `Exception` clauses.
2. `Rationale` (`Why`) and `Remediation` (`How`) are non-normative; NEVER mark code contested if it satisfies the Normative Invariant via a valid alternative mechanism.
3. Polarized Score Calibration:
   - Clearly compliant (or clearly satisfies a declared `Exception`) with zero violations -> `vetting_score` in `[0.85, 1.0]`.
   - Suspected violation, missing required companion update, or subtle boundary dispute requiring judicial adjudication -> `vetting_score` in `[0.0, 0.25]`.
   - Reason concisely in `vetting_summary` BEFORE emitting `vetting_score`.";

pub(super) fn build_vet_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "vetting_summary": {
                "type": "string",
                "description": "Concise prima facie compliance rationale written BEFORE scoring."
            },
            "vetting_score": {
                "type": "number",
                "minimum": 0.0,
                "maximum": 1.0,
                "description": "Prima facie compliance score in [0.0, 1.0]: >= 0.85 when clearly compliant; <= 0.25 when contested or non-compliant."
            }
        },
        "propertyOrdering": ["vetting_summary", "vetting_score"],
        "required": ["vetting_summary", "vetting_score"]
    })
}

#[derive(Deserialize)]
struct WireVetAssessment {
    vetting_summary: String,
    vetting_score: f64,
}

fn decode_vet_payload(
    payload: &Value,
    canon_ref: &str,
    threshold: f64,
    model_specifier: &str,
) -> Result<CanonVettingAssessment, VetError> {
    let wire: WireVetAssessment = serde_json::from_value(payload.clone()).map_err(|err| {
        VetError::Provider(ProviderError::SchemaDecode {
            model_specifier: model_specifier.to_string(),
            message: format!("invalid structured `vet` payload for `{canon_ref}`: {err}"),
        })
    })?;
    let summary = wire.vetting_summary.trim();
    if summary.is_empty() {
        return Err(VetError::Provider(ProviderError::SchemaDecode {
            model_specifier: model_specifier.to_string(),
            message: format!("empty `vetting_summary` returned for `{canon_ref}`"),
        }));
    }
    let vetting_score = normalize_vetting_score(wire.vetting_score).map_err(|detail| {
        VetError::Provider(ProviderError::SchemaDecode {
            model_specifier: model_specifier.to_string(),
            message: format!("invalid `vetting_score` for `{canon_ref}`: {detail}"),
        })
    })?;
    let vetting_status = VettingStatus::from_score(vetting_score, threshold);
    Ok(CanonVettingAssessment {
        vetting_summary: summary.to_string(),
        vetting_score,
        vetting_status,
        contested_exhibits: Vec::new(),
        contested_spans: Vec::new(),
    })
}

pub(super) struct GenerativeVetParams<'a> {
    pub(super) vet_node: &'a ResolvedNodeConfig,
    pub(super) threshold: f64,
    pub(super) concurrency_limit: usize,
    pub(super) timeout_ms: u64,
}

pub(super) fn execute_generative_vetting(
    prepared_cases: &[PreparedVettingCase],
    params: &GenerativeVetParams<'_>,
    provider: &dyn StructuredModelProvider,
) -> Result<BTreeMap<String, CanonVettingAssessment>, VetError> {
    let response_schema = build_vet_schema();
    let (thinking_level, reasoning_budget) = params.vet_node.effective_screening_thinking();
    let prepared: Vec<(&PreparedVettingCase, GenerationRequest)> = prepared_cases
        .iter()
        .map(|case| {
            let req = GenerationRequest {
                model_specifier: params.vet_node.model.clone(),
                system_instruction: VET_SYSTEM_INSTRUCTION.to_string(),
                user_prompt: case.user_prompt.clone(),
                response_schema: response_schema.clone(),
                thinking_level,
                reasoning_budget,
                timeout_ms: params.timeout_ms,
                call_label: Some(case.canon_ref.clone()),
            };
            (case, req)
        })
        .collect();

    let evaluated = execute_bounded_stage_work(
        &prepared,
        params.concurrency_limit,
        provider,
        |_idx, (case, req)| {
            let resp = provider.generate_structured(req)?;
            let decoded = decode_vet_payload(
                &resp.payload,
                &case.canon_ref,
                params.threshold,
                &params.vet_node.model,
            )?;
            let label = format!("{} ({})", case.canon_ref, decoded.vetting_status.as_str());
            Ok::<_, VetError>(((case.canon_ref.clone(), decoded), label))
        },
    )?;

    Ok(evaluated.into_iter().collect())
}
