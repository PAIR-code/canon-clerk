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

//! Core domain execution engine for the `admit` Caseload DAG stage (`docs/architecture/nodes/admit.md`).

mod decision;
mod decode;
mod exhibits;
mod prompt;
mod slices;

use crate::canon::Canon;
use crate::caseload::{
    Caseload, CaseloadDiscovery, CaseloadEvidence, CaseloadIntake, ExhibitAssessment,
    TYPESAFE_PROVIDER, normalize_admissibility_score,
};
use crate::concurrency::execute_bounded_stage_work;
use crate::configure::model_provider;
use crate::discover::WorkspaceSnapshot;
use crate::docket::ENV_MAX_CONCURRENT_CALLS;
use crate::provider::{
    DEFAULT_GENERATION_TIMEOUT_MS, DEFAULT_SCREENER_REASONING_BUDGET, GenerationRequest,
    ProviderError, StructuredModelProvider,
};
use decode::decode_case_exhibits;
use exhibits::resolve_case_candidate_exhibits;
use prompt::{ADMIT_SYSTEM_INSTRUCTION, build_admit_schema};
use slices::ExhibitSlicePlanner;
pub use slices::{MAX_ADMIT_EXHIBITS_PER_SLICE, MAX_ADMIT_SLICE_CHARS};
use std::collections::BTreeMap;
use std::fmt;

/// Default evidentiary admissibility threshold for `admit` (`docs/architecture/nodes/admit.md` §4.5).
pub const DEFAULT_ADMIT_THRESHOLD: f64 = 0.5;

/// Default maximum concurrent in-flight per-case screener calls during `admit`.
pub const DEFAULT_ADMIT_MAX_CONCURRENT_CALLS: usize = 4;

/// Environment variable name for overriding `--admit-threshold` (`docs/architecture/nodes/admit.md` §5).
pub const ENV_ADMIT_THRESHOLD: &str = "CANON_CLERK_ADMIT_THRESHOLD";

/// Environment variable name for overriding `--admit-concurrency` (`docs/architecture/nodes/admit.md` §5).
pub const ENV_ADMIT_CONCURRENCY: &str = "CANON_CLERK_ADMIT_CONCURRENCY";

/// Domain input parameters for executing the `admit` pipeline stage.
#[derive(Debug, Clone, PartialEq)]
pub struct AdmitOptions {
    /// Evidentiary admissibility score threshold in `[0.0, 1.0]` (defaults to [`DEFAULT_ADMIT_THRESHOLD`]).
    pub threshold: f64,
    /// Per-request timeout ceiling in milliseconds (defaults to [`DEFAULT_GENERATION_TIMEOUT_MS`]).
    pub timeout_ms: u64,
    /// Optional maximum concurrent in-flight per-case screener calls (`>= 1`; defaults to [`DEFAULT_ADMIT_MAX_CONCURRENT_CALLS`]).
    pub max_concurrent_calls: Option<usize>,
}

impl Default for AdmitOptions {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_ADMIT_THRESHOLD,
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
            max_concurrent_calls: None,
        }
    }
}

/// Domain errors produced during `admit` execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmitError {
    /// Configured threshold is non-finite or outside `[0.0, 1.0]`.
    InvalidThreshold { detail: String },
    /// Configured `max_concurrent_calls` is zero (`0`).
    InvalidConcurrency { detail: String },
    /// Case-or-Controversy violation: `.intake` is missing from the incoming `Caseload`.
    MissingIntake,
    /// Prerequisite `.discovery` namespace is missing from the incoming `Caseload`.
    MissingDiscovery,
    /// Upstream `.validation` recorded one or more candidate canon lint errors.
    ValidationFailed,
    /// Direct prerequisite `.docket` namespace is missing from the incoming `Caseload`.
    MissingDocket,
    /// Prerequisite `.config` namespace is missing when active cases require model screening.
    MissingConfig,
    /// An active canon listed in `.docket.active_docket` is absent from the workspace snapshot.
    MissingActiveCanon { canon_ref: String },
    /// Per-case or per-slice screening call failed with contextual `canon_ref` and 1-based slice coordinates.
    CaseProvider {
        /// Governing canon identifier whose micro-triage call failed.
        canon_ref: String,
        /// 1-based slice index (`1..=slice_count`).
        slice_index: usize,
        /// Total slices prepared for this case (`>= 1`).
        slice_count: usize,
        /// Underlying provider timeout, transport, HTTP, block, or schema decoding error.
        source: ProviderError,
    },
    /// Structured model provider returned a transport, HTTP, block, or schema decoding error.
    Provider(ProviderError),
}

impl AdmitError {
    pub(super) fn from_case_call(
        canon_ref: &str,
        slice_index: usize,
        slice_count: usize,
        source: ProviderError,
    ) -> Self {
        Self::CaseProvider {
            canon_ref: canon_ref.to_string(),
            slice_index,
            slice_count,
            source,
        }
    }
}

