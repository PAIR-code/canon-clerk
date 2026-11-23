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

//! Core domain execution engine for the `audit` Caseload DAG stage (`docs/architecture/nodes/audit.md`).

mod decode;
mod error;
mod exhibits;
mod fold;
pub(crate) mod metadata;
mod prompt;
mod reference_overlay;
mod slice_pack;
mod slices;

pub use error::AuditError;
pub use exhibits::resolve_canon_reference_paths;
pub(crate) use exhibits::{
    extract_file_exhibit_lines, format_file_exhibit_heading, is_metadata_plane_id,
};
pub(crate) use prompt::build_audit_case_user_prompt;
pub(crate) use reference_overlay::{
    apply_base_diff_to_references, collect_base_reference_disk_paths,
};
pub(crate) use slice_pack::{
    RenderedFileExhibit, format_pinned_reference_exhibits, pack_rendered_exhibit_slices,
};
pub(crate) use slices::{build_shared_case_header, partition_and_render_file_exhibits};

use crate::caseload::{
    CanonAdjudication, Caseload, CaseloadVerdict, HeuristicNode, ResolvedNodeConfig, VerdictStatus,
    normalize_compliance_score,
};
use crate::concurrency::execute_bounded_stage_work;
use crate::configure::is_discriminative_model;
use crate::discover::WorkspaceSnapshot;
use crate::provider::{GenerationRequest, StructuredModelProvider};
use decode::decode_case_adjudication;
use fold::fold_case_slice_adjudications;
use prompt::{AUDIT_SYSTEM_INSTRUCTION, build_audit_schema};
use slices::{MAX_AUDIT_TRIAL_CHARS, build_audit_case_slices};
use std::collections::{BTreeMap, BTreeSet};

/// Default substantive compliance threshold for `audit` (`docs/architecture/nodes/audit.md` §4.5).
pub const DEFAULT_AUDIT_THRESHOLD: f64 = 0.5;

/// Default maximum concurrent in-flight per-case trial calls during `audit`.
pub const DEFAULT_AUDIT_MAX_CONCURRENT_CALLS: usize = 4;

/// Default per-trial timeout ceiling in milliseconds (`60_000ms` / 60 seconds) for bounded slice trials.
pub const DEFAULT_AUDIT_TIMEOUT_MS: u64 = 60_000;

/// Environment variable name for overriding `--audit-threshold` (`docs/architecture/nodes/audit.md` §5).
pub const ENV_AUDIT_THRESHOLD: &str = "CANON_CLERK_AUDIT_THRESHOLD";

/// Environment variable name for overriding `--audit-concurrency` (`docs/architecture/nodes/audit.md` §5).
pub const ENV_AUDIT_CONCURRENCY: &str = "CANON_CLERK_AUDIT_CONCURRENCY";

/// Domain input parameters for executing the `audit` pipeline stage.
#[derive(Debug, Clone, PartialEq)]
pub struct AuditOptions {
    /// Substantive compliance score threshold in `[0.0, 1.0]` (defaults to [`DEFAULT_AUDIT_THRESHOLD`]).
    pub threshold: f64,
    /// Per-request timeout ceiling in milliseconds (defaults to [`DEFAULT_AUDIT_TIMEOUT_MS`]).
    pub timeout_ms: u64,
    /// Optional maximum concurrent in-flight per-case trial calls (`>= 1`; defaults to [`DEFAULT_AUDIT_MAX_CONCURRENT_CALLS`]).
    pub max_concurrent_calls: Option<usize>,
}

impl Default for AuditOptions {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_AUDIT_THRESHOLD,
            timeout_ms: DEFAULT_AUDIT_TIMEOUT_MS,
            max_concurrent_calls: None,
        }
    }
}

pub(super) struct PreparedTrialCall {
    pub(super) canon_ref: String,
    pub(super) canon_id: String,
    pub(super) slice_index: usize,
    pub(super) slice_count: usize,
    pub(super) annotatable_file_paths: BTreeSet<String>,
    pub(super) primary_fallback_path: Option<String>,
    pub(super) request: GenerationRequest,
}

fn validate_audit_options(options: &AuditOptions) -> Result<f64, AuditError> {
    let threshold = normalize_compliance_score(options.threshold)
        .map_err(|detail| AuditError::InvalidThreshold { detail })?;
    if options.max_concurrent_calls == Some(0) {
        return Err(AuditError::InvalidConcurrency {
            detail: format!(
                "max_concurrent_calls must be at least 1, got 0 (see `{ENV_AUDIT_CONCURRENCY}`)"
            ),
        });
    }
    Ok(threshold)
}

fn partition_vetted_and_trial_refs<'a>(
    vetting: Option<&'a crate::caseload::CaseloadVetting>,
    evidence: &'a crate::caseload::CaseloadEvidence,
    threshold: f64,
) -> (Vec<CanonAdjudication>, &'a [String]) {
    match vetting {
        Some(vt) => {
            let carried = vt
                .vetted_cases
                .iter()
                .filter_map(|canon_ref| {
                    vt.cases.get(canon_ref).map(|a| CanonAdjudication {
                        canon_path: canon_ref.clone(),
                        compliance_summary: a.vetting_summary.clone(),
                        compliance_score: a.vetting_score,
                        status: VerdictStatus::from_score(a.vetting_score, threshold),
                        annotations: Vec::new(),
                    })
                })
                .collect();
            (carried, vt.contested_cases.as_slice())
        }
        None => (Vec::new(), evidence.active_cases.as_slice()),
    }
}

fn contested_exhibit_subset_for(
    vetting: Option<&crate::caseload::CaseloadVetting>,
    canon_ref: &str,
) -> Option<BTreeSet<String>> {
    let assessment = vetting?.cases.get(canon_ref)?;
    if assessment.contested_exhibits.is_empty() {
        None
    } else {
        Some(assessment.contested_exhibits.iter().cloned().collect())
    }
}

fn contested_spans_by_file_for(
    vetting: Option<&crate::caseload::CaseloadVetting>,
    canon_ref: &str,
) -> BTreeMap<String, Vec<(usize, usize)>> {
    let Some(assessment) = vetting.and_then(|v| v.cases.get(canon_ref)) else {
        return BTreeMap::new();
    };
    let mut grouped: BTreeMap<String, Vec<(usize, usize)>> = BTreeMap::new();
    for span in &assessment.contested_spans {
        grouped
            .entry(span.path.clone())
            .or_default()
            .push((span.start_line, span.end_line));
    }
    grouped
}

