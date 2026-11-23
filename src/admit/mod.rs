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
mod exhibits;

use crate::caseload::{
    Caseload, CaseloadEvidence, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL,
    DEFAULT_LOCAL_DISCRIMINATOR_MODEL, DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL, ExhibitAssessment,
    HeuristicNode, normalize_admissibility_score,
};
use crate::configure::is_discriminative_model;
use crate::discover::WorkspaceSnapshot;
use crate::provider::{DEFAULT_GENERATION_TIMEOUT_MS, ProviderError, StructuredModelProvider};
use exhibits::resolve_case_candidate_exhibits;
use std::collections::BTreeMap;
use std::fmt;

/// Default evidentiary admissibility threshold for `admit` (`docs/architecture/nodes/admit.md` §4.5).
pub const DEFAULT_ADMIT_THRESHOLD: f64 = 0.5;

/// Default maximum concurrent in-flight per-case `admit` calls.
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
    /// Optional maximum concurrent in-flight per-case `admit` calls (`>= 1`; defaults to [`DEFAULT_ADMIT_MAX_CONCURRENT_CALLS`]).
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
    /// A generative model was configured on `admit` instead of a discriminative `Noul` model.
    UnsupportedGenerativeModel { specifier: String },
    /// An active canon listed in `.docket.active_docket` is absent from the workspace snapshot.
    MissingActiveCanon { canon_ref: String },
    /// Structured model provider returned a transport, HTTP, block, or schema decoding error.
    Provider(ProviderError),
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
            Self::UnsupportedGenerativeModel { specifier } => write!(
                f,
                "Generative model `{specifier}` cannot be used for `admit` \
                 (`admit` requires a discriminative `Noul` decision model).\n  \
                 Hint: Configure a discriminator on `--admit-model` such as `{DEFAULT_GOOGLE_DISCRIMINATOR_MODEL}` \
                 (Google Decisions), `{DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL}` (TypeSafe Jev), \
                 or `{DEFAULT_LOCAL_DISCRIMINATOR_MODEL}` (local `/v1/systemone`)."
            ),
            Self::MissingActiveCanon { canon_ref } => write!(
                f,
                "Active docket canon `{canon_ref}` referenced in `.docket.active_docket` was not found in the workspace snapshot.\n  \
                 Hint: Re-run `canon-clerk docket` or `canon-clerk admit` against the current workspace."
            ),
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
                "max_concurrent_calls must be at least 1, got 0 (see `{ENV_ADMIT_CONCURRENCY}`)"
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

    let node_cfg = caseload
        .config
        .as_ref()
        .and_then(|c| c.node(HeuristicNode::Admit))
        .ok_or(AdmitError::MissingConfig)?;
    if !is_discriminative_model(&node_cfg.model) {
        return Err(AdmitError::UnsupportedGenerativeModel {
            specifier: node_cfg.model.clone(),
        });
    }

    decision::execute_admit_decisions(
        options,
        caseload,
        threshold,
        empty_cases,
        non_empty_cases,
        workspace,
        provider,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::caseload::{
        ActiveExhibits, AdmissibilityStatus, CanonDocketAssessment, CaseloadConfig,
        CaseloadDiscovery, CaseloadDocket, CaseloadIntake, CaseloadValidation, ColorabilityStatus,
        DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, DEFAULT_GOOGLE_GENERATOR_MODEL, DiscoveryMode,
        FileArtifact, FileChangeStatus, LinkedIssueContext,
    };
    use crate::provider::{
        DecisionRequest, DecisionResponse, GenerationRequest, GenerationResponse, TokenUsage,
    };
    use std::sync::Mutex;

    struct RecordingDecisionProvider {
        requests: Mutex<Vec<DecisionRequest>>,
        scores: BTreeMap<String, f64>,
        error: Option<ProviderError>,
    }

    impl RecordingDecisionProvider {
        fn with_scores(scores: BTreeMap<String, f64>) -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
                scores,
                error: None,
            }
        }

        fn empty() -> Self {
            Self::with_scores(BTreeMap::new())
        }

        fn with_error(err: ProviderError) -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
                scores: BTreeMap::new(),
                error: Some(err),
            }
        }

        fn captured_requests(&self) -> Vec<DecisionRequest> {
            self.requests.lock().expect("lock requests").clone()
        }
    }

    impl StructuredModelProvider for RecordingDecisionProvider {
        fn generate_structured(
            &self,
            _request: &GenerationRequest,
        ) -> Result<GenerationResponse, ProviderError> {
            panic!("admit must never invoke generate_structured")
        }

        fn evaluate_decisions(
            &self,
            request: &DecisionRequest,
        ) -> Result<DecisionResponse, ProviderError> {
            self.requests
                .lock()
                .expect("lock requests")
                .push(request.clone());
            if let Some(err) = &self.error {
                return Err(err.clone());
            }
            let mut out = BTreeMap::new();
            for key in request.questions.keys() {
                let score = self
                    .scores
                    .iter()
                    .find(|(needle, _)| key.contains(needle.as_str()))
                    .map(|(_, s)| *s)
                    .unwrap_or(0.1);
                out.insert(key.clone(), score);
            }
            Ok(DecisionResponse {
                scores: out,
                resolved_model: "gemini-decision-1".to_string(),
                latency_ms: 42,
                usage: TokenUsage {
                    prompt_tokens: 240,
                    completion_tokens: 0,
                    reasoning_tokens: 0,
                    total_tokens: 240,
                },
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
                    author: None,
                    labels: Vec::new(),
                    body: Some("Micro evidentiary triage.".to_string()),
                    url: None,
                }]),
                ..CaseloadIntake::default()
            })
            .with_discovery(CaseloadDiscovery {
                mode: DiscoveryMode::Triggered,
                target_files: vec!["src/cli/admit.rs".to_string()],
                config_files: Vec::new(),
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
                ..CaseloadDiscovery::default()
            })
            .with_validation(CaseloadValidation::default())
            .with_config(
                CaseloadConfig::default()
                    .with_node_model(HeuristicNode::Admit, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL),
            )
            .with_docket(CaseloadDocket::from_cases(cases))
    }

    #[test]
    fn test_empty_active_docket_short_circuits_with_zero_model_calls_and_no_config_needed() {
        let ws = sample_workspace();
        let provider = RecordingDecisionProvider::empty();
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
        let provider = RecordingDecisionProvider::empty();
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
    fn test_enforces_prerequisite_and_generator_rejection_guards() {
        let ws = sample_workspace();
        let provider = RecordingDecisionProvider::empty();

        assert_eq!(
            execute_admit(&AdmitOptions::default(), Caseload::new(), &ws, &provider).unwrap_err(),
            AdmitError::MissingIntake
        );
        assert_eq!(
            execute_admit(
                &AdmitOptions::default(),
                Caseload::new().with_intake(CaseloadIntake::default()),
                &ws,
                &provider,
            )
            .unwrap_err(),
            AdmitError::MissingDiscovery
        );
        let no_docket = Caseload::new()
            .with_intake(CaseloadIntake::default())
            .with_discovery(CaseloadDiscovery::default())
            .with_validation(CaseloadValidation::default());
        assert_eq!(
            execute_admit(&AdmitOptions::default(), no_docket, &ws, &provider).unwrap_err(),
            AdmitError::MissingDocket
        );

        let mut gen_cfg = sample_docketed_caseload(vec![".canons/cli/cli-flags-kebab-case.md"]);
        gen_cfg.config = Some(
            CaseloadConfig::default()
                .with_node_model(HeuristicNode::Admit, DEFAULT_GOOGLE_GENERATOR_MODEL),
        );
        let gen_err = execute_admit(&AdmitOptions::default(), gen_cfg, &ws, &provider)
            .expect_err("generator model on admit must be rejected");
        assert!(matches!(
            gen_err,
            AdmitError::UnsupportedGenerativeModel { .. }
        ));
        let msg = gen_err.to_string();
        assert!(msg.contains("google:gemini-decision-1"));
        assert!(msg.contains("typesafe:jev-latest"));
        assert!(msg.contains("local:kev-4b"));

        let zero_conc = execute_admit(
            &AdmitOptions {
                max_concurrent_calls: Some(0),
                ..AdmitOptions::default()
            },
            Caseload::new(),
            &ws,
            &provider,
        )
        .expect_err("zero concurrency must fail");
        assert!(matches!(zero_conc, AdmitError::InvalidConcurrency { .. }));
    }

    #[test]
    fn test_discriminative_admit_screens_exhibits_and_isolates_admitted_cases() {
        let ws = sample_workspace();
        let caseload = sample_docketed_caseload(vec![
            ".canons/cli/cli-flags-kebab-case.md",
            ".canons/readme/feat-updates-readme.md",
        ]);
        let provider = RecordingDecisionProvider::with_scores(BTreeMap::from([(
            "src/cli/admit.rs".to_string(),
            0.92,
        )]));

        let out = execute_admit(&AdmitOptions::default(), caseload, &ws, &provider)
            .expect("admit succeeds");
        let evidence = out.evidence.expect("evidence populated");
        assert_eq!(evidence.active_cases.len(), 2);
        let cli_exhibits = &evidence.exhibits[".canons/cli/cli-flags-kebab-case.md"];
        assert!(cli_exhibits.iter().any(|e| {
            e.file_path == "src/cli/admit.rs"
                && e.admissibility_status == AdmissibilityStatus::Admitted
        }));
        assert!(cli_exhibits.iter().any(|e| {
            e.file_path == "pr_title" && e.admissibility_status == AdmissibilityStatus::Excluded
        }));
    }

    #[test]
    fn test_propagates_provider_error() {
        let ws = sample_workspace();
        let caseload = sample_docketed_caseload(vec![".canons/cli/cli-flags-kebab-case.md"]);
        let provider = RecordingDecisionProvider::with_error(ProviderError::Timeout {
            model_specifier: DEFAULT_GOOGLE_DISCRIMINATOR_MODEL.to_string(),
            timeout_ms: 500,
        });

        let err = execute_admit(&AdmitOptions::default(), caseload, &ws, &provider)
            .expect_err("timeout must propagate");
        assert!(matches!(
            err,
            AdmitError::Provider(ProviderError::Timeout { .. })
        ));
    }
}
