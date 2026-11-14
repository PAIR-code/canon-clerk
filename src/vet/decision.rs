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

//! Discriminative `Noul` prima facie compliance vetting for `vet` (`typesafe:jev-*`, `docs/architecture/nodes/vet.md` §4.3).

use super::{PreparedVettingCase, VetError};
use crate::caseload::{CanonVettingAssessment, VettingStatus, normalize_vetting_score};
use crate::provider::{
    DecisionRequest, MAX_DECISION_BATCH_CHARS, ProviderError, StructuredModelProvider,
};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub(super) const COMPLIANCE_QUESTION_KEY: &str = "compliant";

const VET_NOUL_INSTRUCTION: &str = "\
Evaluate the Admitted Exhibits (and any Statutory Grounding References) in `state` strictly \
against the Governing Canon Statute's Normative Invariant (`What`) and Declared `Exception` clauses:\n\
- `Rationale` (`Why`) and `Remediation` (`How`) are non-normative; NEVER fail code that satisfies \
  the Normative Invariant via a valid alternative mechanism.\n\
- Do NOT invent extra requirements beyond the written Normative Invariant or speculate about un-admitted files.\n\
- If a prima facie violation satisfies the verifiable conditions of a declared `Exception`, treat the case as compliant.\n\n\
Question: Do the Admitted Exhibits comply with the Governing Canon Statute with zero unexcused violations?\n\
- Answer YES only if the Admitted Exhibits comply (or satisfy a declared `Exception`) and require zero violation annotations.\n\
- Answer NO if any unexcused statute violation or missing required companion update is present.";

pub(super) struct DiscriminativeVetParams<'a> {
    pub(super) screener_model: &'a str,
    pub(super) threshold: f64,
    pub(super) concurrency_limit: usize,
    pub(super) timeout_ms: u64,
    pub(super) allow_oversized_contest_fallback: bool,
}

type VetCaseSlot = Option<Result<(String, CanonVettingAssessment), VetError>>;

pub(super) fn execute_discriminative_vetting(
    prepared_cases: &[PreparedVettingCase],
    params: &DiscriminativeVetParams<'_>,
    provider: &dyn StructuredModelProvider,
) -> Result<BTreeMap<String, CanonVettingAssessment>, VetError> {
    let worker_count = params.concurrency_limit.min(prepared_cases.len()).max(1);
    if worker_count == 1 {
        let mut cases = BTreeMap::new();
        for case in prepared_cases {
            let assessment = evaluate_single_decision_case(case, params, provider)?;
            cases.insert(case.canon_ref.clone(), assessment);
        }
        return Ok(cases);
    }

    let next_index = AtomicUsize::new(0);
    let aborted = AtomicBool::new(false);
    let outcomes: Mutex<Vec<VetCaseSlot>> =
        Mutex::new((0..prepared_cases.len()).map(|_| None).collect());

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| {
                loop {
                    if aborted.load(Ordering::Relaxed) {
                        break;
                    }
                    let idx = next_index.fetch_add(1, Ordering::SeqCst);
                    if idx >= prepared_cases.len() {
                        break;
                    }
                    let case = &prepared_cases[idx];
                    let res = evaluate_single_decision_case(case, params, provider)
                        .map(|assessment| (case.canon_ref.clone(), assessment));
                    if res.is_err() {
                        aborted.store(true, Ordering::Relaxed);
                    }
                    if let Ok(mut guard) = outcomes.lock() {
                        guard[idx] = Some(res);
                    }
                }
            });
        }
    });

    let collected = outcomes.into_inner().unwrap_or_default();
    let mut cases = BTreeMap::new();
    for slot in collected.into_iter().flatten() {
        let (canon_ref, assessment) = slot?;
        cases.insert(canon_ref, assessment);
    }
    Ok(cases)
}

fn oversized_contest_assessment() -> CanonVettingAssessment {
    CanonVettingAssessment {
        vetting_summary:
            "Admitted exhibits exceed discriminative context window; bound over for long-context generative `audit`."
                .to_string(),
        vetting_score: 0.0,
        vetting_status: VettingStatus::Contested,
    }
}

fn evaluate_single_decision_case(
    case: &PreparedVettingCase,
    params: &DiscriminativeVetParams<'_>,
    provider: &dyn StructuredModelProvider,
) -> Result<CanonVettingAssessment, VetError> {
    if case.user_prompt.len() + VET_NOUL_INSTRUCTION.len() + 64 > MAX_DECISION_BATCH_CHARS {
        if params.allow_oversized_contest_fallback {
            return Ok(oversized_contest_assessment());
        }
        let detail = format!(
            "case `{}` admitted exhibits exceed discriminative context budget (`{MAX_DECISION_BATCH_CHARS}` chars)",
            case.canon_ref
        );
        let diagnostic = format!(
            "TypeSafe context window exceeded for `{}`: {detail}.\n  \
             Hint: Run `canon-clerk audit` or pass `--screener-model flash-lite` for long-context filings.",
            params.screener_model
        );
        return Err(VetError::Provider(ProviderError::HttpStatus {
            model_specifier: params.screener_model.to_string(),
            status_code: 400,
            detail,
            diagnostic,
        }));
    }

    let mut questions = BTreeMap::new();
    questions.insert(
        COMPLIANCE_QUESTION_KEY.to_string(),
        VET_NOUL_INSTRUCTION.to_string(),
    );
    let decision_req = DecisionRequest {
        model_specifier: params.screener_model.to_string(),
        state: case.user_prompt.clone(),
        questions,
        timeout_ms: params.timeout_ms,
    };

    let decision_resp = match provider.evaluate_decisions(&decision_req) {
        Ok(resp) => resp,
        Err(ProviderError::HttpStatus {
            status_code: 400,
            ref detail,
            ..
        }) if params.allow_oversized_contest_fallback && detail.contains("max_tokens_exceeded") => {
            return Ok(oversized_contest_assessment());
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
                message: format!(
                    "missing compliance `noul` score for active case `{}`",
                    case.canon_ref
                ),
            })
        })?;
    let vetting_score = normalize_vetting_score(raw_score).map_err(|detail| {
        VetError::Provider(ProviderError::SchemaDecode {
            model_specifier: params.screener_model.to_string(),
            message: format!(
                "invalid compliance `noul` score for `{}`: {detail}",
                case.canon_ref
            ),
        })
    })?;

    let status = VettingStatus::from_score(vetting_score, params.threshold);
    let summary = match status {
        VettingStatus::Vetted => format!(
            "Prima facie compliant via discriminative vetting (`{}`: {:.3} >= {:.2}).",
            decision_resp.resolved_model, vetting_score, params.threshold
        ),
        VettingStatus::Contested => format!(
            "Contested via discriminative vetting (`{}`: {:.3} < {:.2}); bound over for substantive `audit`.",
            decision_resp.resolved_model, vetting_score, params.threshold
        ),
    };

    Ok(CanonVettingAssessment {
        vetting_summary: summary,
        vetting_score,
        vetting_status: status,
    })
}