/// Executes the pure `audit` domain stage against an in-memory [`WorkspaceSnapshot`] and
/// hexagonal [`StructuredModelProvider`] (`docs/architecture/nodes/audit.md` §4).
pub fn execute_audit(
    options: &AuditOptions,
    caseload: Caseload,
    workspace: &WorkspaceSnapshot,
    provider: &dyn StructuredModelProvider,
) -> Result<Caseload, AuditError> {
    let threshold = validate_audit_options(options)?;

    if caseload.verdict.is_some() {
        return Ok(caseload);
    }

    let intake = caseload.intake.as_ref().ok_or(AuditError::MissingIntake)?;
    let discovery = caseload
        .discovery
        .as_ref()
        .ok_or(AuditError::MissingDiscovery)?;

    if caseload
        .validation
        .as_ref()
        .is_some_and(|val| val.has_errors)
    {
        return Err(AuditError::ValidationFailed);
    }

    let evidence = caseload
        .evidence
        .as_ref()
        .ok_or(AuditError::MissingEvidence)?;
    if evidence.active_cases.is_empty() {
        return Ok(caseload.with_verdict(CaseloadVerdict::default()));
    }

    let (mut adjudications, trial_canon_refs) =
        partition_vetted_and_trial_refs(caseload.vetting.as_ref(), evidence, threshold);

    let mut active_trials = Vec::new();
    for canon_ref in trial_canon_refs {
        let canon =
            workspace
                .canons
                .get(canon_ref)
                .ok_or_else(|| AuditError::MissingActiveCanon {
                    canon_ref: canon_ref.clone(),
                })?;
        let admitted = evidence.admitted_exhibits_for(canon_ref);
        if !admitted.is_empty() {
            let contested_subset =
                contested_exhibit_subset_for(caseload.vetting.as_ref(), canon_ref);
            let contested_spans = contested_spans_by_file_for(caseload.vetting.as_ref(), canon_ref);
            active_trials.push((
                canon_ref.clone(),
                canon,
                admitted,
                contested_subset,
                contested_spans,
            ));
        }
    }

    if active_trials.is_empty() {
        return Ok(caseload.with_verdict(CaseloadVerdict::from_adjudications(adjudications)));
    }

    let node_cfg = caseload
        .config
        .as_ref()
        .and_then(|c| c.node(HeuristicNode::Audit))
        .ok_or(AuditError::MissingConfig)?;
    if is_discriminative_model(&node_cfg.model) {
        return Err(AuditError::DiscriminativeAuditUnsupported {
            specifier: node_cfg.model.clone(),
        });
    }

    let audit_model = node_cfg.model.clone();
    let (prepared_calls, case_slice_counts) = prepare_all_trial_calls(
        active_trials,
        node_cfg,
        options.timeout_ms,
        intake,
        discovery,
        workspace,
    );

    let concurrency_limit = options
        .max_concurrent_calls
        .unwrap_or(DEFAULT_AUDIT_MAX_CONCURRENT_CALLS);
    let mut slice_adjudications = execute_trial_calls(
        &prepared_calls,
        concurrency_limit,
        threshold,
        &audit_model,
        provider,
    )?;

    let mut drain_iter = slice_adjudications.drain(..);
    for (canon_ref, count) in case_slice_counts {
        let case_decrees: Vec<CanonAdjudication> = drain_iter.by_ref().take(count).collect();
        adjudications.push(fold_case_slice_adjudications(
            &canon_ref,
            case_decrees,
            threshold,
        ));
    }

    Ok(caseload.with_verdict(CaseloadVerdict::from_adjudications(adjudications)))
}

type ActiveTrialEntry<'a> = (
    String,
    &'a crate::canon::Canon,
    Vec<&'a crate::caseload::ExhibitAssessment>,
    Option<BTreeSet<String>>,
    BTreeMap<String, Vec<(usize, usize)>>,
);

fn prepare_all_trial_calls(
    active_trials: Vec<ActiveTrialEntry<'_>>,
    node_cfg: &ResolvedNodeConfig,
    timeout_ms: u64,
    intake: &crate::caseload::CaseloadIntake,
    discovery: &crate::caseload::CaseloadDiscovery,
    workspace: &WorkspaceSnapshot,
) -> (Vec<PreparedTrialCall>, Vec<(String, usize)>) {
    let response_schema = build_audit_schema();
    let (effective_thinking_level, effective_reasoning_budget) =
        node_cfg.effective_audit_thinking();
    let mut prepared_calls = Vec::with_capacity(active_trials.len());
    let mut case_slice_counts = Vec::with_capacity(active_trials.len());

    for (canon_ref, canon, admitted_exhibits, contested_subset, contested_spans) in active_trials {
        let slices = build_audit_case_slices(
            &canon_ref,
            canon,
            &admitted_exhibits,
            contested_subset.as_ref(),
            &contested_spans,
            intake,
            discovery,
            workspace,
            MAX_AUDIT_TRIAL_CHARS,
        );
        let slice_count = slices.len();
        for (idx, slice) in slices.into_iter().enumerate() {
            let slice_index = idx + 1;
            let call_label = if slice_count > 1 {
                format!("{canon_ref} [slice {slice_index}/{slice_count}]")
            } else {
                canon_ref.clone()
            };
            let request = GenerationRequest {
                model_specifier: node_cfg.model.clone(),
                system_instruction: AUDIT_SYSTEM_INSTRUCTION.to_string(),
                user_prompt: slice.user_prompt,
                response_schema: response_schema.clone(),
                thinking_level: effective_thinking_level,
                reasoning_budget: effective_reasoning_budget,
                timeout_ms,
                call_label: Some(call_label),
            };
            prepared_calls.push(PreparedTrialCall {
                canon_ref: canon_ref.clone(),
                canon_id: canon.id.clone(),
                slice_index,
                slice_count,
                annotatable_file_paths: slice.annotatable_file_paths,
                primary_fallback_path: slice.primary_fallback_path,
                request,
            });
        }
        case_slice_counts.push((canon_ref, slice_count));
    }
    (prepared_calls, case_slice_counts)
}

type TrialResult = Result<CanonAdjudication, AuditError>;

fn evaluate_single_trial_call(
    call: &PreparedTrialCall,
    threshold: f64,
    model_specifier: &str,
    provider: &dyn StructuredModelProvider,
) -> TrialResult {
    let response = provider.generate_structured(&call.request).map_err(|err| {
        AuditError::from_case_call(&call.canon_ref, call.slice_index, call.slice_count, err)
    })?;
    decode_case_adjudication(
        &response.payload,
        &call.canon_ref,
        &call.canon_id,
        &call.annotatable_file_paths,
        call.primary_fallback_path.as_deref(),
        threshold,
        model_specifier,
    )
    .map_err(|err| {
        AuditError::from_case_call(&call.canon_ref, call.slice_index, call.slice_count, err)
    })
}

