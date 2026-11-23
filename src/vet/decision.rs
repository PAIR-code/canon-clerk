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

//! Discriminative `Noul` prima facie compliance vetting and span localization for `vet`
//! (`google:gemini-decision-*`, `typesafe:jev-*`, `docs/architecture/nodes/vet.md` §4.3).

use super::prompts::{
    COMPLIANCE_QUESTION_KEY, EXCEPTION_QUESTION_KEY, build_slice_noul_items,
    file_exception_question_key, file_question_key, span_exception_question_key, span_question_key,
};
use super::slices::{DiscriminativeCasePlan, VettingExhibitSlice};
use super::{PreparedVettingCase, VetError};
use crate::caseload::{
    CanonVettingAssessment, ContestedSpan, VettingStatus, canonicalize_contested_spans,
    normalize_vetting_score,
};
use crate::concurrency::execute_bounded_stage_work;
use crate::provider::{
    DecisionBudgetProfile, DecisionRequest, ProviderError, StructuredModelProvider,
    is_decision_context_limit_error, partition_decision_items,
};
use std::collections::{BTreeMap, BTreeSet};

const QUESTION_RESERVE_CHARS: usize = 6_144;

pub(super) fn max_discriminative_state_chars_for_profile(profile: DecisionBudgetProfile) -> usize {
    profile
        .max_batch_chars
        .saturating_sub(QUESTION_RESERVE_CHARS)
}

pub(super) fn slice_fits_single_batch(
    slice: &VettingExhibitSlice,
    profile: DecisionBudgetProfile,
) -> bool {
    let items = build_slice_noul_items(slice);
    partition_decision_items(
        &items,
        slice.state.len(),
        profile.max_nouls_per_call,
        profile.max_batch_chars,
        |(_, q)| q.len(),
    )
    .len()
        <= 1
}

pub(super) struct DiscriminativeVetParams<'a> {
    pub(super) vet_model: &'a str,
    pub(super) threshold: f64,
    pub(super) exception_threshold: f64,
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
        exception_claim_score: None,
        vetting_status: VettingStatus::Contested,
        contested_exhibits: Vec::new(),
        contested_spans: Vec::new(),
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
    let mut max_exception_score = 0.0_f64;
    let mut resolved_model = params.vet_model.to_string();
    let mut contested_slices = 0usize;
    let mut all_sliceable_paths = BTreeSet::new();
    let mut contested_file_paths = BTreeSet::new();
    let mut contested_spans = Vec::new();

    for slice in slices {
        for path in &slice.file_paths {
            all_sliceable_paths.insert(path.clone());
        }
        match evaluate_slice(slice, &case.canon_ref, params, provider)? {
            SliceOutcome::Scored(scored) => {
                min_score = min_score.min(scored.min_score);
                max_exception_score = max_exception_score.max(scored.max_exception_score);
                resolved_model = scored.model;
                if VettingStatus::from_scores(
                    scored.min_score,
                    params.threshold,
                    Some(scored.max_exception_score),
                    params.exception_threshold,
                ) == VettingStatus::Contested
                {
                    contested_slices += 1;
                }
                contested_file_paths.extend(scored.contested_files);
                contested_spans.extend(scored.contested_spans);
            }
            SliceOutcome::OversizedContest => return Ok(oversized_contest_assessment()),
        }
    }

    let status = VettingStatus::from_scores(
        min_score,
        params.threshold,
        Some(max_exception_score),
        params.exception_threshold,
    );
    let contested_exhibits = if status == VettingStatus::Contested
        && !contested_file_paths.is_empty()
        && contested_file_paths.len() < all_sliceable_paths.len()
    {
        contested_file_paths.into_iter().collect()
    } else {
        Vec::new()
    };

    if status == VettingStatus::Vetted {
        contested_spans.clear();
    } else {
        canonicalize_contested_spans(&mut contested_spans);
    }

    let summary = format_vetting_summary(
        status,
        &resolved_model,
        min_score,
        params.threshold,
        max_exception_score,
        params.exception_threshold,
        slices.len(),
        contested_slices,
        &contested_exhibits,
        &contested_spans,
    );

    Ok(CanonVettingAssessment {
        vetting_summary: summary,
        vetting_score: min_score,
        exception_claim_score: Some(max_exception_score),
        vetting_status: status,
        contested_exhibits,
        contested_spans,
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "Deterministic summary formatter combining vetting status, direct/exception scores, slice counts, and localization counts"
)]
fn format_vetting_summary(
    status: VettingStatus,
    resolved_model: &str,
    min_score: f64,
    threshold: f64,
    max_exception_score: f64,
    exception_threshold: f64,
    total_slices: usize,
    contested_slices: usize,
    contested_exhibits: &[String],
    contested_spans: &[ContestedSpan],
) -> String {
    let slice_note = if total_slices > 1 && !contested_exhibits.is_empty() {
        format!(" across {total_slices} slices ({contested_slices} of {total_slices} contested)")
    } else if total_slices > 1 {
        format!(" across {total_slices} slices")
    } else {
        String::new()
    };
    let span_note = if !contested_spans.is_empty() {
        format!(" ({} localized span(s))", contested_spans.len())
    } else {
        String::new()
    };
    match status {
        VettingStatus::Vetted => format!(
            "Prima facie compliant via discriminative vetting{slice_note} (`{resolved_model}`: direct {min_score:.3} >= {threshold:.2}, exception {max_exception_score:.3} < {exception_threshold:.2})."
        ),
        VettingStatus::Contested => format!(
            "Contested via discriminative vetting{slice_note}{span_note} (`{resolved_model}`: direct {min_score:.3} vs {threshold:.2}, exception {max_exception_score:.3} vs {exception_threshold:.2}); bound over for substantive `audit`."
        ),
    }
}

