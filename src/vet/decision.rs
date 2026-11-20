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

use super::slices::{DiscriminativeCasePlan, VettingExhibitSlice, VettingSpanTarget};
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

pub(super) const COMPLIANCE_QUESTION_KEY: &str = "compliant";
const QUESTION_RESERVE_CHARS: usize = 4_096;

const VET_NOUL_INSTRUCTION: &str = "\
Evaluate the Admitted Exhibits (and any Statutory Grounding References) in `state` strictly \
against the Governing Canon Statute's Normative Invariant (`What`) and Declared `Exception` clauses:\n\
- `Rationale` (`Why`) and `Remediation` (`How`) are non-normative; NEVER fail code that satisfies \
  the Normative Invariant via a valid alternative mechanism.\n\
- Do NOT invent extra requirements beyond the written Normative Invariant or speculate about un-admitted files \
  (when `state` presents an exhibit slice, `## All Admitted File Exhibit Paths in Filing` lists all files admitted across the filing).\n\
- If a prima facie violation satisfies the verifiable conditions of a declared `Exception`, treat the case as compliant.\n\n\
Question: Do the Admitted Exhibits in `state` comply with the Governing Canon Statute with zero unexcused violations?\n\
- Answer YES if the Admitted Exhibits comply, satisfy a declared `Exception`, OR if the canon's governed construct is absent/untouched in these exhibits (zero violation annotations required).\n\
- Answer NO ONLY if an unexcused statute violation or missing required companion update is actually present in the exhibits.";

pub(super) fn max_discriminative_state_chars(vet_model: &str) -> usize {
    DecisionBudgetProfile::for_model(vet_model)
        .max_batch_chars
        .saturating_sub(QUESTION_RESERVE_CHARS)
}

pub(super) fn file_question_key(file_path: &str) -> String {
    format!("file:{file_path}")
}

pub(super) fn span_question_key(span_id: &str) -> String {
    format!("span:{span_id}")
}

fn file_noul_instruction(file_path: &str) -> String {
    format!(
        "Evaluate ONLY the admitted file exhibit `{file_path}` in `state` against the Governing Canon \
         Statute's Normative Invariant (`What`) and Declared `Exception` clauses.\n\
         Question: Does `{file_path}` comply with the Governing Canon Statute with zero unexcused violations?\n\
         - Answer YES if `{file_path}` complies, satisfies a declared `Exception`, OR if the canon's governed \
         construct is absent/untouched in `{file_path}`.\n\
         - Answer NO ONLY if an unexcused statute violation or missing required companion update is present in `{file_path}`."
    )
}

fn span_noul_instruction(span: &VettingSpanTarget) -> String {
    format!(
        "Evaluate ONLY `[SPAN {}]` (`{}` L{}..L{}) in `state` against the Governing Canon Statute's \
         Normative Invariant (`What`) and Declared `Exception` clauses (considering enclosing scope and \
         exceptions visible elsewhere in `state`).\n\
         Question: Does `[SPAN {}]` comply with the Governing Canon Statute with zero unexcused violations?\n\
         - Answer YES if `[SPAN {}]` complies, satisfies a declared `Exception`, OR if the canon's governed \
         construct is absent/untouched in `[SPAN {}]`.\n\
         - Answer NO ONLY if an unexcused statute violation is present in `[SPAN {}]`.",
        span.span_id,
        span.file_path,
        span.start_line,
        span.end_line,
        span.span_id,
        span.span_id,
        span.span_id,
        span.span_id
    )
}

