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

//! Discriminative `Noul` batch screening for `apprise` (`google:gemini-decision-*`, `typesafe:jev-*`).

use super::prompt::{append_canon_section, build_apprise_intent_prefix};
use super::{AppriseError, AppriseOptions, DEFAULT_APPRISE_MAX_CONCURRENT_CALLS};
use crate::caseload::{
    ApprisalStatus, CanonApprisalAssessment, Caseload, CaseloadApprisal, HeuristicNode,
    normalize_unit_score,
};
use crate::concurrency::execute_bounded_stage_work;
use crate::discover::WorkspaceSnapshot;
use crate::docket::format_canon_batch_label;
use crate::provider::{
    DecisionBudgetProfile, DecisionRequest, DecisionResponse, ProviderError,
    StructuredModelProvider, partition_decision_items,
};
use std::collections::BTreeMap;
use std::fmt::Write as _;

const APPRISE_NOUL_QUERY_SUFFIX: &str = "\
Question: Does this candidate canon have a colorable claim of prospective subject-matter \
jurisdiction over the planned design intent and target scope in `state`?\n\
- Answer YES when the planned work touches, introduces, or modifies the domain construct, \
layer, workflow, or documentation requirement governed by this canon (even if the design already complies).\n\
- Answer NO ONLY when the canon's specific domain construct is absent from and orthogonal to the planned work.";

pub(super) fn execute_apprise_decisions(
    options: &AppriseOptions,
    caseload: Caseload,
    threshold: f64,
    workspace: &WorkspaceSnapshot,
    provider: &dyn StructuredModelProvider,
) -> Result<Caseload, AppriseError> {
    let intake = caseload
        .intake
        .as_ref()
        .ok_or(AppriseError::MissingIntent)?;
    let discovery = caseload
        .discovery
        .as_ref()
        .ok_or(AppriseError::MissingDiscovery)?;
    let apprise_model = caseload
        .config
        .as_ref()
        .and_then(|c| c.model_for(HeuristicNode::Apprise))
        .ok_or(AppriseError::MissingConfig)?
        .to_string();

    let shared_state = build_apprise_intent_prefix(intake, discovery);
    let mut rendered_items = Vec::with_capacity(discovery.candidate_canons.len());
    for canon_ref in &discovery.candidate_canons {
        let canon =
            workspace
                .canons
                .get(canon_ref)
                .ok_or_else(|| AppriseError::MissingCandidateCanon {
                    canon_ref: canon_ref.clone(),
                })?;
        let matched = discovery
            .triggers_join
            .get(canon_ref)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut q_text = String::with_capacity(640);
        append_canon_section(&mut q_text, canon_ref, canon, matched);
        let _ = write!(q_text, "{APPRISE_NOUL_QUERY_SUFFIX}");
        rendered_items.push((canon_ref.clone(), q_text));
    }

    let budget = DecisionBudgetProfile::for_model(&apprise_model);
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
                model_specifier: apprise_model.clone(),
                state: shared_state.clone(),
                questions,
                timeout_ms: options.timeout_ms,
                call_label,
            },
        ));
    }

    let concurrency_limit = options
        .max_concurrent_calls
        .unwrap_or(DEFAULT_APPRISE_MAX_CONCURRENT_CALLS);
    let assessments = run_apprise_decision_batches(
        &prepared,
        concurrency_limit,
        threshold,
        &apprise_model,
        workspace,
        provider,
    )?;

    Ok(caseload.with_apprisal(CaseloadApprisal { assessments }))
}

type BatchResult = Result<BTreeMap<String, CanonApprisalAssessment>, AppriseError>;

fn run_apprise_decision_batches(
    prepared: &[(Vec<String>, DecisionRequest)],
    concurrency_limit: usize,
    threshold: f64,
    model_specifier: &str,
    workspace: &WorkspaceSnapshot,
    provider: &dyn StructuredModelProvider,
) -> BatchResult {
    let total = prepared.len();
    let batch_maps = execute_bounded_stage_work(
        prepared,
        concurrency_limit,
        provider,
        |idx, (batch_canons, request)| {
            let batch_map = provider
                .evaluate_decisions(request)
                .and_then(|response| {
                    fold_apprise_decisions(
                        &response,
                        batch_canons,
                        threshold,
                        model_specifier,
                        workspace,
                    )
                })
                .map_err(|err| AppriseError::from_batch_call(idx + 1, total, err))?;
            let label = format_canon_batch_label(idx, total, batch_canons.len());
            Ok::<_, AppriseError>((batch_map, label))
        },
    )?;

    let mut combined = BTreeMap::new();
    for batch_map in batch_maps {
        combined.extend(batch_map);
    }
    Ok(combined)
}