struct SliceEvaluation {
    min_score: f64,
    max_exception_score: f64,
    model: String,
    contested_files: Vec<String>,
    contested_spans: Vec<ContestedSpan>,
}

enum SliceOutcome {
    Scored(SliceEvaluation),
    OversizedContest,
}

fn evaluate_slice(
    slice: &VettingExhibitSlice,
    canon_ref: &str,
    params: &DiscriminativeVetParams<'_>,
    provider: &dyn StructuredModelProvider,
) -> Result<SliceOutcome, VetError> {
    let profile = DecisionBudgetProfile::for_model(params.vet_model);
    let items = build_slice_noul_items(slice);
    let batches = partition_decision_items(
        &items,
        slice.state.len(),
        profile.max_nouls_per_call,
        profile.max_batch_chars,
        |(_, q)| q.len(),
    );

    let mut all_scores = BTreeMap::new();
    let mut resolved_model = params.vet_model.to_string();

    for batch in batches {
        let questions: BTreeMap<String, String> = batch.into_iter().collect();
        let decision_req = DecisionRequest {
            model_specifier: params.vet_model.to_string(),
            state: slice.state.clone(),
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

        resolved_model = decision_resp.resolved_model;
        for (key, raw_score) in decision_resp.scores {
            let normalized = normalize_vetting_score(raw_score).map_err(|detail| {
                VetError::Provider(ProviderError::SchemaDecode {
                    model_specifier: params.vet_model.to_string(),
                    message: format!(
                        "invalid compliance `noul` score `{key}` for `{canon_ref}`: {detail}"
                    ),
                })
            })?;
            all_scores.insert(key, normalized);
        }
    }

    let whole_score = all_scores
        .get(COMPLIANCE_QUESTION_KEY)
        .copied()
        .ok_or_else(|| {
            VetError::Provider(ProviderError::SchemaDecode {
                model_specifier: params.vet_model.to_string(),
                message: format!("missing compliance `noul` score for active case `{canon_ref}`"),
            })
        })?;
    let whole_exception_score =
        all_scores
            .get(EXCEPTION_QUESTION_KEY)
            .copied()
            .ok_or_else(|| {
                VetError::Provider(ProviderError::SchemaDecode {
                    model_specifier: params.vet_model.to_string(),
                    message: format!(
                        "missing exception-claim `noul` score for active case `{canon_ref}`"
                    ),
                })
            })?;

    Ok(SliceOutcome::Scored(apply_hierarchical_fallback(
        slice,
        whole_score,
        whole_exception_score,
        &all_scores,
        params.threshold,
        params.exception_threshold,
        resolved_model,
    )))
}

/// Applies conjunctive min direct-compliance / disjunctive max exception-claim scoring and the
/// 3-tier fail-safe localization fallback:
/// 1. Hot span(s) (`direct < threshold || exception >= exception_threshold`) -> record spans in
///    `contested_spans` and file in `contested_files`.
/// 2. File contested (`file_direct < threshold || file_exception >= exception_threshold`) with 0
///    hot spans -> retain full file in `contested_files` (no span narrowing).
/// 3. Whole slice contested (`whole_direct < threshold || whole_exception >= exception_threshold`)
///    with 0 contested files/spans -> retain full slice in `contested_files`.
fn apply_hierarchical_fallback(
    slice: &VettingExhibitSlice,
    whole_score: f64,
    whole_exception_score: f64,
    scores: &BTreeMap<String, f64>,
    threshold: f64,
    exception_threshold: f64,
    model: String,
) -> SliceEvaluation {
    let mut min_score = whole_score;
    let mut max_exception_score = whole_exception_score;
    let mut contested_files = Vec::new();
    let mut contested_spans = Vec::new();

    for file_path in &slice.file_paths {
        let file_score = scores
            .get(&file_question_key(file_path))
            .copied()
            .unwrap_or(whole_score);
        let file_exception_score = scores
            .get(&file_exception_question_key(file_path))
            .copied()
            .unwrap_or(whole_exception_score);
        min_score = min_score.min(file_score);
        max_exception_score = max_exception_score.max(file_exception_score);

        let mut hot_spans_for_file = Vec::new();
        for span in slice.spans.iter().filter(|s| &s.file_path == file_path) {
            let span_score = scores.get(&span_question_key(&span.span_id)).copied();
            let span_exception_score = scores
                .get(&span_exception_question_key(&span.span_id))
                .copied();
            if let Some(direct) = span_score {
                min_score = min_score.min(direct);
            }
            if let Some(exc) = span_exception_score {
                max_exception_score = max_exception_score.max(exc);
            }
            let direct_hot = span_score.is_some_and(|s| s < threshold);
            let exception_hot = span_exception_score.is_some_and(|e| e >= exception_threshold);
            if direct_hot || exception_hot {
                let effective_direct = span_score.unwrap_or(file_score);
                let effective_score = if let Some(exc) = span_exception_score {
                    effective_direct.min((1.0 - exc).clamp(0.0, 1.0))
                } else {
                    effective_direct
                };
                hot_spans_for_file.push(ContestedSpan {
                    path: span.file_path.clone(),
                    start_line: span.start_line,
                    end_line: span.end_line,
                    score: effective_score,
                });
            }
        }

        if !hot_spans_for_file.is_empty() {
            contested_files.push(file_path.clone());
            contested_spans.extend(hot_spans_for_file);
        } else if file_score < threshold || file_exception_score >= exception_threshold {
            // Tier 2 fallback: file contested with 0 hot spans -> keep full file without span narrowing.
            contested_files.push(file_path.clone());
        }
    }

    if (whole_score < threshold || whole_exception_score >= exception_threshold)
        && contested_files.is_empty()
    {
        // Tier 3 fallback: whole slice contested with 0 contested files/spans -> keep full slice.
        contested_files.extend(slice.file_paths.iter().cloned());
    }

    SliceEvaluation {
        min_score,
        max_exception_score,
        model,
        contested_files,
        contested_spans,
    }
}