pub(super) struct DiscriminativeVetParams<'a> {
    pub(super) vet_model: &'a str,
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
                resolved_model = scored.model;
                if VettingStatus::from_score(scored.min_score, params.threshold)
                    == VettingStatus::Contested
                {
                    contested_slices += 1;
                }
                contested_file_paths.extend(scored.contested_files);
                contested_spans.extend(scored.contested_spans);
            }
            SliceOutcome::OversizedContest => return Ok(oversized_contest_assessment()),
        }
    }

    let status = VettingStatus::from_score(min_score, params.threshold);
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
        slices.len(),
        contested_slices,
        &contested_exhibits,
        &contested_spans,
    );

    Ok(CanonVettingAssessment {
        vetting_summary: summary,
        vetting_score: min_score,
        vetting_status: status,
        contested_exhibits,
        contested_spans,
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "Deterministic summary formatter combining vetting status, score, slice counts, and localization counts"
)]
fn format_vetting_summary(
    status: VettingStatus,
    resolved_model: &str,
    min_score: f64,
    threshold: f64,
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
            "Prima facie compliant via discriminative vetting{slice_note} (`{resolved_model}`: {min_score:.3} >= {threshold:.2})."
        ),
        VettingStatus::Contested => format!(
            "Contested via discriminative vetting{slice_note}{span_note} (`{resolved_model}`: {min_score:.3} < {threshold:.2}); bound over for substantive `audit`."
        ),
    }
}

struct SliceEvaluation {
    min_score: f64,
    model: String,
    contested_files: Vec<String>,
    contested_spans: Vec<ContestedSpan>,
}

enum SliceOutcome {
    Scored(SliceEvaluation),
    OversizedContest,
}

fn build_slice_noul_items(slice: &VettingExhibitSlice) -> Vec<(String, String)> {
    let mut items = Vec::with_capacity(1 + slice.file_paths.len() + slice.spans.len());
    items.push((
        COMPLIANCE_QUESTION_KEY.to_string(),
        VET_NOUL_INSTRUCTION.to_string(),
    ));
    for file_path in &slice.file_paths {
        items.push((
            file_question_key(file_path),
            file_noul_instruction(file_path),
        ));
    }
    for span in &slice.spans {
        items.push((
            span_question_key(&span.span_id),
            span_noul_instruction(span),
        ));
    }
    items
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

    Ok(SliceOutcome::Scored(apply_hierarchical_fallback(
        slice,
        whole_score,
        &all_scores,
        params.threshold,
        resolved_model,
    )))
}

/// Applies conjunctive min scoring and the 3-tier fail-safe localization fallback:
/// 1. Hot span(s) `< threshold` -> record spans in `contested_spans` and file in `contested_files`.
/// 2. `file_score < threshold` with 0 hot spans -> retain full file in `contested_files` (no span narrowing).
/// 3. `whole_score < threshold` with 0 contested files/spans -> retain full slice in `contested_files`.
fn apply_hierarchical_fallback(
    slice: &VettingExhibitSlice,
    whole_score: f64,
    scores: &BTreeMap<String, f64>,
    threshold: f64,
    model: String,
) -> SliceEvaluation {
    let mut min_score = whole_score;
    let mut contested_files = Vec::new();
    let mut contested_spans = Vec::new();

    for file_path in &slice.file_paths {
        let file_score = scores
            .get(&file_question_key(file_path))
            .copied()
            .unwrap_or(whole_score);
        min_score = min_score.min(file_score);

        let mut hot_spans_for_file = Vec::new();
        for span in slice.spans.iter().filter(|s| &s.file_path == file_path) {
            if let Some(span_score) = scores.get(&span_question_key(&span.span_id)).copied() {
                min_score = min_score.min(span_score);
                if span_score < threshold {
                    hot_spans_for_file.push(ContestedSpan {
                        path: span.file_path.clone(),
                        start_line: span.start_line,
                        end_line: span.end_line,
                        score: span_score,
                    });
                }
            }
        }

        if !hot_spans_for_file.is_empty() {
            contested_files.push(file_path.clone());
            contested_spans.extend(hot_spans_for_file);
        } else if file_score < threshold {
            // Tier 2 fallback: file contested with 0 hot spans -> keep full file without span narrowing.
            contested_files.push(file_path.clone());
        }
    }

    if whole_score < threshold && contested_files.is_empty() {
        // Tier 3 fallback: whole slice contested with 0 contested files/spans -> keep full slice.
        contested_files.extend(slice.file_paths.iter().cloned());
    }

    SliceEvaluation {
        min_score,
        model,
        contested_files,
        contested_spans,
    }
}