fn execute_trial_calls(
    prepared_calls: &[PreparedTrialCall],
    concurrency_limit: usize,
    threshold: f64,
    model_specifier: &str,
    provider: &dyn StructuredModelProvider,
) -> Result<Vec<CanonAdjudication>, AuditError> {
    execute_bounded_stage_work(prepared_calls, concurrency_limit, provider, |_idx, call| {
        let adjudication = evaluate_single_trial_call(call, threshold, model_specifier, provider)?;
        let label = format!("{} ({})", call.canon_ref, adjudication.status.as_str());
        Ok((adjudication, label))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::caseload::{
        AdmissibilityStatus, AnnotationLevel, CanonDocketAssessment, CaseloadConfig,
        CaseloadDiscovery, CaseloadDocket, CaseloadEvidence, CaseloadIntake, CaseloadValidation,
        ColorabilityStatus, DEFAULT_GOOGLE_GENERATOR_MODEL, DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL,
        DiscoveryMode, ExhibitAssessment, FileArtifact, FileChangeStatus, ThinkingLevel,
        VerdictStatus,
    };
    use crate::provider::{GenerationResponse, ProviderError, TokenUsage};
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    struct RoutingAuditProvider {
        requests: Mutex<Vec<GenerationRequest>>,
        routes: BTreeMap<String, Result<Value, ProviderError>>,
    }

    impl RoutingAuditProvider {
        fn new(routes: BTreeMap<String, Result<Value, ProviderError>>) -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
                routes,
            }
        }

        fn empty() -> Self {
            Self::new(BTreeMap::new())
        }

        fn captured_requests(&self) -> Vec<GenerationRequest> {
            self.requests.lock().expect("lock requests").clone()
        }
    }

    impl StructuredModelProvider for RoutingAuditProvider {
        fn generate_structured(
            &self,
            request: &GenerationRequest,
        ) -> Result<GenerationResponse, ProviderError> {
            self.requests
                .lock()
                .expect("lock requests")
                .push(request.clone());
            for (needle, outcome) in &self.routes {
                if request.user_prompt.contains(needle) {
                    return match outcome {
                        Ok(payload) => Ok(GenerationResponse {
                            payload: payload.clone(),
                            thoughts: vec!["Step 1: Checking invariant...".to_string()],
                            usage: TokenUsage {
                                prompt_tokens: 410,
                                completion_tokens: 95,
                                reasoning_tokens: 220,
                                total_tokens: 725,
                            },
                            latency_ms: 340,
                            resolved_model: "gemini-2.5-pro".to_string(),
                        }),
                        Err(err) => Err(err.clone()),
                    };
                }
            }
            Err(ProviderError::SchemaDecode {
                model_specifier: request.model_specifier.clone(),
                message: "no mock route matched trial prompt".to_string(),
            })
        }
    }

    fn sample_workspace() -> WorkspaceSnapshot {
        WorkspaceSnapshot::new()
            .with_canon_markdown(
                "---\ntriggers:\n  - \"src/cli/**\"\ninspect:\n  - diff\n  - pr_title?\n---\n# CLI Flags Kebab Case\n\nCLI flags MUST use kebab-case naming.\n\n**Exception:** Legacy v0 aliases explicitly marked `hide = true`.\n",
                &ParseCanonOptions::with_source_ref(".canons/cli/cli-flags-kebab-case.canon.md"),
            )
            .with_canon_markdown(
                "---\ntriggers:\n  - \"**/*\"\nreferences:\n  - \"README.md\"\ninspect:\n  - diff\n  - pr_title\n---\n# Feat PRs Must Update README\n\nPull requests introducing new user-facing features (`feat`) MUST update `README.md`.\n",
                &ParseCanonOptions::with_source_ref(".canons/readme/feat-updates-readme.canon.md"),
            )
            .with_canon_markdown(
                "---\ntriggers:\n  - \"**/*\"\ninspect:\n  - pr_title\n---\n# Conventional Commit Title\n\nPR titles MUST use lowercase type prefixes.\n",
                &ParseCanonOptions::with_source_ref(".canons/git/pr-title-lowercase.canon.md"),
            )
            .with_file_content("README.md", "# Canon Clerk\n\nExisting documentation.\n")
    }

    fn sample_intake_diffs() -> BTreeMap<String, FileArtifact> {
        let mut diffs = BTreeMap::new();
        diffs.insert(
            "src/cli/audit.rs".to_string(),
            FileArtifact {
                path: "src/cli/audit.rs".to_string(),
                status: FileChangeStatus::Added,
                previous_path: None,
                lines_added: 3,
                lines_deleted: 0,
                patch: Some(
                    "@@ -0,0 +1,3 @@\n+pub struct AuditArgs {\n+    #[arg(long = \"auditThreshold\")]\n+}\n"
                        .to_string(),
                ),
                patch_omission_reason: None,
                content: None,
                content_omission_reason: None,
            },
        );
        diffs.insert(
            "src/internal/excluded_helper.rs".to_string(),
            FileArtifact {
                path: "src/internal/excluded_helper.rs".to_string(),
                status: FileChangeStatus::Modified,
                previous_path: None,
                lines_added: 1,
                lines_deleted: 0,
                patch: Some("@@ -1,1 +1,2 @@\n+// SECRET_EXCLUDED_SENTINEL\n".to_string()),
                patch_omission_reason: None,
                content: None,
                content_omission_reason: None,
            },
        );
        diffs
    }

    fn sample_admitted_caseload() -> Caseload {
        let diffs = sample_intake_diffs();
        let mut exhibits = BTreeMap::new();
        exhibits.insert(
            ".canons/cli/cli-flags-kebab-case.canon.md".to_string(),
            vec![
                ExhibitAssessment {
                    file_path: "src/cli/audit.rs".to_string(),
                    admissibility_summary: "Defines `--auditThreshold` flag.".to_string(),
                    admissibility_score: 0.95,
                    admissibility_status: AdmissibilityStatus::Admitted,
                },
                ExhibitAssessment {
                    file_path: "src/internal/excluded_helper.rs".to_string(),
                    admissibility_summary: "Unrelated internal helper.".to_string(),
                    admissibility_score: 0.08,
                    admissibility_status: AdmissibilityStatus::Excluded,
                },
            ],
        );
        exhibits.insert(
            ".canons/readme/feat-updates-readme.canon.md".to_string(),
            vec![
                ExhibitAssessment {
                    file_path: "pr_title".to_string(),
                    admissibility_summary: "`feat(cli):` triggers README requirement.".to_string(),
                    admissibility_score: 0.92,
                    admissibility_status: AdmissibilityStatus::Admitted,
                },
                ExhibitAssessment {
                    file_path: "src/cli/audit.rs".to_string(),
                    admissibility_summary: "Adds new user-facing CLI command.".to_string(),
                    admissibility_score: 0.89,
                    admissibility_status: AdmissibilityStatus::Admitted,
                },
            ],
        );

        let mut docket_cases = BTreeMap::new();
        for c in [
            ".canons/cli/cli-flags-kebab-case.canon.md",
            ".canons/readme/feat-updates-readme.canon.md",
        ] {
            docket_cases.insert(
                c.to_string(),
                CanonDocketAssessment {
                    colorability_summary: "Docketed.".to_string(),
                    colorability_score: 0.9,
                    colorability_status: ColorabilityStatus::Docketed,
                },
            );
        }

        Caseload::new()
            .with_intake(CaseloadIntake {
                pr_title: Some("feat(cli): add audit subcommand".to_string()),
                diffs,
                ..CaseloadIntake::default()
            })
            .with_discovery(CaseloadDiscovery {
                mode: DiscoveryMode::Triggered,
                target_files: vec![
                    "src/cli/audit.rs".to_string(),
                    "src/internal/excluded_helper.rs".to_string(),
                ],
                candidate_canons: vec![
                    ".canons/cli/cli-flags-kebab-case.canon.md".to_string(),
                    ".canons/readme/feat-updates-readme.canon.md".to_string(),
                ],
                ..CaseloadDiscovery::default()
            })
            .with_validation(CaseloadValidation::default())
            .with_config(CaseloadConfig::default().with_node(
                HeuristicNode::Audit,
                ResolvedNodeConfig::with_thinking(DEFAULT_GOOGLE_GENERATOR_MODEL, None, Some(4096)),
            ))
            .with_docket(CaseloadDocket::from_cases(docket_cases))
            .with_evidence(CaseloadEvidence::from_exhibits(exhibits))
    }

    mod guards_and_short_circuits {
        use super::*;

        #[test]
        fn test_empty_active_cases_short_circuits_with_zero_calls_and_no_config_needed() {
            let ws = sample_workspace();
            let provider = RoutingAuditProvider::empty();
            let mut caseload = sample_admitted_caseload();
            caseload.config = None;
            caseload.evidence = Some(CaseloadEvidence::default());

            let out = execute_audit(&AuditOptions::default(), caseload, &ws, &provider)
                .expect("empty active_cases must short-circuit cleanly");
            assert!(provider.captured_requests().is_empty());
            let verdict = out.verdict.expect("verdict populated");
            assert!(verdict.is_pass());
            assert!(verdict.adjudications.is_empty());
        }

        #[test]
        fn test_enforces_prerequisites_and_fast_forwards_populated_verdict() {
            let ws = sample_workspace();
            let provider = RoutingAuditProvider::empty();

            assert!(matches!(
                execute_audit(
                    &AuditOptions {
                        threshold: -0.1,
                        ..AuditOptions::default()
                    },
                    Caseload::new(),
                    &ws,
                    &provider
                )
                .unwrap_err(),
                AuditError::InvalidThreshold { .. }
            ));
            assert!(matches!(
                execute_audit(
                    &AuditOptions {
                        max_concurrent_calls: Some(0),
                        ..AuditOptions::default()
                    },
                    Caseload::new(),
                    &ws,
                    &provider
                )
                .unwrap_err(),
                AuditError::InvalidConcurrency { .. }
            ));
            assert_eq!(
                execute_audit(&AuditOptions::default(), Caseload::new(), &ws, &provider)
                    .unwrap_err(),
                AuditError::MissingIntake
            );

            let pre_populated = Caseload::new()
                .with_intake(CaseloadIntake::default())
                .with_verdict(CaseloadVerdict::default());
            let ff = execute_audit(
                &AuditOptions::default(),
                pre_populated.clone(),
                &ws,
                &provider,
            )
            .expect("fast-forward");
            assert_eq!(ff, pre_populated);
            assert!(provider.captured_requests().is_empty());
        }
    }

    mod trial_isolation_references_and_annotation_guardrails {
        use super::*;

        fn two_violation_routes() -> BTreeMap<String, Result<Value, ProviderError>> {
            let mut routes = BTreeMap::new();
            routes.insert(
                ".canons/cli/cli-flags-kebab-case.canon.md".to_string(),
                Ok(json!({
                    "compliance_summary": "Flag `--auditThreshold` uses camelCase instead of kebab-case `--audit-threshold`.",
                    "compliance_score": 0.08,
                    "annotations": [{
                        "path": "src/cli/audit.rs",
                        "start_line": 2,
                        "end_line": 2,
                        "start_column": 20,
                        "end_column": 12,
                        "annotation_level": "failure",
                        "message": "Rename `--auditThreshold` to `--audit-threshold`."
                    }]
                })),
            );
            routes.insert(
                ".canons/readme/feat-updates-readme.canon.md".to_string(),
                Ok(json!({
                    "compliance_summary": "`feat(cli):` adds a user-facing subcommand without updating `README.md`.",
                    "compliance_score": 0.10,
                    "annotations": [{
                        "path": "README.md",
                        "start_line": 1,
                        "end_line": 1,
                        "annotation_level": "failure",
                        "title": "Missing README Update",
                        "message": "Document `canon-clerk audit` in `README.md`."
                    }]
                })),
            );
            routes
        }

        #[test]
        fn test_concurrent_trials_enforce_zero_leakage_and_inject_references() {
            let ws = sample_workspace();
            let provider = RoutingAuditProvider::new(two_violation_routes());
            let _ = execute_audit(
                &AuditOptions {
                    max_concurrent_calls: Some(2),
                    ..AuditOptions::default()
                },
                sample_admitted_caseload(),
                &ws,
                &provider,
            )
            .expect("concurrent audit trials");

            let requests = provider.captured_requests();
            assert_eq!(requests.len(), 2);
            for req in &requests {
                assert_eq!(req.model_specifier, DEFAULT_GOOGLE_GENERATOR_MODEL);
                assert_eq!(req.reasoning_budget, Some(4096));
                assert!(!req.user_prompt.contains("SECRET_EXCLUDED_SENTINEL"));
                assert!(req.user_prompt.contains("+   2 |"));
            }
            assert!(
                requests
                    .iter()
                    .any(|r| r.user_prompt.contains("Existing documentation."))
            );
        }

        #[test]
        fn test_decoder_reanchors_omission_paths_and_normalizes_columns() {
            let ws = sample_workspace();
            let provider = RoutingAuditProvider::new(two_violation_routes());
            let enriched = execute_audit(
                &AuditOptions::default(),
                sample_admitted_caseload(),
                &ws,
                &provider,
            )
            .expect("audit trials");

            let verdict = enriched.verdict.expect("verdict populated");
            assert_eq!(verdict.status, VerdictStatus::Fail);
            assert_eq!(verdict.failed_count(), 2);

            let cli_adj = &verdict.adjudications[0];
            assert_eq!(cli_adj.annotations[0].path, "src/cli/audit.rs");
            assert_eq!(cli_adj.annotations[0].start_column, Some(20));
            assert_eq!(cli_adj.annotations[0].end_column, Some(20));
            assert_eq!(
                cli_adj.annotations[0].title.as_deref(),
                Some("cli-flags-kebab-case")
            );

            let readme_adj = &verdict.adjudications[1];
            assert_eq!(readme_adj.annotations[0].path, "src/cli/audit.rs");
            assert_eq!(readme_adj.annotations[0].start_line, 1);
            assert_eq!(
                readme_adj.annotations[0].annotation_level,
                AnnotationLevel::Failure
            );
        }

        #[test]
        fn test_exception_pass_and_metadata_only_violation_emit_empty_annotations() {
            let ws = sample_workspace();
            let mut exhibits = BTreeMap::new();
            exhibits.insert(
                ".canons/cli/cli-flags-kebab-case.canon.md".to_string(),
                vec![ExhibitAssessment {
                    file_path: "src/cli/audit.rs".to_string(),
                    admissibility_summary: "Legacy hidden alias.".to_string(),
                    admissibility_score: 0.90,
                    admissibility_status: AdmissibilityStatus::Admitted,
                }],
            );
            exhibits.insert(
                ".canons/git/pr-title-lowercase.canon.md".to_string(),
                vec![ExhibitAssessment {
                    file_path: "pr_title".to_string(),
                    admissibility_summary: "PR title prefix.".to_string(),
                    admissibility_score: 0.95,
                    admissibility_status: AdmissibilityStatus::Admitted,
                }],
            );

            let mut caseload = sample_admitted_caseload();
            caseload.evidence = Some(CaseloadEvidence::from_exhibits(exhibits));

            let mut routes = BTreeMap::new();
            routes.insert(
                ".canons/cli/cli-flags-kebab-case.canon.md".to_string(),
                Ok(json!({
                    "compliance_summary": "Satisfies declared Exception: legacy alias is marked `hide = true`.",
                    "compliance_score": 0.88,
                    "annotations": []
                })),
            );
            routes.insert(
                ".canons/git/pr-title-lowercase.canon.md".to_string(),
                Ok(json!({
                    "compliance_summary": "PR title `FEAT: ...` uses uppercase prefix; change to `feat: ...`.",
                    "compliance_score": 0.05,
                    "annotations": [{
                        "path": "pr_title",
                        "start_line": 1,
                        "end_line": 1,
                        "annotation_level": "failure",
                        "message": "Lowercase `FEAT` to `feat`."
                    }]
                })),
            );

            let provider = RoutingAuditProvider::new(routes);
            let out = execute_audit(&AuditOptions::default(), caseload, &ws, &provider)
                .expect("exception pass + metadata fail");
            let verdict = out.verdict.expect("verdict");
            assert_eq!(verdict.status, VerdictStatus::Fail);
            assert_eq!(verdict.passed_count(), 1);
            assert_eq!(verdict.failed_count(), 1);
            assert!(verdict.adjudications[0].annotations.is_empty());
            assert!(
                verdict.adjudications[1].annotations.is_empty(),
                "metadata-only violations must strip non-file `pr_title` annotations"
            );
        }
    }

    mod vetting_fold_forward_and_contested_isolation {
        use super::*;
        use crate::caseload::{CanonVettingAssessment, CaseloadVetting, VettingStatus};
        use crate::provider::{DecisionRequest, DecisionResponse};
        use crate::vet::{VetOptions, execute_vet};

        #[test]
        fn test_folds_vetted_cases_and_evaluates_only_contested_cases_or_short_circuits_when_all_vetted()
         {
            let ws = sample_workspace();
            let mut vetting_cases = BTreeMap::new();
            vetting_cases.insert(
                ".canons/cli/cli-flags-kebab-case.canon.md".to_string(),
                CanonVettingAssessment {
                    vetting_summary: "Prima facie compliant via discriminative vetting."
                        .to_string(),
                    vetting_score: 0.96,
                    exception_claim_score: None,
                    vetting_status: VettingStatus::Vetted,
                    contested_exhibits: Vec::new(),
                    contested_spans: Vec::new(),
                },
            );
            vetting_cases.insert(
                ".canons/readme/feat-updates-readme.canon.md".to_string(),
                CanonVettingAssessment {
                    vetting_summary: "Contested: missing README update.".to_string(),
                    vetting_score: 0.14,
                    exception_claim_score: None,
                    vetting_status: VettingStatus::Contested,
                    contested_exhibits: Vec::new(),
                    contested_spans: Vec::new(),
                },
            );

            let caseload =
                sample_admitted_caseload().with_vetting(CaseloadVetting::from_cases(vetting_cases));
            let mut routes = BTreeMap::new();
            routes.insert(
                ".canons/readme/feat-updates-readme.canon.md".to_string(),
                Ok(json!({
                    "compliance_summary": "`feat(cli):` omits `README.md` update.",
                    "compliance_score": 0.10,
                    "annotations": [{
                        "path": "src/cli/audit.rs",
                        "start_line": 1,
                        "end_line": 1,
                        "annotation_level": "failure",
                        "message": "Update `README.md`."
                    }]
                })),
            );
            let provider = RoutingAuditProvider::new(routes);

            let out = execute_audit(&AuditOptions::default(), caseload, &ws, &provider)
                .expect("audit with vetting");
            assert_eq!(
                provider.captured_requests().len(),
                1,
                "only the 1 contested case must invoke audit node model"
            );
            let verdict = out.verdict.expect("verdict");
            assert_eq!(verdict.adjudications.len(), 2);
            assert_eq!(verdict.passed_count(), 1);
            assert_eq!(verdict.failed_count(), 1);

            // All-vetted caseload short-circuits with zero provider calls and no .config required
            let mut all_vetted = BTreeMap::new();
            for c in [
                ".canons/cli/cli-flags-kebab-case.canon.md",
                ".canons/readme/feat-updates-readme.canon.md",
            ] {
                all_vetted.insert(
                    c.to_string(),
                    CanonVettingAssessment {
                        vetting_summary: "Vetted.".to_string(),
                        vetting_score: 0.95,
                        exception_claim_score: None,
                        vetting_status: VettingStatus::Vetted,
                        contested_exhibits: Vec::new(),
                        contested_spans: Vec::new(),
                    },
                );
            }
            let mut no_cfg =
                sample_admitted_caseload().with_vetting(CaseloadVetting::from_cases(all_vetted));
            no_cfg.config = None;
            let zero_provider = RoutingAuditProvider::empty();
            let clean = execute_audit(&AuditOptions::default(), no_cfg, &ws, &zero_provider)
                .expect("all-vetted short-circuit");
            assert!(zero_provider.captured_requests().is_empty());
            assert_eq!(clean.verdict.expect("verdict").passed_count(), 2);
        }

        struct DualStageMockProvider {
            vet_violation_marker: String,
            audit_requests: Mutex<Vec<GenerationRequest>>,
            audit_routes: Vec<(String, Value)>,
            default_audit_payload: Value,
        }

        impl StructuredModelProvider for DualStageMockProvider {
            fn evaluate_decisions(
                &self,
                request: &DecisionRequest,
            ) -> Result<DecisionResponse, ProviderError> {
                let score = if request.state.contains(&self.vet_violation_marker) {
                    0.18
                } else {
                    0.94
                };
                Ok(DecisionResponse {
                    scores: BTreeMap::from([
                        ("compliant".to_string(), score),
                        ("claims_exception".to_string(), 0.05),
                    ]),
                    resolved_model: "jev-1.14.0".to_string(),
                    latency_ms: 40,
                    usage: TokenUsage {
                        prompt_tokens: 150,
                        completion_tokens: 0,
                        reasoning_tokens: 0,
                        total_tokens: 150,
                    },
                })
            }

            fn generate_structured(
                &self,
                request: &GenerationRequest,
            ) -> Result<GenerationResponse, ProviderError> {
                self.audit_requests
                    .lock()
                    .expect("lock")
                    .push(request.clone());
                for (needle, payload) in &self.audit_routes {
                    if request.user_prompt.contains(needle) {
                        return Ok(GenerationResponse {
                            payload: payload.clone(),
                            resolved_model: "gemini-3.1-pro".to_string(),
                            latency_ms: 120,
                            usage: TokenUsage {
                                prompt_tokens: 400,
                                completion_tokens: 60,
                                reasoning_tokens: 120,
                                total_tokens: 580,
                            },
                            thoughts: Vec::new(),
                        });
                    }
                }
                Ok(GenerationResponse {
                    payload: self.default_audit_payload.clone(),
                    resolved_model: "gemini-3.1-pro".to_string(),
                    latency_ms: 100,
                    usage: TokenUsage {
                        prompt_tokens: 350,
                        completion_tokens: 40,
                        reasoning_tokens: 80,
                        total_tokens: 470,
                    },
                    thoughts: Vec::new(),
                })
            }
        }

        fn large_multi_module_fixture(
            canon_ref: &str,
            module_count: usize,
        ) -> (WorkspaceSnapshot, Caseload) {
            let ws = WorkspaceSnapshot::new()
                .with_canon_markdown(
                    "---\ntriggers:\n  - \"src/**\"\nreferences:\n  - \"README.md\"\n---\nEvery public CLI module MUST document flags in `README.md`.\n\nException: Internal helper modules marked `// internal` are exempt.\n",
                    &ParseCanonOptions::with_source_ref(canon_ref),
                )
                .with_file_content("README.md", "# CLI Docs\n");

            let mut diffs = BTreeMap::from([(
                "README.md".to_string(),
                FileArtifact {
                    path: "README.md".to_string(),
                    previous_path: None,
                    status: FileChangeStatus::Modified,
                    lines_added: 2,
                    lines_deleted: 0,
                    patch: Some("@@ -1,1 +1,3 @@\n+PINNED_README_DIFF_SENTINEL\n".to_string()),
                    patch_omission_reason: None,
                    content: None,
                    content_omission_reason: None,
                },
            )]);
            let mut admitted = vec![
                ExhibitAssessment {
                    file_path: "pr_title".to_string(),
                    admissibility_summary: "PR title".to_string(),
                    admissibility_score: 0.90,
                    admissibility_status: AdmissibilityStatus::Admitted,
                },
                ExhibitAssessment {
                    file_path: "README.md".to_string(),
                    admissibility_summary: "companion README".to_string(),
                    admissibility_score: 0.96,
                    admissibility_status: AdmissibilityStatus::Admitted,
                },
            ];

            let body: String = (1..=360)
                .map(|ln| format!("+pub fn cli_handler_fn_{ln}() -> usize {{ {ln} }}\n"))
                .collect();
            for idx in 1..=module_count {
                let path = format!("src/mod_{idx}.rs");
                let sentinel = format!("// MODULE_{idx}_BODY_SENTINEL\n");
                diffs.insert(
                    path.clone(),
                    FileArtifact {
                        path: path.clone(),
                        previous_path: None,
                        status: FileChangeStatus::Added,
                        lines_added: 361,
                        lines_deleted: 0,
                        patch: Some(format!("@@ -0,0 +1,361 @@\n+{sentinel}{body}")),
                        patch_omission_reason: None,
                        content: None,
                        content_omission_reason: None,
                    },
                );
                admitted.push(ExhibitAssessment {
                    file_path: path,
                    admissibility_summary: "module diff".to_string(),
                    admissibility_score: 0.91,
                    admissibility_status: AdmissibilityStatus::Admitted,
                });
            }

            let caseload = Caseload::new()
                .with_intake(CaseloadIntake {
                    pr_title: Some("feat(cli): add modular CLI handlers".to_string()),
                    diffs,
                    ..CaseloadIntake::default()
                })
                .with_discovery(CaseloadDiscovery {
                    mode: DiscoveryMode::Triggered,
                    candidate_canons: vec![canon_ref.to_string()],
                    ..CaseloadDiscovery::default()
                })
                .with_validation(CaseloadValidation::default())
                .with_config(
                    CaseloadConfig::default()
                        .with_node_model(HeuristicNode::Vet, DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL)
                        .with_node(
                            HeuristicNode::Audit,
                            ResolvedNodeConfig::with_thinking(
                                DEFAULT_GOOGLE_GENERATOR_MODEL,
                                None,
                                Some(2048),
                            ),
                        ),
                )
                .with_docket(CaseloadDocket::from_cases(BTreeMap::from([(
                    canon_ref.to_string(),
                    CanonDocketAssessment {
                        colorability_summary: "docketed".to_string(),
                        colorability_score: 0.95,
                        colorability_status: ColorabilityStatus::Docketed,
                    },
                )])))
                .with_evidence(CaseloadEvidence::from_exhibits(BTreeMap::from([(
                    canon_ref.to_string(),
                    admitted,
                )])));
            (ws, caseload)
        }

        #[test]
        fn test_vet_to_audit_prunes_cleared_slices_while_retaining_pinned_references_metadata_and_manifest()
         {
            let canon_ref = ".canons/readme/broad-cli-rule.md";
            let (ws, caseload) = large_multi_module_fixture(canon_ref, 6);

            let provider = DualStageMockProvider {
                vet_violation_marker: "MODULE_5_BODY_SENTINEL".to_string(),
                audit_requests: Mutex::new(Vec::new()),
                audit_routes: vec![(
                    "MODULE_5_BODY_SENTINEL".to_string(),
                    json!({
                        "compliance_summary": "`src/mod_5.rs` adds undocumented CLI flags; document them in `README.md`.",
                        "compliance_score": 0.12,
                        "annotations": [
                            {
                                "path": "src/mod_5.rs",
                                "start_line": 2,
                                "end_line": 2,
                                "annotation_level": "failure",
                                "message": "Document handler flags in `README.md`."
                            },
                            {
                                "path": "src/mod_1.rs",
                                "start_line": 10,
                                "end_line": 10,
                                "annotation_level": "warning",
                                "message": "Pruned-file citation must re-anchor to contested slice exhibit."
                            }
                        ]
                    }),
                )],
                default_audit_payload: json!({
                    "compliance_summary": "Unexpected call.",
                    "compliance_score": 0.95,
                    "annotations": []
                }),
            };

            let after_vet =
                execute_vet(&VetOptions::default(), caseload, &ws, &provider).expect("vet");
            let contested_exhibits = after_vet.vetting.as_ref().unwrap().cases[canon_ref]
                .contested_exhibits
                .clone();
            assert!(
                contested_exhibits.contains(&"src/mod_5.rs".to_string())
                    && !contested_exhibits.contains(&"src/mod_1.rs".to_string())
                    && contested_exhibits.len() < 6,
                "expected partial contested subset containing src/mod_5.rs, got {contested_exhibits:?}"
            );

            let after_audit =
                execute_audit(&AuditOptions::default(), after_vet, &ws, &provider).expect("audit");
            let requests = provider.audit_requests.lock().unwrap();
            assert_eq!(
                requests.len(),
                1,
                "pruning cleared slices must leave a single bounded audit request"
            );
            let prompt = &requests[0].user_prompt;
            assert!(prompt.contains("MODULE_5_BODY_SENTINEL"));
            assert!(
                !prompt.contains("MODULE_1_BODY_SENTINEL"),
                "vet-cleared slice diff bodies must be pruned from audit prompt"
            );
            assert!(
                prompt.contains("PINNED_README_DIFF_SENTINEL"),
                "admitted companion `references:` diffs must remain pinned even when cleared"
            );
            assert!(
                prompt.contains("### Admitted Metadata Exhibit: `pr_title`"),
                "admitted metadata planes must remain present"
            );
            assert!(
                prompt.contains("## All Admitted File Exhibit Paths in Filing")
                    && prompt.contains("`src/mod_1.rs`")
                    && prompt.contains("`src/mod_6.rs`")
                    && prompt.contains("`README.md`"),
                "full filing path manifest must include all admitted files across the filing"
            );

            let verdict = after_audit.verdict.expect("verdict");
            assert_eq!(verdict.status, VerdictStatus::Fail);
            let adj = &verdict.adjudications[0];
            assert!(
                adj.annotations
                    .iter()
                    .all(|a| contested_exhibits.contains(&a.path) && a.path != "src/mod_1.rs"),
                "annotations must anchor strictly to contested slice exhibits, got {:?}",
                adj.annotations
            );
        }

        #[test]
        fn test_unpruned_oversized_audit_slices_concurrently_and_folds_failing_decrees() {
            let canon_ref = ".canons/readme/broad-cli-rule.md";
            let (ws, caseload) = large_multi_module_fixture(canon_ref, 6);

            let failing_provider = DualStageMockProvider {
                vet_violation_marker: String::new(),
                audit_requests: Mutex::new(Vec::new()),
                audit_routes: vec![
                    (
                        "MODULE_2_BODY_SENTINEL".to_string(),
                        json!({
                            "compliance_summary": "Undocumented flag in mod_2.",
                            "compliance_score": 0.16,
                            "annotations": [{
                                "path": "src/mod_2.rs",
                                "start_line": 4,
                                "end_line": 4,
                                "annotation_level": "failure",
                                "message": "Add mod_2 flag to README.md."
                            }]
                        }),
                    ),
                    (
                        "MODULE_6_BODY_SENTINEL".to_string(),
                        json!({
                            "compliance_summary": "Undocumented flag in mod_6.",
                            "compliance_score": 0.09,
                            "annotations": [{
                                "path": "src/mod_6.rs",
                                "start_line": 7,
                                "end_line": 7,
                                "annotation_level": "failure",
                                "message": "Add mod_6 flag to README.md."
                            }]
                        }),
                    ),
                ],
                default_audit_payload: json!({
                    "compliance_summary": "Slice complies.",
                    "compliance_score": 0.93,
                    "annotations": []
                }),
            };

            let fail_out = execute_audit(
                &AuditOptions {
                    max_concurrent_calls: Some(3),
                    ..AuditOptions::default()
                },
                caseload,
                &ws,
                &failing_provider,
            )
            .expect("concurrent multi-slice failing audit");
            let fail_reqs = failing_provider.audit_requests.lock().unwrap();
            assert!(
                fail_reqs.len() >= 2,
                "oversized 6-module case without vetting must partition into multiple trial slices"
            );
            assert!(
                fail_reqs
                    .iter()
                    .all(|r| r.user_prompt.len() <= MAX_AUDIT_TRIAL_CHARS + 1024),
                "each trial slice prompt must stay bounded near MAX_AUDIT_TRIAL_CHARS"
            );
            let fail_adj = &fail_out.verdict.unwrap().adjudications[0];
            assert_eq!(fail_adj.status, VerdictStatus::Fail);
            assert!((fail_adj.compliance_score - 0.09).abs() < 1e-9);
            assert_eq!(fail_adj.annotations.len(), 2);
            assert_eq!(fail_adj.annotations[0].path, "src/mod_2.rs");
            assert_eq!(fail_adj.annotations[1].path, "src/mod_6.rs");
            assert!(
                fail_adj
                    .compliance_summary
                    .contains("Undocumented flag in mod_2.")
                    && fail_adj
                        .compliance_summary
                        .contains("Undocumented flag in mod_6.")
            );
        }

        #[test]
        fn test_unpruned_oversized_audit_slices_folds_passing_exception_decrees() {
            let canon_ref = ".canons/readme/broad-cli-rule.md";
            let (ws, caseload) = large_multi_module_fixture(canon_ref, 6);
            let passing_provider = DualStageMockProvider {
                vet_violation_marker: String::new(),
                audit_requests: Mutex::new(Vec::new()),
                audit_routes: Vec::new(),
                default_audit_payload: json!({
                    "compliance_summary": "Exempt under internal module Exception.",
                    "compliance_score": 0.88,
                    "annotations": []
                }),
            };
            let pass_out = execute_audit(
                &AuditOptions {
                    max_concurrent_calls: Some(4),
                    ..AuditOptions::default()
                },
                caseload,
                &ws,
                &passing_provider,
            )
            .expect("concurrent multi-slice passing audit");
            let pass_adj = &pass_out.verdict.unwrap().adjudications[0];
            assert_eq!(pass_adj.status, VerdictStatus::Pass);
            assert!((pass_adj.compliance_score - 0.88).abs() < 1e-9);
            assert!(pass_adj.annotations.is_empty());
            assert!(pass_adj.compliance_summary.contains("Compliant across"));
            assert!(pass_adj.compliance_summary.contains("Exception"));
        }

        #[test]
        fn test_bounded_default_thinking_timeout_ceiling_and_contextual_slice_timeout_errors() {
            struct FailingSliceProvider;
            impl StructuredModelProvider for FailingSliceProvider {
                fn generate_structured(
                    &self,
                    request: &GenerationRequest,
                ) -> Result<GenerationResponse, ProviderError> {
                    if request.user_prompt.contains("src/mod_4.rs")
                        || request.user_prompt.contains("src/mod_5.rs")
                    {
                        Err(ProviderError::Timeout {
                            model_specifier: request.model_specifier.clone(),
                            timeout_ms: request.timeout_ms,
                        })
                    } else {
                        Ok(GenerationResponse {
                            payload: json!({
                                "compliance_summary": "Compliant.",
                                "compliance_score": 0.95,
                                "annotations": []
                            }),
                            resolved_model: "gemini-4-carbon".to_string(),
                            latency_ms: 800,
                            usage: TokenUsage::default(),
                            thoughts: Vec::new(),
                        })
                    }
                }
            }

            assert_eq!(DEFAULT_AUDIT_TIMEOUT_MS, 60_000);
            let canon_ref = ".canons/readme/broad-cli-rule.md";
            let (ws, fixture_caseload) = large_multi_module_fixture(canon_ref, 6);
            let caseload = fixture_caseload.with_config(CaseloadConfig::default());

            // Default CaseloadConfig (no thinking_level or reasoning_budget) injects LOW thinking.
            let capture_provider = DualStageMockProvider {
                vet_violation_marker: String::new(),
                audit_requests: Mutex::new(Vec::new()),
                audit_routes: Vec::new(),
                default_audit_payload: json!({
                    "compliance_summary": "Compliant.",
                    "compliance_score": 0.94,
                    "annotations": []
                }),
            };
            execute_audit(
                &AuditOptions::default(),
                caseload.clone(),
                &ws,
                &capture_provider,
            )
            .expect("default audit succeeds");
            let reqs = capture_provider.audit_requests.lock().unwrap();
            assert!(
                reqs.iter()
                    .all(|r| r.thinking_level == Some(ThinkingLevel::Low)
                        && r.reasoning_budget.is_none()
                        && r.timeout_ms == 60_000)
            );
            assert!(
                reqs[0]
                    .call_label
                    .as_deref()
                    .unwrap_or_default()
                    .contains("[slice 1/")
            );
            drop(reqs);

            // Multi-slice timeout error identifies both canon_ref and slice k/N.
            let err = execute_audit(
                &AuditOptions::default(),
                caseload,
                &ws,
                &FailingSliceProvider,
            )
            .expect_err("stalled slice must fail");
            let rendered = err.to_string();
            assert!(rendered.contains(canon_ref), "got: {rendered}");
            assert!(rendered.contains("(slice "), "got: {rendered}");
            assert!(
                rendered.contains("timed out after 60000ms"),
                "got: {rendered}"
            );
            assert!(rendered.contains("Hint:"), "got: {rendered}");
        }

        #[test]
        fn test_audit_renders_strategy_d_breadcrumb_skeleton_for_single_file_contested_spans() {
            use crate::caseload::ContestedSpan;

            let canon_ref = ".canons/core/no-unwrap.md";
            let mut src = String::from("use std::fmt;\n\nmod engine {\n    pub fn cold() {\n");
            for i in 1..=35 {
                src.push_str(&format!("        let cold_{i} = {i};\n"));
            }
            src.push_str(
                "    }\n\n    pub fn hot() {\n        let x = Some(1).unwrap();\n        let _ = x;\n    }\n}\n",
            );
            let hot_line = src
                .lines()
                .enumerate()
                .find(|(_, l)| l.contains("unwrap()"))
                .map(|(i, _)| i + 1)
                .expect("unwrap line");

            let ws = WorkspaceSnapshot::new()
                .with_canon_markdown(
                    "---\ntriggers:\n  - \"src/**\"\n---\nProduction code MUST NOT call unwrap.\n",
                    &ParseCanonOptions::with_source_ref(canon_ref),
                )
                .with_file_content("src/engine.rs", &src);

            let vetting = CaseloadVetting::from_cases(BTreeMap::from([(
                canon_ref.to_string(),
                CanonVettingAssessment {
                    vetting_summary: "Localized unwrap violation.".to_string(),
                    vetting_score: 0.12,
                    exception_claim_score: None,
                    vetting_status: VettingStatus::Contested,
                    contested_exhibits: Vec::new(),
                    contested_spans: vec![ContestedSpan {
                        path: "src/engine.rs".to_string(),
                        start_line: hot_line.saturating_sub(1),
                        end_line: hot_line + 1,
                        score: 0.12,
                    }],
                },
            )]));

            let caseload = Caseload::new()
                .with_intake(CaseloadIntake {
                    target_paths: vec!["src/engine.rs".to_string()],
                    ..CaseloadIntake::default()
                })
                .with_discovery(CaseloadDiscovery {
                    mode: DiscoveryMode::Triggered,
                    candidate_canons: vec![canon_ref.to_string()],
                    ..CaseloadDiscovery::default()
                })
                .with_validation(CaseloadValidation::default())
                .with_config(CaseloadConfig::default())
                .with_docket(CaseloadDocket::from_cases(BTreeMap::from([(
                    canon_ref.to_string(),
                    CanonDocketAssessment {
                        colorability_summary: "ok".to_string(),
                        colorability_score: 0.9,
                        colorability_status: ColorabilityStatus::Docketed,
                    },
                )])))
                .with_evidence(CaseloadEvidence::from_exhibits(BTreeMap::from([(
                    canon_ref.to_string(),
                    vec![ExhibitAssessment {
                        file_path: "src/engine.rs".to_string(),
                        admissibility_summary: "ok".to_string(),
                        admissibility_score: 0.95,
                        admissibility_status: AdmissibilityStatus::Admitted,
                    }],
                )])))
                .with_vetting(vetting);

            let provider = DualStageMockProvider {
                vet_violation_marker: String::new(),
                audit_requests: Mutex::new(Vec::new()),
                audit_routes: Vec::new(),
                default_audit_payload: json!({
                    "compliance_summary": "Calls unwrap() in production code.",
                    "compliance_score": 0.05,
                    "annotations": [{
                        "path": "src/engine.rs",
                        "start_line": hot_line,
                        "end_line": hot_line,
                        "annotation_level": "failure",
                        "message": "Avoid unwrap()."
                    }]
                }),
            };

            let out = execute_audit(&AuditOptions::default(), caseload, &ws, &provider)
                .expect("audit with breadcrumb skeleton");
            let reqs = provider.audit_requests.lock().unwrap();
            assert_eq!(reqs.len(), 1);
            let prompt = &reqs[0].user_prompt;
            assert!(prompt.contains("   1 | use std::fmt;"));
            assert!(prompt.contains("mod engine {"));
            assert!(prompt.contains("pub fn hot() {"));
            assert!(prompt.contains("// ... [L"));
            assert!(!prompt.contains("let cold_20 = 20;"));
            assert!(prompt.contains(&format!(
                "{hot_line:>4} |         let x = Some(1).unwrap();"
            )));

            let verdict = out.verdict.expect("verdict");
            assert_eq!(verdict.failed_count(), 1);
            assert_eq!(verdict.adjudications[0].annotations[0].start_line, hot_line);
        }
    }
}
