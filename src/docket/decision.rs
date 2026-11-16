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

//! Discriminative `Noul` batch screening for `docket` (`google:gemini-decision-*`, `typesafe:jev-*`).

use super::prompt::{append_canon_section, build_bounded_docket_filing_prefix};
use super::{
    DEFAULT_DOCKET_MAX_CONCURRENT_CALLS, DocketError, DocketOptions, format_canon_batch_label,
};
use crate::caseload::{
    CanonDocketAssessment, Caseload, CaseloadDocket, ColorabilityStatus,
    normalize_colorability_score,
};
use crate::concurrency::execute_bounded_stage_work;
use crate::discover::WorkspaceSnapshot;
use crate::provider::{
    DecisionBudgetProfile, DecisionRequest, DecisionResponse, ProviderError,
    StructuredModelProvider, partition_decision_items,
};
use std::collections::BTreeMap;
use std::fmt::Write as _;

const DOCKET_NOUL_QUERY_SUFFIX: &str = "\
Question: Does this candidate canon have a colorable claim of subject-matter jurisdiction over \
the in-flight filing in `state`?\n\
- Answer YES if the filing introduces, modifies, or deletes the specific domain construct, \
architectural boundary, or workflow governed by this canon, OR if this canon enforces an omission \
rule on a `feat` PR.\n\
- NEVER answer NO because the code appears compliant: compliant implementations remain under jurisdiction.\n\
- Answer NO when the canon's specific governed construct is absent from the diff (for example, general \
CLI edits that do not modify CLI flags, stdin, or operands do NOT fall under flag, stdin, or operand canons).";

pub(super) fn execute_docket_decisions(
    options: &DocketOptions,
    caseload: Caseload,
    threshold: f64,
    workspace: &WorkspaceSnapshot,
    provider: &dyn StructuredModelProvider,
) -> Result<Caseload, DocketError> {
    let intake = caseload.intake.as_ref().ok_or(DocketError::MissingIntake)?;
    let discovery = caseload
        .discovery
        .as_ref()
        .ok_or(DocketError::MissingDiscovery)?;
    let config = caseload.config.as_ref().ok_or(DocketError::MissingConfig)?;

    let budget = DecisionBudgetProfile::for_model(&config.screener_model);
    let shared_state =
        build_bounded_docket_filing_prefix(intake, discovery, budget.max_state_chars);
    let mut rendered_items = Vec::with_capacity(discovery.candidate_canons.len());
    for canon_ref in &discovery.candidate_canons {
        let canon =
            workspace
                .canons
                .get(canon_ref)
                .ok_or_else(|| DocketError::MissingCandidateCanon {
                    canon_ref: canon_ref.clone(),
                })?;
        let matched = discovery
            .triggers_join
            .get(canon_ref)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut q_text = String::with_capacity(640);
        append_canon_section(&mut q_text, canon_ref, canon, matched);
        let _ = write!(q_text, "{DOCKET_NOUL_QUERY_SUFFIX}");
        rendered_items.push((canon_ref.clone(), q_text));
    }

    let max_per_call = options
        .max_canons_per_call
        .map_or(budget.max_nouls_per_call, |n| {
            n.min(budget.max_nouls_per_call)
        });
    let batches = partition_decision_items(
        &rendered_items,
        shared_state.len(),
        max_per_call,
        budget.max_batch_chars,
        |(_, q)| q.len(),
    );

    let total_batches = batches.len();
    let mut prepared = Vec::with_capacity(total_batches);
    for (idx, batch_items) in batches.into_iter().enumerate() {
        let mut batch_canons = Vec::with_capacity(batch_items.len());
        let mut questions = BTreeMap::new();
        for (canon_ref, q_text) in batch_items {
            batch_canons.push(canon_ref.clone());
            questions.insert(canon_ref, q_text);
        }
        let call_label = (total_batches > 1).then(|| format!("batch {}/{total_batches}", idx + 1));
        prepared.push((
            batch_canons,
            DecisionRequest {
                model_specifier: config.screener_model.clone(),
                state: shared_state.clone(),
                questions,
                timeout_ms: options.timeout_ms,
                call_label,
            },
        ));
    }

    let concurrency_limit = options
        .max_concurrent_calls
        .unwrap_or(DEFAULT_DOCKET_MAX_CONCURRENT_CALLS);
    let cases = run_decision_batches(
        &prepared,
        concurrency_limit,
        threshold,
        &config.screener_model,
        provider,
    )?;

    Ok(caseload.with_docket(CaseloadDocket::from_cases(cases)))
}

