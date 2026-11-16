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

//! Discriminative `Noul` prima facie compliance vetting for `vet` (`google:gemini-decision-*`, `typesafe:jev-*`, `docs/architecture/nodes/vet.md` §4.3).

use super::slices::DiscriminativeCasePlan;
use super::{PreparedVettingCase, VetError};
use crate::caseload::{CanonVettingAssessment, VettingStatus, normalize_vetting_score};
use crate::concurrency::execute_bounded_stage_work;
use crate::provider::{
    DecisionBudgetProfile, DecisionRequest, ProviderError, StructuredModelProvider,
    is_decision_context_limit_error,
};
use std::collections::BTreeMap;

pub(super) const COMPLIANCE_QUESTION_KEY: &str = "compliant";

const VET_NOUL_INSTRUCTION: &str = "\
Evaluate the Admitted Exhibits (and any Statutory Grounding References) in `state` strictly \
against the Governing Canon Statute's Normative Invariant (`What`) and Declared `Exception` clauses:\n\
- `Rationale` (`Why`) and `Remediation` (`How`) are non-normative; NEVER fail code that satisfies \
  the Normative Invariant via a valid alternative mechanism.\n\
- Do NOT invent extra requirements beyond the written Normative Invariant or speculate about un-admitted files \
  (when `state` presents an exhibit slice, `## All Admitted File Exhibit Paths in Filing` lists all files admitted across the filing).\n\
- If a prima facie violation satisfies the verifiable conditions of a declared `Exception`, treat the case as compliant.\n\n\
Question: Do the Admitted Exhibits in `state` comply with the Governing Canon Statute with zero unexcused violations?\n\
- Answer YES only if the Admitted Exhibits comply (or satisfy a declared `Exception`) and require zero violation annotations.\n\
- Answer NO if any unexcused statute violation or missing required companion update is present.";

pub(super) fn max_discriminative_state_chars(screener_model: &str) -> usize {
    DecisionBudgetProfile::for_model(screener_model)
        .max_batch_chars
        .saturating_sub(VET_NOUL_INSTRUCTION.len() + 128)
}

pub(super) struct DiscriminativeVetParams<'a> {
    pub(super) screener_model: &'a str,
    pub(super) threshold: f64,
    pub(super) concurrency_limit: usize,
    pub(super) timeout_ms: u64,
}

pub(super) fn execute_discriminative_vetting(
    prepared_cases: &[PreparedVettingCase],
    params: &DiscriminativeVetParams<'_>,
    provider: &dyn StructuredModelProvider,
) -> Result<BTreeMap<String, CanonVettingAssessment>, VetError> {
    let evaluated = execute_bounded_stage_work(
        prepared_cases,
        params.concurrency_limit,
        provider,
        |_idx, case| {
            let assessment = evaluate_single_decision_case(case, params, provider)?;
            let label = format!(
                "{} ({})",
                case.canon_ref,
                assessment.vetting_status.as_str()
            );
            Ok::<_, VetError>(((case.canon_ref.clone(), assessment), label))
        },
    )?;

    Ok(evaluated.into_iter().collect())
}

fn oversized_contest_assessment() -> CanonVettingAssessment {
    CanonVettingAssessment {
        vetting_summary:
            "Admitted exhibits exceed discriminative context window; bound over for long-context generative `audit`."
                .to_string(),
        vetting_score: 0.0,
        vetting_status: VettingStatus::Contested,
        contested_exhibits: Vec::new(),
    }
}