fn fold_apprise_decisions(
    response: &DecisionResponse,
    batch_canons: &[String],
    threshold: f64,
    model_specifier: &str,
    workspace: &WorkspaceSnapshot,
) -> Result<BTreeMap<String, CanonApprisalAssessment>, ProviderError> {
    let mut assessments = BTreeMap::new();
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
        let apprisal_score =
            normalize_unit_score(raw_score).map_err(|detail| ProviderError::SchemaDecode {
                model_specifier: model_specifier.to_string(),
                message: format!("invalid decision score for `{canon_ref}`: {detail}"),
            })?;
        let apprisal_status = ApprisalStatus::from_score(apprisal_score, threshold);
        let apprisal_summary = match apprisal_status {
            ApprisalStatus::Applicable => {
                let invariant = workspace
                    .canons
                    .get(canon_ref)
                    .map(|c| c.invariant.trim())
                    .unwrap_or_default();
                format!(
                    "[{} {:.3}] {invariant}",
                    response.resolved_model, apprisal_score
                )
            }
            ApprisalStatus::Dismissed => format!(
                "Dismissed by discriminative apprisal screen (`{}`: {:.3}).",
                response.resolved_model, apprisal_score
            ),
        };
        assessments.insert(
            canon_ref.clone(),
            CanonApprisalAssessment {
                apprisal_summary,
                apprisal_score,
                apprisal_status,
            },
        );
    }
    Ok(assessments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apprise::execute_apprise;
    use crate::canon::ParseCanonOptions;
    use crate::caseload::{
        CaseloadConfig, CaseloadDiscovery, CaseloadIntake, CaseloadValidation,
        DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL, DiscoveryMode,
    };
    use crate::provider::{GenerationRequest, GenerationResponse, TokenUsage};

    struct StubAppriseDecisions;

    impl StructuredModelProvider for StubAppriseDecisions {
        fn generate_structured(
            &self,
            _request: &GenerationRequest,
        ) -> Result<GenerationResponse, ProviderError> {
            Err(ProviderError::UnsupportedProvider {
                provider: "typesafe".to_string(),
                model_specifier: DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL.to_string(),
            })
        }

        fn evaluate_decisions(
            &self,
            request: &DecisionRequest,
        ) -> Result<DecisionResponse, ProviderError> {
            let mut scores = BTreeMap::new();
            for key in request.questions.keys() {
                let s = if key.contains("applicable") {
                    0.96
                } else {
                    0.02
                };
                scores.insert(key.clone(), s);
            }
            Ok(DecisionResponse {
                scores,
                resolved_model: "jev-1.14.0".to_string(),
                latency_ms: 88,
                usage: TokenUsage::default(),
            })
        }
    }

    #[test]
    fn test_execute_apprise_decisions_surfaces_invariant_for_applicable_canons() {
        let app_ref = ".canons/cli/applicable-rule.md";
        let dis_ref = ".canons/cli/dismissed-rule.md";
        let ws = WorkspaceSnapshot::new()
            .with_canon_markdown(
                "---\ntriggers:\n  - \"src/cli/**\"\n---\nCLI flags MUST support environment fallbacks.\n",
                &ParseCanonOptions::with_source_ref(app_ref),
            )
            .with_canon_markdown(
                "---\ntriggers:\n  - \"src/cli/**\"\n---\nBrand logos MUST use alpha transparency.\n",
                &ParseCanonOptions::with_source_ref(dis_ref),
            );
        let caseload = Caseload::new()
            .with_intake(CaseloadIntake {
                intent: Some("Add a new CLI threshold flag".to_string()),
                target_paths: vec!["src/cli/mod.rs".to_string()],
                ..CaseloadIntake::default()
            })
            .with_discovery(CaseloadDiscovery {
                mode: DiscoveryMode::Triggered,
                candidate_canons: vec![app_ref.to_string(), dis_ref.to_string()],
                ..CaseloadDiscovery::default()
            })
            .with_validation(CaseloadValidation::default())
            .with_config(
                CaseloadConfig::default()
                    .with_node_model(HeuristicNode::Apprise, DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL),
            );

        let out = execute_apprise(
            &AppriseOptions::default(),
            caseload,
            &ws,
            &StubAppriseDecisions,
        )
        .expect("apprise decision screening");
        let apprisal = out.apprisal.expect("apprisal populated");
        assert_eq!(
            apprisal.assessments[app_ref].apprisal_status,
            ApprisalStatus::Applicable
        );
        assert!(
            apprisal.assessments[app_ref]
                .apprisal_summary
                .contains("environment fallbacks")
        );
        assert_eq!(
            apprisal.assessments[dis_ref].apprisal_status,
            ApprisalStatus::Dismissed
        );

        // Also verify default google:gemini-decision-1 takes the discriminative Noul path
        let gemini_dec_caseload = Caseload::new()
            .with_intake(CaseloadIntake {
                intent: Some("Add a new CLI threshold flag".to_string()),
                target_paths: vec!["src/cli/mod.rs".to_string()],
                ..CaseloadIntake::default()
            })
            .with_discovery(CaseloadDiscovery {
                mode: DiscoveryMode::Triggered,
                candidate_canons: vec![app_ref.to_string(), dis_ref.to_string()],
                ..CaseloadDiscovery::default()
            })
            .with_validation(CaseloadValidation::default())
            .with_config(CaseloadConfig::default());
        let gemini_out = execute_apprise(
            &AppriseOptions {
                max_canons_per_call: None,
                ..AppriseOptions::default()
            },
            gemini_dec_caseload,
            &ws,
            &StubAppriseDecisions,
        )
        .expect("gemini-decision-1 apprise screening");
        assert_eq!(
            gemini_out.apprisal.unwrap().assessments[app_ref].apprisal_status,
            ApprisalStatus::Applicable
        );
    }
}