type BatchResult = Result<BTreeMap<String, CanonDocketAssessment>, DocketError>;

fn run_decision_batches(
    prepared: &[(Vec<String>, DecisionRequest)],
    concurrency_limit: usize,
    threshold: f64,
    model_specifier: &str,
    provider: &dyn StructuredModelProvider,
) -> BatchResult {
    let total = prepared.len();
    let batch_maps = execute_bounded_stage_work(
        prepared,
        concurrency_limit,
        provider,
        |idx, (batch_canons, request)| {
            let batch_cases = provider
                .evaluate_decisions(request)
                .and_then(|response| {
                    fold_decision_cases(&response, batch_canons, threshold, model_specifier)
                })
                .map_err(|err| DocketError::from_batch_call(idx + 1, total, err))?;
            let label = format_canon_batch_label(idx, total, batch_canons.len());
            Ok::<_, DocketError>((batch_cases, label))
        },
    )?;

    let mut combined = BTreeMap::new();
    for batch_cases in batch_maps {
        combined.extend(batch_cases);
    }
    Ok(combined)
}

fn fold_decision_cases(
    response: &DecisionResponse,
    batch_canons: &[String],
    threshold: f64,
    model_specifier: &str,
) -> Result<BTreeMap<String, CanonDocketAssessment>, ProviderError> {
    let mut cases = BTreeMap::new();
    for canon_ref in batch_canons {
        let raw_score =
            response
                .scores
                .get(canon_ref)
                .copied()
                .ok_or_else(|| ProviderError::SchemaDecode {
                    model_specifier: model_specifier.to_string(),
                    message: format!("missing decision score for candidate canon `{canon_ref}`"),
                })?;
        let colorability_score = normalize_colorability_score(raw_score).map_err(|detail| {
            ProviderError::SchemaDecode {
                model_specifier: model_specifier.to_string(),
                message: format!("invalid decision score for `{canon_ref}`: {detail}"),
            }
        })?;
        let colorability_status = ColorabilityStatus::from_score(colorability_score, threshold);
        let colorability_summary = format!(
            "Discriminative jurisdiction screen (`{}`: {:.3} -> {}).",
            response.resolved_model,
            colorability_score,
            colorability_status.as_str()
        );
        cases.insert(
            canon_ref.clone(),
            CanonDocketAssessment {
                colorability_summary,
                colorability_score,
                colorability_status,
            },
        );
    }
    Ok(cases)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::caseload::{
        CaseloadConfig, CaseloadDiscovery, CaseloadIntake, CaseloadValidation, DiscoveryMode,
    };
    use crate::configure::DEFAULT_JEV_MODEL;
    use crate::docket::execute_docket;
    use crate::provider::{GenerationRequest, GenerationResponse, TokenUsage};

    struct StubDecisionScreener {
        scores: BTreeMap<String, f64>,
    }

    impl StructuredModelProvider for StubDecisionScreener {
        fn generate_structured(
            &self,
            _request: &GenerationRequest,
        ) -> Result<GenerationResponse, ProviderError> {
            Err(ProviderError::UnsupportedProvider {
                provider: "typesafe".to_string(),
                model_specifier: DEFAULT_JEV_MODEL.to_string(),
            })
        }

        fn evaluate_decisions(
            &self,
            request: &DecisionRequest,
        ) -> Result<DecisionResponse, ProviderError> {
            let mut scores = BTreeMap::new();
            for key in request.questions.keys() {
                scores.insert(
                    key.clone(),
                    self.scores.get(key).copied().unwrap_or_default(),
                );
            }
            Ok(DecisionResponse {
                scores,
                resolved_model: "jev-1.14.0".to_string(),
                latency_ms: 95,
                usage: TokenUsage {
                    prompt_tokens: 150,
                    completion_tokens: 0,
                    reasoning_tokens: 0,
                    total_tokens: 150,
                },
            })
        }
    }

    #[test]
    fn test_execute_docket_decisions_screens_candidates_via_noul_probabilities() {
        let keep_ref = ".canons/cli/keep.md";
        let drop_ref = ".canons/cli/drop.md";
        let ws = WorkspaceSnapshot::new()
            .with_canon_markdown(
                "---\ntriggers:\n  - \"src/cli/**\"\n---\nCLI flags MUST be kebab-case.\n",
                &ParseCanonOptions::with_source_ref(keep_ref),
            )
            .with_canon_markdown(
                "---\ntriggers:\n  - \"src/cli/**\"\n---\nDatabase migrations MUST include rollback.\n",
                &ParseCanonOptions::with_source_ref(drop_ref),
            );
        let caseload = Caseload::new()
            .with_intake(CaseloadIntake {
                target_paths: vec!["src/cli/mod.rs".to_string()],
                ..CaseloadIntake::default()
            })
            .with_discovery(CaseloadDiscovery {
                mode: DiscoveryMode::Triggered,
                candidate_canons: vec![drop_ref.to_string(), keep_ref.to_string()],
                ..CaseloadDiscovery::default()
            })
            .with_validation(CaseloadValidation::default())
            .with_config(CaseloadConfig {
                screener_model: DEFAULT_JEV_MODEL.to_string(),
                ..CaseloadConfig::default()
            });
        let provider = StubDecisionScreener {
            scores: BTreeMap::from([(keep_ref.to_string(), 0.92), (drop_ref.to_string(), 0.04)]),
        };

        let out = execute_docket(&DocketOptions::default(), caseload, &ws, &provider)
            .expect("docket decision screening");
        let docket = out.docket.expect("docket populated");
        assert_eq!(docket.active_docket, vec![keep_ref.to_string()]);
        assert_eq!(
            docket.cases[drop_ref].colorability_status,
            ColorabilityStatus::Dismissed
        );
    }

    #[test]
    fn test_effective_docket_batch_size_and_per_model_budget_profiles() {
        use crate::caseload::{DEFAULT_GENERATIVE_SCREENER_MODEL, DEFAULT_SCREENER_MODEL};
        use crate::docket::{DEFAULT_DOCKET_BATCH_SIZE, effective_docket_batch_size};

        assert_eq!(
            effective_docket_batch_size(None, DEFAULT_GENERATIVE_SCREENER_MODEL),
            Some(DEFAULT_DOCKET_BATCH_SIZE)
        );
        assert_eq!(
            effective_docket_batch_size(None, DEFAULT_SCREENER_MODEL),
            None
        );
        assert_eq!(effective_docket_batch_size(None, DEFAULT_JEV_MODEL), None);
        assert_eq!(
            effective_docket_batch_size(Some(12), DEFAULT_SCREENER_MODEL),
            Some(12)
        );
        assert_eq!(
            DecisionBudgetProfile::for_model(DEFAULT_SCREENER_MODEL),
            DecisionBudgetProfile {
                max_state_chars: 240_000,
                max_batch_chars: 480_000,
                max_nouls_per_call: 256,
            }
        );
        assert_eq!(
            DecisionBudgetProfile::for_model(DEFAULT_JEV_MODEL),
            DecisionBudgetProfile {
                max_state_chars: 36_000,
                max_batch_chars: 64_000,
                max_nouls_per_call: 48,
            }
        );
    }
}