fn evaluate_single_decision_case(
    case: &PreparedVettingCase,
    params: &DiscriminativeVetParams<'_>,
    provider: &dyn StructuredModelProvider,
) -> Result<CanonVettingAssessment, VetError> {
    let DiscriminativeCasePlan::Slices(ref slices) = case.discriminative_plan else {
        return Ok(oversized_contest_assessment());
    };
    if slices.is_empty() {
        return Ok(oversized_contest_assessment());
    }

    let mut min_score = 1.0_f64;
    let mut resolved_model = params.screener_model.to_string();
    let mut cleared_slices = 0usize;
    let mut contested_slices = 0usize;
    let mut failed_slice_paths = Vec::new();

    for slice in slices {
        match evaluate_slice(&slice.state, &case.canon_ref, params, provider)? {
            SliceOutcome::Scored { score, model } => {
                min_score = min_score.min(score);
                resolved_model = model;
                if VettingStatus::from_score(score, params.threshold) == VettingStatus::Vetted {
                    cleared_slices += 1;
                } else {
                    contested_slices += 1;
                    failed_slice_paths.extend(slice.file_paths.iter().cloned());
                }
            }
            SliceOutcome::OversizedContest => return Ok(oversized_contest_assessment()),
        }
    }

    let status = VettingStatus::from_score(min_score, params.threshold);
    let contested_exhibits =
        if status == VettingStatus::Contested && cleared_slices > 0 && contested_slices > 0 {
            failed_slice_paths.sort();
            failed_slice_paths.dedup();
            failed_slice_paths
        } else {
            Vec::new()
        };

    let slice_note = if slices.len() > 1 && !contested_exhibits.is_empty() {
        format!(
            " across {} slices ({contested_slices} of {} contested)",
            slices.len(),
            slices.len()
        )
    } else if slices.len() > 1 {
        format!(" across {} slices", slices.len())
    } else {
        String::new()
    };
    let summary = match status {
        VettingStatus::Vetted => format!(
            "Prima facie compliant via discriminative vetting{slice_note} (`{resolved_model}`: {min_score:.3} >= {:.2}).",
            params.threshold
        ),
        VettingStatus::Contested => format!(
            "Contested via discriminative vetting{slice_note} (`{resolved_model}`: {min_score:.3} < {:.2}); bound over for substantive `audit`.",
            params.threshold
        ),
    };

    Ok(CanonVettingAssessment {
        vetting_summary: summary,
        vetting_score: min_score,
        vetting_status: status,
        contested_exhibits,
    })
}

enum SliceOutcome {
    Scored { score: f64, model: String },
    OversizedContest,
}

fn evaluate_slice(
    slice_state: &str,
    canon_ref: &str,
    params: &DiscriminativeVetParams<'_>,
    provider: &dyn StructuredModelProvider,
) -> Result<SliceOutcome, VetError> {
    let mut questions = BTreeMap::new();
    questions.insert(
        COMPLIANCE_QUESTION_KEY.to_string(),
        VET_NOUL_INSTRUCTION.to_string(),
    );
    let decision_req = DecisionRequest {
        model_specifier: params.screener_model.to_string(),
        state: slice_state.to_string(),
        questions,
        timeout_ms: params.timeout_ms,
        call_label: Some(canon_ref.to_string()),
    };

    let decision_resp = match provider.evaluate_decisions(&decision_req) {
        Ok(resp) => resp,
        Err(ProviderError::HttpStatus {
            status_code: 400,
            ref detail,
            ..
        }) if is_decision_context_limit_error(detail) => {
            return Ok(SliceOutcome::OversizedContest);
        }
        Err(err) => return Err(VetError::Provider(err)),
    };

    let raw_score = decision_resp
        .scores
        .get(COMPLIANCE_QUESTION_KEY)
        .copied()
        .ok_or_else(|| {
            VetError::Provider(ProviderError::SchemaDecode {
                model_specifier: params.screener_model.to_string(),
                message: format!("missing compliance `noul` score for active case `{canon_ref}`"),
            })
        })?;
    let score = normalize_vetting_score(raw_score).map_err(|detail| {
        VetError::Provider(ProviderError::SchemaDecode {
            model_specifier: params.screener_model.to_string(),
            message: format!("invalid compliance `noul` score for `{canon_ref}`: {detail}"),
        })
    })?;

    Ok(SliceOutcome::Scored {
        score,
        model: decision_resp.resolved_model,
    })
}
