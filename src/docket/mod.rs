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

//! Core domain execution engine for the `docket` Caseload DAG stage (`docs/architecture/nodes/docket.md`).

mod decision;
mod prompt;

use crate::caseload::{
    Caseload, CaseloadDocket, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL,
    DEFAULT_LOCAL_DISCRIMINATOR_MODEL, DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL, HeuristicNode,
    normalize_colorability_score,
};
use crate::configure::is_discriminative_model;
use crate::discover::WorkspaceSnapshot;
use crate::provider::{DEFAULT_GENERATION_TIMEOUT_MS, ProviderError, StructuredModelProvider};
use std::fmt;

/// Default subject-matter jurisdiction threshold for `docket` (`docs/architecture/nodes/docket.md` §4.3).
pub const DEFAULT_DOCKET_THRESHOLD: f64 = 0.5;

/// Default maximum concurrent in-flight `docket` model calls when executing multiple batches.
pub const DEFAULT_DOCKET_MAX_CONCURRENT_CALLS: usize = 4;

/// Environment variable name for overriding `--docket-threshold` (`docs/architecture/nodes/docket.md` §5).
pub const ENV_DOCKET_THRESHOLD: &str = "CANON_CLERK_DOCKET_THRESHOLD";

/// Environment variable name for overriding `--docket-batch-size` (`docs/architecture/nodes/docket.md` §5).
pub const ENV_DOCKET_BATCH_SIZE: &str = "CANON_CLERK_DOCKET_BATCH_SIZE";

/// Environment variable name for overriding `--docket-concurrency` (`docs/architecture/nodes/docket.md` §5).
pub const ENV_DOCKET_CONCURRENCY: &str = "CANON_CLERK_DOCKET_CONCURRENCY";

/// Domain input parameters for executing the `docket` pipeline stage.
#[derive(Debug, Clone, PartialEq)]
pub struct DocketOptions {
    /// Subject-matter jurisdiction score threshold in `[0.0, 1.0]` (defaults to [`DEFAULT_DOCKET_THRESHOLD`]).
    pub threshold: f64,
    /// Per-request timeout ceiling in milliseconds (defaults to [`DEFAULT_GENERATION_TIMEOUT_MS`]).
    pub timeout_ms: u64,
    /// Optional maximum candidate canons evaluated per `docket` model call (`>= 1`; `None` defers to the model's `DecisionBudgetProfile`).
    pub max_canons_per_call: Option<usize>,
    /// Optional maximum concurrent in-flight `docket` model calls when batched (`>= 1`; defaults to [`DEFAULT_DOCKET_MAX_CONCURRENT_CALLS`]).
    pub max_concurrent_calls: Option<usize>,
}

impl Default for DocketOptions {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_DOCKET_THRESHOLD,
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
            max_canons_per_call: None,
            max_concurrent_calls: None,
        }
    }
}

/// Domain errors produced during `docket` execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocketError {
    /// Configured threshold is non-finite or outside `[0.0, 1.0]`.
    InvalidThreshold { detail: String },
    /// Configured `max_canons_per_call` is zero (`0`).
    InvalidBatchSize { detail: String },
    /// Configured `max_concurrent_calls` is zero (`0`).
    InvalidConcurrency { detail: String },
    /// Case-or-Controversy violation: `.intake` is missing from the incoming `Caseload`.
    MissingIntake,
    /// Prerequisite `.discovery` namespace is missing from the incoming `Caseload`.
    MissingDiscovery,
    /// Upstream `.validation` recorded one or more candidate canon lint errors.
    ValidationFailed,
    /// Prerequisite `.config` namespace is missing when non-empty candidate canons require model screening.
    MissingConfig,
    /// A generative model was configured on `docket` instead of a discriminative `Noul` model.
    UnsupportedGenerativeModel { specifier: String },
    /// A candidate canon listed in `.discovery.candidate_canons` is absent from the workspace snapshot.
    MissingCandidateCanon { canon_ref: String },
    /// Multi-batch model call or response decoding failed with 1-based batch coordinates.
    BatchProvider {
        /// 1-based batch index (`1..=batch_count`).
        batch_index: usize,
        /// Total batches prepared for this stage (`>= 2`).
        batch_count: usize,
        /// Underlying provider timeout, transport, HTTP, block, or schema decoding error.
        source: ProviderError,
    },
    /// Single-batch or unscoped structured model provider error.
    Provider(ProviderError),
}