impl fmt::Display for AdmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidThreshold { detail } => write!(
                f,
                "Invalid admit threshold: {detail}.\n  \
                 Hint: Pass a finite threshold in `[0.0, 1.0]` via `--admit-threshold <FLOAT>` \
                 or `{ENV_ADMIT_THRESHOLD}` (default: `{DEFAULT_ADMIT_THRESHOLD}`)."
            ),
            Self::InvalidConcurrency { detail } => write!(
                f,
                "Invalid admit concurrency limit: {detail}.\n  \
                 Hint: Pass a positive integer (`>= 1`) via `--admit-concurrency <COUNT>` \
                 or `{ENV_ADMIT_CONCURRENCY}` (default: `{DEFAULT_ADMIT_MAX_CONCURRENT_CALLS}`)."
            ),
            Self::MissingIntake => write!(
                f,
                "Micro evidentiary triage (`admit`) requires an underlying filing in `.intake` (Case-or-Controversy invariant).\n  \
                 Hint: Pipe a diff (`--diff -`), specify target paths (`src/...`, `-`, `--all-targets`), \
                 or pass `--caseload <PATH|->` with `.intake` populated."
            ),
            Self::MissingDiscovery => write!(
                f,
                "Micro evidentiary triage (`admit`) requires a resolved `.discovery` namespace.\n  \
                 Hint: Run `canon-clerk discover` first or invoke `canon-clerk admit` with a filing source."
            ),
            Self::ValidationFailed => write!(
                f,
                "Cannot execute `admit` because one or more candidate canons failed static validation.\n  \
                 Hint: Run `canon-clerk validate` to inspect and repair malformed candidate canons."
            ),
            Self::MissingDocket => write!(
                f,
                "Micro evidentiary triage (`admit`) requires a resolved `.docket` namespace.\n  \
                 Hint: Run `canon-clerk docket` first or invoke `canon-clerk admit` directly to execute macro triage upstream."
            ),
            Self::MissingConfig => write!(
                f,
                "Cannot execute `admit` screening without a resolved `.config` namespace on the Caseload.\n  \
                 Hint: Run `canon-clerk configure` first or invoke `canon-clerk admit` directly to resolve configuration JIT."
            ),
            Self::MissingActiveCanon { canon_ref } => write!(
                f,
                "Active docket canon `{canon_ref}` referenced in `.docket.active_docket` was not found in the workspace snapshot.\n  \
                 Hint: Re-run `canon-clerk docket` or `canon-clerk admit` against the current workspace."
            ),
            Self::CaseProvider {
                canon_ref,
                slice_index,
                slice_count,
                source,
            } => {
                if *slice_count > 1 {
                    write!(
                        f,
                        "Admit screening failed for canon `{canon_ref}` (slice {slice_index}/{slice_count}): {source}"
                    )
                } else {
                    write!(
                        f,
                        "Admit screening failed for canon `{canon_ref}`: {source}"
                    )
                }
            }
            Self::Provider(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for AdmitError {}

impl From<ProviderError> for AdmitError {
    fn from(err: ProviderError) -> Self {
        Self::Provider(err)
    }
}

struct PreparedCaseCall {
    canon_ref: String,
    slice_index: usize,
    slice_count: usize,
    progress_label: String,
    candidate_exhibits: Vec<String>,
    request: GenerationRequest,
}

/// Executes the pure `admit` domain stage against an in-memory [`WorkspaceSnapshot`] and
/// hexagonal [`StructuredModelProvider`] (`docs/architecture/nodes/admit.md` §4).
pub fn execute_admit(
    options: &AdmitOptions,
    caseload: Caseload,
    workspace: &WorkspaceSnapshot,
    provider: &dyn StructuredModelProvider,
) -> Result<Caseload, AdmitError> {
    let threshold = normalize_admissibility_score(options.threshold)
        .map_err(|detail| AdmitError::InvalidThreshold { detail })?;
    if options.max_concurrent_calls == Some(0) {
        return Err(AdmitError::InvalidConcurrency {
            detail: format!(
                "max_concurrent_calls must be at least 1, got 0 (see `{ENV_ADMIT_CONCURRENCY}` / `{ENV_MAX_CONCURRENT_CALLS}`)"
            ),
        });
    }

    if caseload.evidence.is_some() {
        return Ok(caseload);
    }

    let intake = caseload.intake.as_ref().ok_or(AdmitError::MissingIntake)?;
    let discovery = caseload
        .discovery
        .as_ref()
        .ok_or(AdmitError::MissingDiscovery)?;

    if caseload
        .validation
        .as_ref()
        .is_some_and(|val| val.has_errors)
    {
        return Err(AdmitError::ValidationFailed);
    }

    let docket = caseload.docket.as_ref().ok_or(AdmitError::MissingDocket)?;
    if docket.active_docket.is_empty() {
        return Ok(caseload.with_evidence(CaseloadEvidence::default()));
    }

    let mut empty_cases: BTreeMap<String, Vec<ExhibitAssessment>> = BTreeMap::new();
    let mut non_empty_cases = Vec::new();
    for canon_ref in &docket.active_docket {
        let canon =
            workspace
                .canons
                .get(canon_ref)
                .ok_or_else(|| AdmitError::MissingActiveCanon {
                    canon_ref: canon_ref.clone(),
                })?;
        let candidate_exhibits =
            resolve_case_candidate_exhibits(canon_ref, canon, intake, discovery);
        if candidate_exhibits.is_empty() {
            empty_cases.insert(canon_ref.clone(), Vec::new());
        } else {
            non_empty_cases.push((canon_ref.clone(), canon, candidate_exhibits));
        }
    }

    if non_empty_cases.is_empty() {
        return Ok(caseload.with_evidence(CaseloadEvidence::from_exhibits(empty_cases)));
    }

    let config = caseload.config.as_ref().ok_or(AdmitError::MissingConfig)?;
    if model_provider(&config.screener_model) == TYPESAFE_PROVIDER {
        return decision::execute_admit_decisions(
            options,
            caseload,
            threshold,
            empty_cases,
            non_empty_cases,
            workspace,
            provider,
        );
    }
    let prepared_calls = prepare_generative_case_calls(
        non_empty_cases,
        &config.screener_model,
        options.timeout_ms,
        intake,
        discovery,
        workspace,
    );

    let concurrency_limit = options
        .max_concurrent_calls
        .unwrap_or(DEFAULT_ADMIT_MAX_CONCURRENT_CALLS);
    let mut all_exhibits = execute_case_calls(
        &prepared_calls,
        concurrency_limit,
        threshold,
        &config.screener_model,
        provider,
    )?;
    all_exhibits.extend(empty_cases);

    Ok(caseload.with_evidence(CaseloadEvidence::from_exhibits(all_exhibits)))
}

fn prepare_generative_case_calls(
    non_empty_cases: Vec<(String, &Canon, Vec<String>)>,
    screener_model: &str,
    timeout_ms: u64,
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
    workspace: &WorkspaceSnapshot,
) -> Vec<PreparedCaseCall> {
    let response_schema = build_admit_schema();
    let mut planner = ExhibitSlicePlanner::new(
        intake,
        discovery,
        workspace,
        MAX_ADMIT_SLICE_CHARS,
        MAX_ADMIT_EXHIBITS_PER_SLICE,
    );
    let mut prepared_calls = Vec::with_capacity(non_empty_cases.len());

    for (canon_ref, canon, candidate_exhibits) in non_empty_cases {
        let slices = planner.build_case_slices(&canon_ref, canon, candidate_exhibits);
        let slice_count = slices.len();
        for (idx, slice) in slices.into_iter().enumerate() {
            let slice_index = idx + 1;
            let call_label = if slice_count > 1 {
                format!("{canon_ref} [slice {slice_index}/{slice_count}]")
            } else {
                canon_ref.clone()
            };
            let request = GenerationRequest {
                model_specifier: screener_model.to_string(),
                system_instruction: ADMIT_SYSTEM_INSTRUCTION.to_string(),
                user_prompt: slice.user_prompt,
                response_schema: response_schema.clone(),
                thinking_level: None,
                reasoning_budget: DEFAULT_SCREENER_REASONING_BUDGET,
                timeout_ms,
                call_label: Some(call_label.clone()),
            };
            prepared_calls.push(PreparedCaseCall {
                canon_ref: canon_ref.clone(),
                slice_index,
                slice_count,
                progress_label: call_label,
                candidate_exhibits: slice.candidate_exhibits,
                request,
            });
        }
    }

    prepared_calls
}

type CaseResult = Result<(String, Vec<ExhibitAssessment>), AdmitError>;

fn evaluate_single_case_call(
    call: &PreparedCaseCall,
    threshold: f64,
    model_specifier: &str,
    provider: &dyn StructuredModelProvider,
) -> CaseResult {
    let response = provider.generate_structured(&call.request).map_err(|err| {
        AdmitError::from_case_call(&call.canon_ref, call.slice_index, call.slice_count, err)
    })?;
    let decoded = decode_case_exhibits(
        &response.payload,
        &call.canon_ref,
        &call.candidate_exhibits,
        threshold,
        model_specifier,
    )
    .map_err(|err| {
        AdmitError::from_case_call(&call.canon_ref, call.slice_index, call.slice_count, err)
    })?;
    Ok((call.canon_ref.clone(), decoded))
}

fn execute_case_calls(
    prepared_calls: &[PreparedCaseCall],
    concurrency_limit: usize,
    threshold: f64,
    model_specifier: &str,
    provider: &dyn StructuredModelProvider,
) -> Result<BTreeMap<String, Vec<ExhibitAssessment>>, AdmitError> {
    let slice_outcomes =
        execute_bounded_stage_work(prepared_calls, concurrency_limit, provider, |_idx, call| {
            let outcome = evaluate_single_case_call(call, threshold, model_specifier, provider)?;
            Ok::<_, AdmitError>((outcome, call.progress_label.clone()))
        })?;

    let mut combined: BTreeMap<String, Vec<ExhibitAssessment>> = BTreeMap::new();
    for (canon_ref, items) in slice_outcomes {
        combined.entry(canon_ref).or_default().extend(items);
    }
    for items in combined.values_mut() {
        items.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    }
    Ok(combined)
}

#[cfg(test)]
mod tests {
    use super::prompt::build_admit_case_user_prompt;
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::caseload::{
        ActiveExhibits, CanonDocketAssessment, CaseloadConfig, CaseloadDiscovery, CaseloadDocket,
        CaseloadIntake, CaseloadValidation, ColorabilityStatus, DiscoveryMode, FileArtifact,
        FileChangeStatus, LinkedIssueContext,
    };
    use crate::provider::{GenerationResponse, TokenUsage};
    use serde_json::{Value, json};
    use std::sync::Mutex;

    struct RoutingMockProvider {
        requests: Mutex<Vec<GenerationRequest>>,
        routes: BTreeMap<String, Result<Value, ProviderError>>,
    }

    impl RoutingMockProvider {
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

    impl StructuredModelProvider for RoutingMockProvider {
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
                            thoughts: Vec::new(),
                            usage: TokenUsage {
                                prompt_tokens: 240,
                                completion_tokens: 60,
                                reasoning_tokens: 0,
                                total_tokens: 300,
                            },
                            latency_ms: 42,
                            resolved_model: "gemini-2.5-flash-lite".to_string(),
                        }),
                        Err(err) => Err(err.clone()),
                    };
                }
            }
            Err(ProviderError::SchemaDecode {
                model_specifier: request.model_specifier.clone(),
                message: "no mock route matched prompt".to_string(),
            })
        }
    }

    fn sample_workspace() -> WorkspaceSnapshot {
        WorkspaceSnapshot::new()
            .with_canon_markdown(
                "---\ntriggers:\n  - \"src/cli/**\"\ninspect:\n  - diff\n  - pr_title?\n  - pr_body?\n---\n# CLI Flags Kebab Case\n\nCLI flags MUST use kebab-case naming.\n",
                &ParseCanonOptions::with_source_ref(".canons/cli/cli-flags-kebab-case.md"),
            )
            .with_canon_markdown(
                "---\ntriggers:\n  - \"**/*\"\nreferences:\n  - \"README.md\"\ninspect:\n  - diff\n  - pr_title\n  - linked_issues?\n---\n# Feat PRs Must Update README\n\nFeature PRs (`feat`) MUST update `README.md`.\n",
                &ParseCanonOptions::with_source_ref(".canons/readme/feat-updates-readme.md"),
            )
            .with_file_content("src/cli/admit.rs", "pub struct AdmitArgs {}\n")
    }

    fn sample_docketed_caseload(active_canons: Vec<&str>) -> Caseload {
        let mut diffs = BTreeMap::new();
        diffs.insert(
            "src/cli/admit.rs".to_string(),
            FileArtifact {
                path: "src/cli/admit.rs".to_string(),
                status: FileChangeStatus::Added,
                previous_path: None,
                lines_added: 24,
                lines_deleted: 0,
                patch: Some("@@ -0,0 +1,24 @@\n+#[arg(long = \"admit-threshold\")]\n".to_string()),
                patch_omission_reason: None,
                content: None,
                content_omission_reason: None,
            },
        );

        let mut triggers_join = BTreeMap::new();
        triggers_join.insert(
            ".canons/cli/cli-flags-kebab-case.md".to_string(),
            vec![
                "src/cli/admit.rs".to_string(),
                "pr_title".to_string(),
                "pr_body".to_string(),
            ],
        );
        triggers_join.insert(
            ".canons/readme/feat-updates-readme.md".to_string(),
            vec![
                "src/cli/admit.rs".to_string(),
                "pr_title".to_string(),
                "linked_issues".to_string(),
            ],
        );

        let mut cases = BTreeMap::new();
        for &c in &[
            ".canons/cli/cli-flags-kebab-case.md",
            ".canons/readme/feat-updates-readme.md",
        ] {
            let is_active = active_canons.contains(&c);
            cases.insert(
                c.to_string(),
                CanonDocketAssessment {
                    colorability_summary: "Screened in docket.".to_string(),
                    colorability_score: if is_active { 0.9 } else { 0.1 },
                    colorability_status: if is_active {
                        ColorabilityStatus::Docketed
                    } else {
                        ColorabilityStatus::Dismissed
                    },
                },
            );
        }

        Caseload::new()
            .with_intake(CaseloadIntake {
                pr_title: Some("feat(cli): implement admit subcommand".to_string()),
                pr_body: Some("Closes #291".to_string()),
                diffs,
                linked_issues: Some(vec![LinkedIssueContext {
                    number: 291,
                    title: Some("Implement admit stage".to_string()),
                    body: Some("Micro evidentiary triage.".to_string()),
                    url: None,
                }]),
                ..CaseloadIntake::default()
            })
            .with_discovery(CaseloadDiscovery {
                mode: DiscoveryMode::Triggered,
                target_files: vec!["src/cli/admit.rs".to_string()],
                candidate_canons: vec![
                    ".canons/cli/cli-flags-kebab-case.md".to_string(),
                    ".canons/readme/feat-updates-readme.md".to_string(),
                ],
                active_exhibits: ActiveExhibits {
                    files: vec!["src/cli/admit.rs".to_string()],
                    pr_title: Some(true),
                    pr_body: Some(true),
                    commit_messages: None,
                    linked_issues: Some(true),
                },
                triggers_join,
            })
            .with_validation(CaseloadValidation::default())
            .with_config(CaseloadConfig::default())
            .with_docket(CaseloadDocket::from_cases(cases))
    }

    mod guards_and_short_circuits {
        use super::*;

        #[test]
        fn test_empty_active_docket_short_circuits_with_zero_model_calls_and_no_config_needed() {
            let ws = sample_workspace();
            let provider = RoutingMockProvider::empty();
            let mut caseload = sample_docketed_caseload(vec![]);
            caseload.config = None;

            let enriched = execute_admit(&AdmitOptions::default(), caseload, &ws, &provider)
                .expect("empty docket must short-circuit cleanly");

            assert!(provider.captured_requests().is_empty());
            let evidence = enriched.evidence.expect("evidence populated");
            assert!(evidence.exhibits.is_empty());
            assert!(evidence.active_cases.is_empty());
        }

        #[test]
        fn test_fast_forwards_when_evidence_already_populated() {
            let ws = sample_workspace();
            let provider = RoutingMockProvider::empty();
            let pre_populated = Caseload::new()
                .with_intake(CaseloadIntake::default())
                .with_evidence(CaseloadEvidence::default());

            let out = execute_admit(
                &AdmitOptions::default(),
                pre_populated.clone(),
                &ws,
                &provider,
            )
            .expect("fast-forward completed evidence");
            assert_eq!(out, pre_populated);
            assert!(provider.captured_requests().is_empty());
        }

        #[test]
        fn test_enforces_prerequisite_threshold_and_concurrency_guards() {
            let ws = sample_workspace();
            let provider = RoutingMockProvider::empty();

            assert!(matches!(
                execute_admit(
                    &AdmitOptions {
                        threshold: 1.2,
                        ..AdmitOptions::default()
                    },
                    Caseload::new(),
                    &ws,
                    &provider
                )
                .unwrap_err(),
                AdmitError::InvalidThreshold { .. }
            ));
            assert!(matches!(
                execute_admit(
                    &AdmitOptions {
                        max_concurrent_calls: Some(0),
                        ..AdmitOptions::default()
                    },
                    Caseload::new(),
                    &ws,
                    &provider
                )
                .unwrap_err(),
                AdmitError::InvalidConcurrency { .. }
            ));

            assert_eq!(
                execute_admit(&AdmitOptions::default(), Caseload::new(), &ws, &provider)
                    .unwrap_err(),
                AdmitError::MissingIntake
            );

            let with_intake = Caseload::new().with_intake(CaseloadIntake::default());
            assert_eq!(
                execute_admit(&AdmitOptions::default(), with_intake, &ws, &provider).unwrap_err(),
                AdmitError::MissingDiscovery
            );

            let mut failed_val =
                sample_docketed_caseload(vec![".canons/cli/cli-flags-kebab-case.md"]);
            failed_val.validation = Some(CaseloadValidation {
                has_errors: true,
                results: BTreeMap::new(),
            });
            assert_eq!(
                execute_admit(&AdmitOptions::default(), failed_val, &ws, &provider).unwrap_err(),
                AdmitError::ValidationFailed
            );

            let mut no_docket =
                sample_docketed_caseload(vec![".canons/cli/cli-flags-kebab-case.md"]);
            no_docket.docket = None;
            assert_eq!(
                execute_admit(&AdmitOptions::default(), no_docket, &ws, &provider).unwrap_err(),
                AdmitError::MissingDocket
            );

            let mut no_cfg = sample_docketed_caseload(vec![".canons/cli/cli-flags-kebab-case.md"]);
            no_cfg.config = None;
            assert_eq!(
                execute_admit(&AdmitOptions::default(), no_cfg, &ws, &provider).unwrap_err(),
                AdmitError::MissingConfig
            );
        }
    }

    mod unified_file_and_metadata_screening {
        use super::*;

        #[test]
        fn test_screens_files_and_metadata_planes_concurrently_and_isolates_admitted_exhibits() {
            let ws = sample_workspace();
            let mut routes = BTreeMap::new();
            routes.insert(
                ".canons/cli/cli-flags-kebab-case.md".to_string(),
                Ok(json!({
                    "exhibits": [
                        {
                            "file_path": "pr_body",
                            "admissibility_summary": "PR body only links #291 and has no flag syntax.",
                            "admissibility_score": 0.10
                        },
                        {
                            "file_path": "pr_title",
                            "admissibility_summary": "Conventional Commit title is orthogonal to flag naming.",
                            "admissibility_score": 0.12
                        },
                        {
                            "file_path": "src/cli/admit.rs",
                            "admissibility_summary": "Introduces `--admit-threshold` CLI flag attribute.",
                            "admissibility_score": 0.96
                        }
                    ]
                })),
            );
            routes.insert(
                ".canons/readme/feat-updates-readme.md".to_string(),
                Ok(json!({
                    "exhibits": [
                        {
                            "file_path": "linked_issues",
                            "admissibility_summary": "Confirms #291 is a user-facing feature (`type: feat`).",
                            "admissibility_score": 0.88
                        },
                        {
                            "file_path": "pr_title",
                            "admissibility_summary": "`feat(cli):` prefix triggers README synchronization requirement despite README.md omission.",
                            "admissibility_score": 0.94
                        },
                        {
                            "file_path": "src/cli/admit.rs",
                            "admissibility_summary": "Adds new user-facing CLI subcommand without accompanying README update.",
                            "admissibility_score": 0.91
                        }
                    ]
                })),
            );

            let provider = RoutingMockProvider::new(routes);
            let caseload = sample_docketed_caseload(vec![
                ".canons/cli/cli-flags-kebab-case.md",
                ".canons/readme/feat-updates-readme.md",
            ]);

            let enriched = execute_admit(
                &AdmitOptions {
                    max_concurrent_calls: Some(2),
                    ..AdmitOptions::default()
                },
                caseload,
                &ws,
                &provider,
            )
            .expect("concurrent admit screening");

            assert_eq!(provider.captured_requests().len(), 2);
            let evidence = enriched.evidence.expect("evidence populated");
            assert_eq!(
                evidence.active_cases,
                vec![
                    ".canons/cli/cli-flags-kebab-case.md".to_string(),
                    ".canons/readme/feat-updates-readme.md".to_string()
                ]
            );
            assert_eq!(evidence.admitted_exhibit_count(), 4);
            assert_eq!(evidence.excluded_exhibit_count(), 2);

            // Excluded metadata planes are retained in `.evidence` for provenance,
            // yet strictly excluded from `admitted_exhibits_for` (zero leakage into `audit`).
            let cli_admitted =
                evidence.admitted_exhibits_for(".canons/cli/cli-flags-kebab-case.md");
            assert_eq!(cli_admitted.len(), 1);
            assert_eq!(cli_admitted[0].file_path, "src/cli/admit.rs");

            let readme_admitted =
                evidence.admitted_exhibits_for(".canons/readme/feat-updates-readme.md");
            assert_eq!(readme_admitted.len(), 3);
        }

        #[test]
        fn test_zero_admitted_exhibits_leaves_active_cases_empty_and_rejects_malformed_schema() {
            let ws = sample_workspace();
            let mut routes = BTreeMap::new();
            routes.insert(
                ".canons/cli/cli-flags-kebab-case.md".to_string(),
                Ok(json!({
                    "exhibits": [
                        {
                            "file_path": "pr_body",
                            "admissibility_summary": "Irrelevant.",
                            "admissibility_score": 0.05
                        },
                        {
                            "file_path": "pr_title",
                            "admissibility_summary": "Irrelevant.",
                            "admissibility_score": 0.08
                        },
                        {
                            "file_path": "src/cli/admit.rs",
                            "admissibility_summary": "Contains no flags.",
                            "admissibility_score": 0.15
                        }
                    ]
                })),
            );
            let provider = RoutingMockProvider::new(routes);
            let caseload = sample_docketed_caseload(vec![".canons/cli/cli-flags-kebab-case.md"]);

            let enriched =
                execute_admit(&AdmitOptions::default(), caseload.clone(), &ws, &provider)
                    .expect("zero-admitted-exhibits short-circuit");
            let evidence = enriched.evidence.expect("evidence attached");
            assert!(evidence.active_cases.is_empty());
            assert_eq!(evidence.excluded_exhibit_count(), 3);

            // Missing candidate exhibit in model response fails with SchemaDecode
            let mut incomplete_routes = BTreeMap::new();
            incomplete_routes.insert(
                ".canons/cli/cli-flags-kebab-case.md".to_string(),
                Ok(json!({
                    "exhibits": [
                        {
                            "file_path": "src/cli/admit.rs",
                            "admissibility_summary": "Only returned one item.",
                            "admissibility_score": 0.9
                        }
                    ]
                })),
            );
            let bad_provider = RoutingMockProvider::new(incomplete_routes);
            let err = execute_admit(&AdmitOptions::default(), caseload, &ws, &bad_provider)
                .expect_err("incomplete exhibit response must fail");
            assert!(matches!(
                err,
                AdmitError::CaseProvider {
                    ref canon_ref,
                    slice_index: 1,
                    slice_count: 1,
                    source: ProviderError::SchemaDecode { .. },
                } if canon_ref == ".canons/cli/cli-flags-kebab-case.md"
            ));
            let rendered_err = err.to_string();
            assert!(rendered_err.contains(".canons/cli/cli-flags-kebab-case.md"));
            assert!(!rendered_err.contains("(slice "));
        }

        #[test]
        fn test_all_canons_path_only_sweep_renders_workspace_file_snapshot() {
            let ws = sample_workspace();
            let mut routes = BTreeMap::new();
            routes.insert(
                "pub struct AdmitArgs".to_string(),
                Ok(json!({
                    "exhibits": [
                        {
                            "file_path": "src/cli/admit.rs",
                            "admissibility_summary": "Materialized target file defines CLI struct.",
                            "admissibility_score": 0.85
                        }
                    ]
                })),
            );
            let provider = RoutingMockProvider::new(routes);

            let mut cases = BTreeMap::new();
            cases.insert(
                ".canons/cli/cli-flags-kebab-case.md".to_string(),
                CanonDocketAssessment {
                    colorability_summary: "CLI target file.".to_string(),
                    colorability_score: 0.9,
                    colorability_status: ColorabilityStatus::Docketed,
                },
            );

            let caseload = Caseload::new()
                .with_intake(CaseloadIntake {
                    target_paths: vec!["src/cli/admit.rs".to_string()],
                    ..CaseloadIntake::default()
                })
                .with_discovery(CaseloadDiscovery {
                    mode: DiscoveryMode::AllCanons,
                    target_files: vec!["src/cli/admit.rs".to_string()],
                    candidate_canons: vec![".canons/cli/cli-flags-kebab-case.md".to_string()],
                    ..CaseloadDiscovery::default()
                })
                .with_validation(CaseloadValidation::default())
                .with_config(CaseloadConfig::default())
                .with_docket(CaseloadDocket::from_cases(cases));

            let enriched = execute_admit(&AdmitOptions::default(), caseload, &ws, &provider)
                .expect("all-canons path-only admit");
            assert_eq!(
                enriched.evidence.expect("evidence").active_cases,
                vec![".canons/cli/cli-flags-kebab-case.md".to_string()]
            );

            let reqs = provider.captured_requests();
            assert_eq!(reqs.len(), 1);
            assert_eq!(reqs[0].reasoning_budget, DEFAULT_SCREENER_REASONING_BUDGET);
            let overview_pos = reqs[0]
                .user_prompt
                .find("## Filing Overview")
                .expect("overview section present");
            let exhibits_pos = reqs[0]
                .user_prompt
                .find("## Candidate Exhibits")
                .expect("exhibits section present");
            let canon_pos = reqs[0]
                .user_prompt
                .find("## Active Case Governing Canon:")
                .expect("canon section present");
            let required_pos = reqs[0]
                .user_prompt
                .find("## Required `file_path` Identifiers")
                .expect("required identifiers section present");
            assert!(
                overview_pos < exhibits_pos && exhibits_pos < canon_pos && canon_pos < required_pos,
                "shared exhibits must lead for prefix caching while governing canon sits at peak recency before generation"
            );
            assert!(reqs[0].user_prompt.contains("- Governing Invariant:"));
            assert!(
                reqs[0]
                    .system_instruction
                    .contains("Code Stratum vs. Documentation Stratum")
            );
            assert!(
                !reqs[0].user_prompt.contains("Modified Files in Filing ("),
                "filing overview must not prime comma-joined filename lists"
            );
        }

        #[test]
        fn test_shared_candidate_exhibits_produce_byte_identical_prompt_prefixes() {
            let ws = sample_workspace();
            let cli_canon = ws
                .canons
                .get(".canons/cli/cli-flags-kebab-case.md")
                .expect("cli canon");
            let readme_canon = ws
                .canons
                .get(".canons/readme/feat-updates-readme.md")
                .expect("readme canon");
            let shared_exhibits = vec!["src/cli/admit.rs".to_string(), "pr_title".to_string()];
            let p1 = build_admit_case_user_prompt(
                ".canons/cli/cli-flags-kebab-case.md",
                cli_canon,
                &shared_exhibits,
                &CaseloadIntake::default(),
                &CaseloadDiscovery::default(),
                &ws,
            );
            let p2 = build_admit_case_user_prompt(
                ".canons/readme/feat-updates-readme.md",
                readme_canon,
                &shared_exhibits,
                &CaseloadIntake::default(),
                &CaseloadDiscovery::default(),
                &ws,
            );
            let p1_split = p1.find("## Active Case Governing Canon:").expect("split 1");
            let p2_split = p2.find("## Active Case Governing Canon:").expect("split 2");
            assert_eq!(&p1[..p1_split], &p2[..p2_split]);
        }

        #[test]
        fn test_recovers_exact_comma_grouped_file_paths_while_rejecting_unknown_segments() {
            let ws = sample_workspace();
            let mut ok_routes = BTreeMap::new();
            ok_routes.insert(
                ".canons/cli/cli-flags-kebab-case.md".to_string(),
                Ok(json!({
                    "exhibits": [
                        {
                            "file_path": "pr_body, pr_title",
                            "admissibility_summary": "Metadata planes do not define CLI option flags.",
                            "admissibility_score": 0.10
                        },
                        {
                            "file_path": "src/cli/admit.rs",
                            "admissibility_summary": "Defines `--admit-threshold` CLI flag.",
                            "admissibility_score": 0.94
                        }
                    ]
                })),
            );
            let caseload = sample_docketed_caseload(vec![".canons/cli/cli-flags-kebab-case.md"]);
            let enriched = execute_admit(
                &AdmitOptions::default(),
                caseload.clone(),
                &ws,
                &RoutingMockProvider::new(ok_routes),
            )
            .expect("exact comma-grouped exhibits must expand cleanly");
            let ev = enriched.evidence.expect("evidence");
            assert_eq!(ev.admitted_exhibit_count(), 1);
            assert_eq!(ev.excluded_exhibit_count(), 2);

            let mut bad_routes = BTreeMap::new();
            bad_routes.insert(
                ".canons/cli/cli-flags-kebab-case.md".to_string(),
                Ok(json!({
                    "exhibits": [
                        {
                            "file_path": "pr_body, src/unknown.rs",
                            "admissibility_summary": "Contains unknown path.",
                            "admissibility_score": 0.10
                        },
                        {
                            "file_path": "pr_title",
                            "admissibility_summary": "Irrelevant.",
                            "admissibility_score": 0.10
                        },
                        {
                            "file_path": "src/cli/admit.rs",
                            "admissibility_summary": "Defines CLI flag.",
                            "admissibility_score": 0.94
                        }
                    ]
                })),
            );
            let err = execute_admit(
                &AdmitOptions::default(),
                caseload,
                &ws,
                &RoutingMockProvider::new(bad_routes),
            )
            .expect_err("unknown segment inside comma-grouped file_path must fail");
            assert!(matches!(
                err,
                AdmitError::CaseProvider {
                    source: ProviderError::SchemaDecode { .. },
                    ..
                }
            ));
        }
    }

    mod exhibit_slicing_and_map_reduce_folding {
        use super::*;
        use crate::caseload::AdmissibilityStatus;
        use std::time::Duration;

        fn multi_file_docketed_fixture(file_count: usize) -> (WorkspaceSnapshot, Caseload) {
            let cli_ref = ".canons/cli/cli-flags-kebab-case.md";
            let readme_ref = ".canons/readme/feat-updates-readme.md";
            let ws = sample_workspace();

            let mut diffs = BTreeMap::new();
            let mut target_files = Vec::new();
            let mut joined = vec![
                "linked_issues".to_string(),
                "pr_body".to_string(),
                "pr_title".to_string(),
            ];
            for idx in 0..file_count {
                let path = format!("src/cli/subcmd_{idx:02}.rs");
                diffs.insert(
                    path.clone(),
                    FileArtifact {
                        path: path.clone(),
                        status: FileChangeStatus::Added,
                        previous_path: None,
                        lines_added: 18,
                        lines_deleted: 0,
                        patch: Some(format!(
                            "@@ -0,0 +1,18 @@\n+#[arg(long = \"flag-{idx:02}\")]\n+pub struct Args{idx:02} {{}}\n"
                        )),
                        patch_omission_reason: None,
                        content: None,
                        content_omission_reason: None,
                    },
                );
                target_files.push(path.clone());
                joined.push(path);
            }
            joined.sort();

            let mut triggers_join = BTreeMap::new();
            triggers_join.insert(cli_ref.to_string(), joined.clone());
            triggers_join.insert(readme_ref.to_string(), joined);

            let mut cases = BTreeMap::new();
            for canon_ref in [cli_ref, readme_ref] {
                cases.insert(
                    canon_ref.to_string(),
                    CanonDocketAssessment {
                        colorability_summary: "Active in docket.".to_string(),
                        colorability_score: 0.92,
                        colorability_status: ColorabilityStatus::Docketed,
                    },
                );
            }

            let caseload = Caseload::new()
                .with_intake(CaseloadIntake {
                    pr_title: Some("feat(cli): add 20 subcommand modules".to_string()),
                    pr_body: Some("Closes #313".to_string()),
                    diffs,
                    linked_issues: Some(vec![LinkedIssueContext {
                        number: 313,
                        title: Some("Slice admit exhibits".to_string()),
                        body: Some("Prevent 60s screener timeouts.".to_string()),
                        url: None,
                    }]),
                    ..CaseloadIntake::default()
                })
                .with_discovery(CaseloadDiscovery {
                    mode: DiscoveryMode::Triggered,
                    target_files: target_files.clone(),
                    candidate_canons: vec![cli_ref.to_string(), readme_ref.to_string()],
                    active_exhibits: ActiveExhibits {
                        files: target_files,
                        pr_title: Some(true),
                        pr_body: Some(true),
                        commit_messages: None,
                        linked_issues: Some(true),
                    },
                    triggers_join,
                })
                .with_validation(CaseloadValidation::default())
                .with_config(CaseloadConfig::default())
                .with_docket(CaseloadDocket::from_cases(cases));

            (ws, caseload)
        }

        fn parse_required_identifiers(prompt: &str) -> Vec<String> {
            let marker = "## Required `file_path` Identifiers (";
            let start = prompt.find(marker).expect("required identifiers marker");
            prompt[start..]
                .lines()
                .skip(1)
                .filter_map(|line| {
                    let tick_start = line.find('`')? + 1;
                    let tick_end = line[tick_start..].find('`')? + tick_start;
                    Some(line[tick_start..tick_end].to_string())
                })
                .collect()
        }

        struct OutOfOrderSliceProvider {
            requests: Mutex<Vec<GenerationRequest>>,
            progress: Mutex<Vec<String>>,
            fail_slice_needle: Option<String>,
        }

        impl OutOfOrderSliceProvider {
            fn new(fail_slice_needle: Option<&str>) -> Self {
                Self {
                    requests: Mutex::new(Vec::new()),
                    progress: Mutex::new(Vec::new()),
                    fail_slice_needle: fail_slice_needle.map(ToString::to_string),
                }
            }
        }

        impl StructuredModelProvider for OutOfOrderSliceProvider {
            fn generate_structured(
                &self,
                request: &GenerationRequest,
            ) -> Result<GenerationResponse, ProviderError> {
                self.requests
                    .lock()
                    .expect("lock requests")
                    .push(request.clone());

                if let Some(ref needle) = self.fail_slice_needle
                    && request
                        .call_label
                        .as_deref()
                        .unwrap_or_default()
                        .contains(needle)
                {
                    return Err(ProviderError::Timeout {
                        model_specifier: request.model_specifier.clone(),
                        timeout_ms: request.timeout_ms,
                    });
                }

                // Delay slice 1 so slice 2 finishes first under concurrent execution.
                if request
                    .call_label
                    .as_deref()
                    .unwrap_or_default()
                    .contains("[slice 1/")
                {
                    std::thread::sleep(Duration::from_millis(15));
                }

                let required_ids = parse_required_identifiers(&request.user_prompt);
                let items: Vec<Value> = required_ids
                    .into_iter()
                    .map(|id| {
                        let score = if id.ends_with("_00.rs") || id.ends_with("_18.rs") {
                            0.93
                        } else {
                            0.11
                        };
                        json!({
                            "file_path": id,
                            "admissibility_summary": format!("Evaluated `{id}` in isolated slice."),
                            "admissibility_score": score
                        })
                    })
                    .collect();

                Ok(GenerationResponse {
                    payload: json!({ "exhibits": items }),
                    thoughts: Vec::new(),
                    usage: TokenUsage {
                        prompt_tokens: 400,
                        completion_tokens: 120,
                        reasoning_tokens: 20,
                        total_tokens: 540,
                    },
                    latency_ms: 35,
                    resolved_model: "gemini-4-hydrogen".to_string(),
                })
            }

            fn record_progress(&self, completed: usize, total: usize, label: &str) {
                self.progress
                    .lock()
                    .expect("lock progress")
                    .push(format!("{completed}/{total}:{label}"));
            }
        }

        fn assert_folded_case_evidence(evidence: &CaseloadEvidence, canon_ref: &str) {
            let items = &evidence.exhibits[canon_ref];
            assert_eq!(items.len(), 23);
            let sorted_paths: Vec<&str> = items.iter().map(|e| e.file_path.as_str()).collect();
            let mut check = sorted_paths.clone();
            check.sort_unstable();
            assert_eq!(sorted_paths, check);

            let admitted: Vec<&str> = evidence
                .admitted_exhibits_for(canon_ref)
                .into_iter()
                .map(|e| e.file_path.as_str())
                .collect();
            assert_eq!(
                admitted,
                vec!["src/cli/subcmd_00.rs", "src/cli/subcmd_18.rs"]
            );
            assert!(items.iter().any(|e| {
                e.file_path == "pr_title" && e.admissibility_status == AdmissibilityStatus::Excluded
            }));
        }

        #[test]
        fn test_concurrent_out_of_order_slices_fold_into_byte_identical_evidence_and_record_slice_labels()
         {
            let (ws, caseload) = multi_file_docketed_fixture(20);

            let sequential_provider = OutOfOrderSliceProvider::new(None);
            let seq_out = execute_admit(
                &AdmitOptions {
                    max_concurrent_calls: Some(1),
                    ..AdmitOptions::default()
                },
                caseload.clone(),
                &ws,
                &sequential_provider,
            )
            .expect("sequential sliced admit");

            let concurrent_provider = OutOfOrderSliceProvider::new(None);
            let conc_out = execute_admit(
                &AdmitOptions {
                    max_concurrent_calls: Some(4),
                    ..AdmitOptions::default()
                },
                caseload,
                &ws,
                &concurrent_provider,
            )
            .expect("concurrent out-of-order sliced admit");

            let seq_json =
                serde_json::to_string(&seq_out.evidence).expect("serialize sequential evidence");
            let conc_json =
                serde_json::to_string(&conc_out.evidence).expect("serialize concurrent evidence");
            assert_eq!(
                seq_json, conc_json,
                "out-of-order slice completions must fold into byte-for-byte identical .evidence"
            );

            let evidence = conc_out.evidence.expect("evidence populated");
            assert_eq!(evidence.exhibits.len(), 2);
            assert_folded_case_evidence(&evidence, ".canons/cli/cli-flags-kebab-case.md");
            assert_folded_case_evidence(&evidence, ".canons/readme/feat-updates-readme.md");

            let captured = sequential_provider.requests.lock().expect("requests");
            assert_eq!(captured.len(), 4);
            let labels: Vec<&str> = captured
                .iter()
                .filter_map(|r| r.call_label.as_deref())
                .collect();
            assert_eq!(
                labels,
                vec![
                    ".canons/cli/cli-flags-kebab-case.md [slice 1/2]",
                    ".canons/cli/cli-flags-kebab-case.md [slice 2/2]",
                    ".canons/readme/feat-updates-readme.md [slice 1/2]",
                    ".canons/readme/feat-updates-readme.md [slice 2/2]",
                ]
            );
        }

        #[test]
        fn test_multi_slice_timeout_error_annotates_canon_ref_and_slice_coordinates() {
            let (ws, caseload) = multi_file_docketed_fixture(20);
            let failing_provider = OutOfOrderSliceProvider::new(Some(
                ".canons/cli/cli-flags-kebab-case.md [slice 2/2]",
            ));

            let err = execute_admit(
                &AdmitOptions {
                    max_concurrent_calls: Some(2),
                    timeout_ms: 60_000,
                    ..AdmitOptions::default()
                },
                caseload,
                &ws,
                &failing_provider,
            )
            .expect_err("slice 2 timeout must fail fast");

            assert!(matches!(
                err,
                AdmitError::CaseProvider {
                    ref canon_ref,
                    slice_index: 2,
                    slice_count: 2,
                    source: ProviderError::Timeout { timeout_ms: 60_000, .. },
                } if canon_ref == ".canons/cli/cli-flags-kebab-case.md"
            ));
            let rendered = err.to_string();
            assert!(
                rendered.contains(".canons/cli/cli-flags-kebab-case.md"),
                "got: {rendered}"
            );
            assert!(rendered.contains("(slice 2/2)"), "got: {rendered}");
            assert!(
                rendered.contains("timed out after 60000ms"),
                "got: {rendered}"
            );
            assert!(rendered.contains("Hint:"), "got: {rendered}");
        }
    }
}