impl DocketError {
    pub(super) fn from_batch_call(
        batch_index: usize,
        batch_count: usize,
        source: ProviderError,
    ) -> Self {
        if batch_count > 1 {
            Self::BatchProvider {
                batch_index,
                batch_count,
                source,
            }
        } else {
            Self::Provider(source)
        }
    }
}

impl fmt::Display for DocketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidThreshold { detail } => write!(
                f,
                "Invalid docket threshold: {detail}.\n  \
                 Hint: Pass a finite threshold in `[0.0, 1.0]` via `--docket-threshold <FLOAT>` \
                 or `{ENV_DOCKET_THRESHOLD}` (default: `{DEFAULT_DOCKET_THRESHOLD}`)."
            ),
            Self::InvalidBatchSize { detail } => write!(
                f,
                "Invalid docket batch size: {detail}.\n  \
                 Hint: Pass a positive integer (`>= 1`) via `--docket-batch-size <COUNT>` \
                 or `{ENV_DOCKET_BATCH_SIZE}`."
            ),
            Self::InvalidConcurrency { detail } => write!(
                f,
                "Invalid docket concurrency limit: {detail}.\n  \
                 Hint: Pass a positive integer (`>= 1`) via `--docket-concurrency <COUNT>` \
                 or `{ENV_DOCKET_CONCURRENCY}` (default: `{DEFAULT_DOCKET_MAX_CONCURRENT_CALLS}`)."
            ),
            Self::MissingIntake => write!(
                f,
                "Macro jurisdiction triage (`docket`) requires an underlying filing in `.intake` (Case-or-Controversy invariant).\n  \
                 Hint: Pipe a diff (`--diff -`), specify target paths (`src/...`, `-`, `--all-targets`), \
                 or pass `--caseload <PATH|->` with `.intake` populated."
            ),
            Self::MissingDiscovery => write!(
                f,
                "Macro jurisdiction triage (`docket`) requires a resolved `.discovery` namespace.\n  \
                 Hint: Run `canon-clerk discover` first or invoke `canon-clerk docket` with a filing source."
            ),
            Self::ValidationFailed => write!(
                f,
                "Cannot execute `docket` because one or more candidate canons failed static validation.\n  \
                 Hint: Run `canon-clerk validate` to inspect and repair malformed candidate canons."
            ),
            Self::MissingConfig => write!(
                f,
                "Cannot execute `docket` screening without a resolved `.config` namespace on the Caseload.\n  \
                 Hint: Run `canon-clerk configure` first or invoke `canon-clerk docket` directly to resolve configuration JIT."
            ),
            Self::UnsupportedGenerativeModel { specifier } => write!(
                f,
                "Generative model `{specifier}` cannot be used for `docket` \
                 (`docket` requires a discriminative `Noul` decision model).\n  \
                 Hint: Configure a discriminator on `--docket-model` such as `{DEFAULT_GOOGLE_DISCRIMINATOR_MODEL}` \
                 (Google Decisions), `{DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL}` (TypeSafe Jev), \
                 or `{DEFAULT_LOCAL_DISCRIMINATOR_MODEL}` (local `/v1/systemone`)."
            ),
            Self::MissingCandidateCanon { canon_ref } => write!(
                f,
                "Candidate canon `{canon_ref}` referenced in `.discovery` was not found in the workspace snapshot.\n  \
                 Hint: Re-run `canon-clerk discover` against the current workspace."
            ),
            Self::BatchProvider {
                batch_index,
                batch_count,
                source,
            } => write!(
                f,
                "Docket screening failed (batch {batch_index}/{batch_count}): {source}"
            ),
            Self::Provider(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for DocketError {}

impl From<ProviderError> for DocketError {
    fn from(err: ProviderError) -> Self {
        Self::Provider(err)
    }
}

fn validate_batching_options(options: &DocketOptions) -> Result<(), DocketError> {
    if options.max_canons_per_call == Some(0) {
        return Err(DocketError::InvalidBatchSize {
            detail: "max_canons_per_call must be at least 1, got 0".to_string(),
        });
    }
    if options.max_concurrent_calls == Some(0) {
        return Err(DocketError::InvalidConcurrency {
            detail: "max_concurrent_calls must be at least 1, got 0".to_string(),
        });
    }
    Ok(())
}

/// Executes the pure `docket` domain stage against an in-memory [`WorkspaceSnapshot`] and
/// hexagonal [`StructuredModelProvider`] (`docs/architecture/nodes/docket.md` §4).
pub fn execute_docket(
    options: &DocketOptions,
    caseload: Caseload,
    workspace: &WorkspaceSnapshot,
    provider: &dyn StructuredModelProvider,
) -> Result<Caseload, DocketError> {
    let threshold = normalize_colorability_score(options.threshold)
        .map_err(|detail| DocketError::InvalidThreshold { detail })?;
    validate_batching_options(options)?;

    if caseload.docket.is_some() {
        return Ok(caseload);
    }

    let _intake = caseload.intake.as_ref().ok_or(DocketError::MissingIntake)?;
    let discovery = caseload
        .discovery
        .as_ref()
        .ok_or(DocketError::MissingDiscovery)?;

    if discovery.candidate_canons.is_empty() {
        return Ok(caseload.with_docket(CaseloadDocket::default()));
    }

    if caseload
        .validation
        .as_ref()
        .is_some_and(|val| val.has_errors)
    {
        return Err(DocketError::ValidationFailed);
    }

    let node_cfg = caseload
        .config
        .as_ref()
        .and_then(|c| c.node(HeuristicNode::Docket))
        .ok_or(DocketError::MissingConfig)?;
    if !is_discriminative_model(&node_cfg.model) {
        return Err(DocketError::UnsupportedGenerativeModel {
            specifier: node_cfg.model.clone(),
        });
    }

    decision::execute_docket_decisions(options, caseload, threshold, workspace, provider)
}

pub(crate) fn format_canon_batch_label(
    batch_idx: usize,
    total: usize,
    canon_count: usize,
) -> String {
    if total == 1 {
        format!("{canon_count} canon(s)")
    } else {
        format!("batch {} ({canon_count} canon(s))", batch_idx + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::caseload::{
        ActiveExhibits, CaseloadConfig, CaseloadDiscovery, CaseloadIntake, CaseloadValidation,
        ColorabilityStatus, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, DEFAULT_GOOGLE_GENERATOR_MODEL,
        FileArtifact, FileChangeStatus, LinkedIssueContext,
    };
    use crate::provider::{
        DecisionRequest, DecisionResponse, GenerationRequest, GenerationResponse, TokenUsage,
    };
    use std::collections::BTreeMap;
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
            panic!("docket must never invoke generate_structured")
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
                out.insert(key.clone(), self.scores.get(key).copied().unwrap_or(0.05));
            }
            Ok(DecisionResponse {
                scores: out,
                resolved_model: "gemini-decision-1".to_string(),
                latency_ms: 48,
                usage: TokenUsage {
                    prompt_tokens: 310,
                    completion_tokens: 0,
                    reasoning_tokens: 0,
                    total_tokens: 310,
                },
            })
        }
    }

    fn sample_workspace() -> WorkspaceSnapshot {
        WorkspaceSnapshot::new()
            .with_canon_markdown(
                "---\ntriggers:\n  - \"src/cli/**\"\n---\n# CLI Flags Kebab Case\n\nCLI flags MUST use kebab-case naming.\n",
                &ParseCanonOptions::with_source_ref(".canons/cli/cli-flags-kebab-case.md"),
            )
            .with_canon_markdown(
                "---\ntriggers:\n  - \"src/**\"\n---\n# Database Migrations Must Include Rollback\n\nDatabase schema migrations MUST define rollback steps.\n",
                &ParseCanonOptions::with_source_ref(".canons/db/migrations-rollback.md"),
            )
    }

    fn sample_filing_caseload(candidates: Vec<String>) -> Caseload {
        let mut diffs = BTreeMap::new();
        diffs.insert(
            "src/cli/docket.rs".to_string(),
            FileArtifact {
                path: "src/cli/docket.rs".to_string(),
                status: FileChangeStatus::Added,
                previous_path: None,
                lines_added: 18,
                lines_deleted: 0,
                patch: Some("@@ -0,0 +1,18 @@\n+#[arg(long = \"docket-threshold\")]\n".to_string()),
                patch_omission_reason: None,
                content: None,
                content_omission_reason: None,
            },
        );
        let mut triggers_join = BTreeMap::new();
        for c in &candidates {
            triggers_join.insert(c.clone(), vec!["src/cli/docket.rs".to_string()]);
        }

        Caseload::new()
            .with_intake(CaseloadIntake {
                pr_title: Some("feat(cli): add docket subcommand".to_string()),
                pr_body: Some("Implements macro jurisdiction screening.".to_string()),
                intent: None,
                diffs,
                linked_issues: Some(vec![LinkedIssueContext {
                    number: 260,
                    title: Some("Implement docket stage".to_string()),
                    author: None,
                    labels: Vec::new(),
                    body: Some("Add CaseloadDocket and CLI.".to_string()),
                    url: None,
                }]),
                ..CaseloadIntake::default()
            })
            .with_discovery(CaseloadDiscovery {
                target_files: vec!["src/cli/docket.rs".to_string()],
                candidate_canons: candidates,
                active_exhibits: ActiveExhibits {
                    files: vec!["src/cli/docket.rs".to_string()],
                    pr_title: Some(true),
                    pr_body: Some(true),
                    commit_messages: None,
                    linked_issues: Some(true),
                },
                triggers_join,
                ..CaseloadDiscovery::default()
            })
            .with_validation(CaseloadValidation {
                has_errors: false,
                results: BTreeMap::new(),
            })
            .with_config(
                CaseloadConfig::default()
                    .with_node_model(HeuristicNode::Docket, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL),
            )
    }

    #[test]
    fn test_empty_candidate_canons_short_circuits_with_zero_model_calls_and_no_config_needed() {
        let ws = sample_workspace();
        let provider = RecordingDecisionProvider::with_scores(BTreeMap::new());
        let caseload = Caseload::new()
            .with_intake(CaseloadIntake::default())
            .with_discovery(CaseloadDiscovery::default());

        let enriched = execute_docket(&DocketOptions::default(), caseload, &ws, &provider)
            .expect("zero-candidate docket must short-circuit cleanly");

        assert!(provider.captured_requests().is_empty());
        assert!(enriched.config.is_none());
        let docket = enriched.docket.expect("docket namespace populated");
        assert!(docket.cases.is_empty());
        assert!(docket.active_docket.is_empty());
    }

    #[test]
    fn test_fast_forwards_when_docket_already_populated() {
        let ws = sample_workspace();
        let provider = RecordingDecisionProvider::with_scores(BTreeMap::new());
        let pre_populated = Caseload::new()
            .with_intake(CaseloadIntake::default())
            .with_docket(CaseloadDocket::default());

        let out = execute_docket(
            &DocketOptions::default(),
            pre_populated.clone(),
            &ws,
            &provider,
        )
        .expect("fast-forward completed docket");
        assert_eq!(out, pre_populated);
        assert!(provider.captured_requests().is_empty());
    }

    #[test]
    fn test_enforces_prerequisite_and_generator_rejection_guards() {
        let ws = sample_workspace();
        let provider = RecordingDecisionProvider::with_scores(BTreeMap::new());

        let abstract_corpus = Caseload::new()
            .with_discovery(CaseloadDiscovery {
                candidate_canons: vec![".canons/cli/cli-flags-kebab-case.md".to_string()],
                ..CaseloadDiscovery::default()
            })
            .with_validation(CaseloadValidation::default());
        assert_eq!(
            execute_docket(&DocketOptions::default(), abstract_corpus, &ws, &provider).unwrap_err(),
            DocketError::MissingIntake
        );

        let no_disc = Caseload::new().with_intake(CaseloadIntake::default());
        assert_eq!(
            execute_docket(&DocketOptions::default(), no_disc, &ws, &provider).unwrap_err(),
            DocketError::MissingDiscovery
        );

        let mut failed_val =
            sample_filing_caseload(vec![".canons/cli/cli-flags-kebab-case.md".to_string()]);
        failed_val.validation = Some(CaseloadValidation {
            has_errors: true,
            results: BTreeMap::new(),
        });
        assert_eq!(
            execute_docket(&DocketOptions::default(), failed_val, &ws, &provider).unwrap_err(),
            DocketError::ValidationFailed
        );

        let mut no_cfg =
            sample_filing_caseload(vec![".canons/cli/cli-flags-kebab-case.md".to_string()]);
        no_cfg.config = None;
        assert_eq!(
            execute_docket(&DocketOptions::default(), no_cfg, &ws, &provider).unwrap_err(),
            DocketError::MissingConfig
        );

        let mut gen_cfg =
            sample_filing_caseload(vec![".canons/cli/cli-flags-kebab-case.md".to_string()]);
        gen_cfg.config = Some(
            CaseloadConfig::default()
                .with_node_model(HeuristicNode::Docket, DEFAULT_GOOGLE_GENERATOR_MODEL),
        );
        let gen_err = execute_docket(&DocketOptions::default(), gen_cfg, &ws, &provider)
            .expect_err("generator model on docket must be rejected");
        assert!(matches!(
            gen_err,
            DocketError::UnsupportedGenerativeModel { .. }
        ));
        let msg = gen_err.to_string();
        assert!(msg.contains("google:gemini-decision-1"));
        assert!(msg.contains("typesafe:jev-latest"));
        assert!(msg.contains("local:kev-4b"));

        let missing_canon = sample_filing_caseload(vec![".canons/nonexistent.md".to_string()]);
        assert_eq!(
            execute_docket(&DocketOptions::default(), missing_canon, &ws, &provider).unwrap_err(),
            DocketError::MissingCandidateCanon {
                canon_ref: ".canons/nonexistent.md".to_string()
            }
        );

        for bad in [f64::NAN, -0.1, 1.1] {
            let err = execute_docket(
                &DocketOptions {
                    threshold: bad,
                    ..DocketOptions::default()
                },
                Caseload::new(),
                &ws,
                &provider,
            )
            .expect_err("out-of-bounds threshold must fail");
            assert!(matches!(err, DocketError::InvalidThreshold { .. }));
        }
    }

    #[test]
    fn test_rejects_zero_batch_size_and_zero_concurrency_with_hints() {
        let ws = sample_workspace();
        let provider = RecordingDecisionProvider::with_scores(BTreeMap::new());
        let caseload =
            sample_filing_caseload(vec![".canons/cli/cli-flags-kebab-case.md".to_string()]);

        let batch_err = execute_docket(
            &DocketOptions {
                max_canons_per_call: Some(0),
                ..DocketOptions::default()
            },
            caseload.clone(),
            &ws,
            &provider,
        )
        .expect_err("zero batch size must fail");
        assert!(matches!(batch_err, DocketError::InvalidBatchSize { .. }));
        assert!(batch_err.to_string().contains("--docket-batch-size"));

        let conc_err = execute_docket(
            &DocketOptions {
                max_concurrent_calls: Some(0),
                ..DocketOptions::default()
            },
            caseload,
            &ws,
            &provider,
        )
        .expect_err("zero concurrency must fail");
        assert!(matches!(conc_err, DocketError::InvalidConcurrency { .. }));
        assert!(conc_err.to_string().contains("--docket-concurrency"));
    }

    #[test]
    fn test_discriminative_docket_screens_and_folds_active_docket() {
        let ws = sample_workspace();
        let keep = ".canons/cli/cli-flags-kebab-case.md";
        let drop = ".canons/db/migrations-rollback.md";
        let caseload = sample_filing_caseload(vec![keep.to_string(), drop.to_string()]);
        let provider = RecordingDecisionProvider::with_scores(BTreeMap::from([
            (keep.to_string(), 0.94),
            (drop.to_string(), 0.08),
        ]));

        let out = execute_docket(&DocketOptions::default(), caseload, &ws, &provider)
            .expect("docket succeeds");
        let docket = out.docket.expect("docket populated");
        assert_eq!(docket.active_docket, vec![keep.to_string()]);
        assert_eq!(
            docket.cases[keep].colorability_status,
            ColorabilityStatus::Docketed
        );
        assert_eq!(
            docket.cases[drop].colorability_status,
            ColorabilityStatus::Dismissed
        );
        assert_eq!(provider.captured_requests().len(), 1);
    }

    #[test]
    fn test_propagates_provider_error_with_batch_coordinates_when_batched() {
        let ws = sample_workspace();
        let keep = ".canons/cli/cli-flags-kebab-case.md";
        let drop = ".canons/db/migrations-rollback.md";
        let caseload = sample_filing_caseload(vec![keep.to_string(), drop.to_string()]);
        let provider = RecordingDecisionProvider::with_error(ProviderError::Timeout {
            model_specifier: DEFAULT_GOOGLE_DISCRIMINATOR_MODEL.to_string(),
            timeout_ms: 500,
        });

        let err = execute_docket(
            &DocketOptions {
                max_canons_per_call: Some(1),
                ..DocketOptions::default()
            },
            caseload,
            &ws,
            &provider,
        )
        .expect_err("timeout must propagate");
        assert!(matches!(
            err,
            DocketError::BatchProvider { batch_count: 2, .. }
        ));
    }
}
