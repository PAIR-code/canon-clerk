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
mod decode;
mod prompt;

pub(crate) use decode::resolve_canon_path;

use crate::caseload::{
    CanonDocketAssessment, Caseload, CaseloadDocket, HeuristicNode, normalize_colorability_score,
};
use crate::concurrency::execute_bounded_stage_work;
use crate::configure::is_discriminative_model;
use crate::discover::WorkspaceSnapshot;
use crate::provider::{
    DEFAULT_GENERATION_TIMEOUT_MS, GenerationRequest, ProviderError, StructuredModelProvider,
};
use decode::decode_docket_cases;
use prompt::{
    DOCKET_SYSTEM_INSTRUCTION, build_bounded_docket_filing_prefix, build_docket_batch_user_prompt,
    build_docket_schema,
};
use std::collections::BTreeMap;
use std::fmt;

/// Default subject-matter jurisdiction threshold for `docket` (`docs/architecture/nodes/docket.md` §4.4).
pub const DEFAULT_DOCKET_THRESHOLD: f64 = 0.5;

/// Default maximum candidate canons evaluated per generative `docket` model call (`docs/architecture/nodes/docket.md` §4.2).
pub const DEFAULT_DOCKET_BATCH_SIZE: usize = 25;

/// Default total character ceiling for the shared generative `docket` filing prefix (`~16k` tokens).
pub const MAX_DOCKET_FILING_PREFIX_CHARS: usize = 64_000;

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
    /// Optional maximum candidate canons evaluated per `docket` model call (`>= 1`; defaults to `Some(DEFAULT_DOCKET_BATCH_SIZE)`, while `None` evaluates all in a single unbounded turn).
    pub max_canons_per_call: Option<usize>,
    /// Optional maximum concurrent in-flight `docket` model calls when batched (`>= 1`; defaults to [`DEFAULT_DOCKET_MAX_CONCURRENT_CALLS`]).
    pub max_concurrent_calls: Option<usize>,
}

impl Default for DocketOptions {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_DOCKET_THRESHOLD,
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
            max_canons_per_call: Some(DEFAULT_DOCKET_BATCH_SIZE),
            max_concurrent_calls: None,
        }
    }
}

/// Resolves the effective `max_canons_per_call` for `docket` given an optional explicit override
/// and the configured `docket_model` (`None` on discriminative models so their `DecisionBudgetProfile`
/// governs batching, `Some(DEFAULT_DOCKET_BATCH_SIZE)` on generative models).
pub(crate) fn effective_docket_batch_size(
    explicit: Option<usize>,
    docket_model: &str,
) -> Option<usize> {
    if explicit.is_some() {
        explicit
    } else if is_discriminative_model(docket_model) {
        None
    } else {
        Some(DEFAULT_DOCKET_BATCH_SIZE)
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
                 or `{ENV_DOCKET_BATCH_SIZE}` (default: `{DEFAULT_DOCKET_BATCH_SIZE}`)."
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

/// Partitions `candidate_canons` into deterministic, balanced contiguous batches of at most `max_canons_per_call`.
pub fn partition_candidate_canons(
    candidate_canons: &[String],
    max_canons_per_call: Option<usize>,
) -> Vec<Vec<String>> {
    let total = candidate_canons.len();
    if total == 0 {
        return Vec::new();
    }
    match max_canons_per_call {
        Some(batch_size) if batch_size >= 1 && batch_size < total => {
            let num_batches = total.div_ceil(batch_size);
            let base_len = total / num_batches;
            let remainder = total % num_batches;
            let mut batches = Vec::with_capacity(num_batches);
            let mut offset = 0;
            for idx in 0..num_batches {
                let len = base_len + usize::from(idx < remainder);
                batches.push(candidate_canons[offset..offset + len].to_vec());
                offset += len;
            }
            batches
        }
        _ => vec![candidate_canons.to_vec()],
    }
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

    let intake = caseload.intake.as_ref().ok_or(DocketError::MissingIntake)?;
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
    if is_discriminative_model(&node_cfg.model) {
        return decision::execute_docket_decisions(
            options, caseload, threshold, workspace, provider,
        );
    }

    let docket_model = node_cfg.model.clone();
    let (thinking_level, reasoning_budget) = node_cfg.effective_screening_thinking();
    let filing_prefix =
        build_bounded_docket_filing_prefix(intake, discovery, MAX_DOCKET_FILING_PREFIX_CHARS);
    let response_schema = build_docket_schema();
    let batches =
        partition_candidate_canons(&discovery.candidate_canons, options.max_canons_per_call);

    let total_batches = batches.len();
    let mut prepared_batches = Vec::with_capacity(total_batches);
    for (idx, batch_canons) in batches.into_iter().enumerate() {
        let user_prompt =
            build_docket_batch_user_prompt(&filing_prefix, &batch_canons, discovery, workspace)
                .map_err(|canon_ref| DocketError::MissingCandidateCanon { canon_ref })?;
        let call_label = (total_batches > 1).then(|| format!("batch {}/{total_batches}", idx + 1));
        let request = GenerationRequest {
            model_specifier: docket_model.clone(),
            system_instruction: DOCKET_SYSTEM_INSTRUCTION.to_string(),
            user_prompt,
            response_schema: response_schema.clone(),
            thinking_level,
            reasoning_budget,
            timeout_ms: options.timeout_ms,
            call_label,
        };
        prepared_batches.push((batch_canons, request));
    }

    let concurrency_limit = options
        .max_concurrent_calls
        .unwrap_or(DEFAULT_DOCKET_MAX_CONCURRENT_CALLS);
    let cases = execute_docket_batches(
        &prepared_batches,
        concurrency_limit,
        threshold,
        &docket_model,
        provider,
    )?;

    Ok(caseload.with_docket(CaseloadDocket::from_cases(cases)))
}

type BatchCasesResult = Result<BTreeMap<String, CanonDocketAssessment>, DocketError>;

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

fn execute_docket_batches(
    prepared_batches: &[(Vec<String>, GenerationRequest)],
    concurrency_limit: usize,
    threshold: f64,
    model_specifier: &str,
    provider: &dyn StructuredModelProvider,
) -> BatchCasesResult {
    let total = prepared_batches.len();
    let batch_maps = execute_bounded_stage_work(
        prepared_batches,
        concurrency_limit,
        provider,
        |idx, (batch_canons, request)| {
            let batch_cases = provider
                .generate_structured(request)
                .and_then(|response| {
                    decode_docket_cases(&response.payload, batch_canons, threshold, model_specifier)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::caseload::{
        ActiveExhibits, CaseloadConfig, CaseloadDiscovery, CaseloadIntake, CaseloadValidation,
        DEFAULT_GENERATIVE_SCREENING_MODEL, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, FileArtifact,
        FileChangeStatus, LinkedIssueContext,
    };
    use crate::provider::{DEFAULT_SCREENING_REASONING_BUDGET, GenerationResponse, TokenUsage};
    use serde_json::{Value, json};
    use std::sync::Mutex;

    struct RecordingMockProvider {
        requests: Mutex<Vec<GenerationRequest>>,
        outcome: Result<Value, ProviderError>,
    }

    impl RecordingMockProvider {
        fn with_payload(payload: Value) -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
                outcome: Ok(payload),
            }
        }

        fn with_error(err: ProviderError) -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
                outcome: Err(err),
            }
        }

        fn captured_requests(&self) -> Vec<GenerationRequest> {
            self.requests.lock().expect("lock requests").clone()
        }
    }

    impl StructuredModelProvider for RecordingMockProvider {
        fn generate_structured(
            &self,
            request: &GenerationRequest,
        ) -> Result<GenerationResponse, ProviderError> {
            self.requests
                .lock()
                .expect("lock requests")
                .push(request.clone());
            match &self.outcome {
                Ok(payload) => Ok(GenerationResponse {
                    payload: payload.clone(),
                    thoughts: Vec::new(),
                    usage: TokenUsage {
                        prompt_tokens: 310,
                        completion_tokens: 72,
                        reasoning_tokens: 0,
                        total_tokens: 382,
                    },
                    latency_ms: 48,
                    resolved_model: "gemini-2.5-flash-lite".to_string(),
                }),
                Err(err) => Err(err.clone()),
            }
        }
    }

    fn sample_workspace() -> WorkspaceSnapshot {
        let mut ws = WorkspaceSnapshot::new();
        ws = ws.with_canon_markdown(
            "---\ntriggers:\n  - \"src/cli/**\"\n---\n# CLI Flags Kebab Case\n\nCLI flags MUST use kebab-case naming.\n",
            &ParseCanonOptions::with_source_ref(".canons/cli/cli-flags-kebab-case.md"),
        );
        ws = ws.with_canon_markdown(
            "---\ntriggers:\n  - \"src/**\"\n---\n# Database Migrations Must Include Rollback\n\nDatabase schema migrations MUST define rollback steps.\n",
            &ParseCanonOptions::with_source_ref(".canons/db/migrations-rollback.md"),
        );
        ws
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
                    .with_node_model(HeuristicNode::Docket, DEFAULT_GENERATIVE_SCREENING_MODEL),
            )
    }

    mod zero_candidate_and_prerequisite_guards {
        use super::*;

        #[test]
        fn test_empty_candidate_canons_short_circuits_with_zero_model_calls_and_no_config_needed() {
            let ws = sample_workspace();
            let provider = RecordingMockProvider::with_payload(json!({}));
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
            assert!(docket.anomalies.is_empty());
        }

        #[test]
        fn test_fast_forwards_when_docket_already_populated() {
            let ws = sample_workspace();
            let provider = RecordingMockProvider::with_payload(json!({}));
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
        fn test_enforces_case_or_controversy_and_prerequisite_guards() {
            let ws = sample_workspace();
            let provider = RecordingMockProvider::with_payload(json!({}));

            // Abstract corpus-only caseload (from `validate --all-canons`) lacks `.intake`
            let abstract_corpus = Caseload::new()
                .with_discovery(CaseloadDiscovery {
                    candidate_canons: vec![".canons/cli/cli-flags-kebab-case.md".to_string()],
                    ..CaseloadDiscovery::default()
                })
                .with_validation(CaseloadValidation::default());
            let err = execute_docket(&DocketOptions::default(), abstract_corpus, &ws, &provider)
                .expect_err("null intake must violate Case-or-Controversy");
            assert_eq!(err, DocketError::MissingIntake);
            assert!(err.to_string().contains("Case-or-Controversy"));

            // Missing discovery
            let no_disc = Caseload::new().with_intake(CaseloadIntake::default());
            assert_eq!(
                execute_docket(&DocketOptions::default(), no_disc, &ws, &provider).unwrap_err(),
                DocketError::MissingDiscovery
            );

            // Validation failed
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

            // Missing config when candidates > 0
            let mut no_cfg =
                sample_filing_caseload(vec![".canons/cli/cli-flags-kebab-case.md".to_string()]);
            no_cfg.config = None;
            assert_eq!(
                execute_docket(&DocketOptions::default(), no_cfg, &ws, &provider).unwrap_err(),
                DocketError::MissingConfig
            );

            // Missing candidate canon in snapshot
            let missing_canon = sample_filing_caseload(vec![".canons/nonexistent.md".to_string()]);
            assert_eq!(
                execute_docket(&DocketOptions::default(), missing_canon, &ws, &provider)
                    .unwrap_err(),
                DocketError::MissingCandidateCanon {
                    canon_ref: ".canons/nonexistent.md".to_string()
                }
            );

            // Invalid thresholds
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
            let provider = RecordingMockProvider::with_payload(json!({}));

            let zero_batch = execute_docket(
                &DocketOptions {
                    max_canons_per_call: Some(0),
                    ..DocketOptions::default()
                },
                Caseload::new(),
                &ws,
                &provider,
            )
            .expect_err("zero batch size must fail");
            assert!(matches!(zero_batch, DocketError::InvalidBatchSize { .. }));
            assert!(zero_batch.to_string().contains("Hint:"));

            let zero_conc = execute_docket(
                &DocketOptions {
                    max_concurrent_calls: Some(0),
                    ..DocketOptions::default()
                },
                Caseload::new(),
                &ws,
                &provider,
            )
            .expect_err("zero concurrency must fail");
            assert!(matches!(zero_conc, DocketError::InvalidConcurrency { .. }));
            assert!(zero_conc.to_string().contains("Hint:"));
        }
    }

    mod aggregate_screening_and_threshold_gating {
        use super::*;

        #[test]
        fn test_single_turn_domain_indirected_reason_first_schema_and_deterministic_active_docket()
        {
            let ws = sample_workspace();
            // Emit items in reverse lexicographical order to verify deterministic BTreeMap + active_docket sorting
            let provider = RecordingMockProvider::with_payload(json!({
                "cases": [
                    {
                        "canon_path": ".canons/db/migrations-rollback.md",
                        "colorability_summary": " Filing only adds CLI flag handling and touches zero database migrations. ",
                        "colorability_score": 0.04
                    },
                    {
                        "canon_path": ".canons/cli/cli-flags-kebab-case.md",
                        "colorability_summary": "Filing introduces `--docket-threshold` flag in `src/cli/docket.rs`.",
                        "colorability_score": 0.95
                    }
                ]
            }));

            let caseload = sample_filing_caseload(vec![
                ".canons/cli/cli-flags-kebab-case.md".to_string(),
                ".canons/db/migrations-rollback.md".to_string(),
            ]);

            let enriched = execute_docket(&DocketOptions::default(), caseload, &ws, &provider)
                .expect("execute docket screening");

            let requests = provider.captured_requests();
            assert_eq!(requests.len(), 1, "docket must execute in a single turn");
            let req = &requests[0];
            assert_eq!(req.model_specifier, DEFAULT_GENERATIVE_SCREENING_MODEL);
            assert_eq!(req.reasoning_budget, DEFAULT_SCREENING_REASONING_BUDGET);
            assert!(req.user_prompt.contains("feat(cli): add docket subcommand"));
            assert!(req.user_prompt.contains("#260: Implement docket stage"));
            assert!(req.user_prompt.contains("src/cli/docket.rs"));
            assert!(req.user_prompt.contains("docket-threshold"));

            let ordering = &req.response_schema["properties"]["cases"]["items"]["propertyOrdering"];
            assert_eq!(
                ordering,
                &json!(["canon_path", "colorability_summary", "colorability_score"])
            );

            let docket = enriched.docket.expect("docket attached");
            assert_eq!(
                docket.active_docket,
                vec![".canons/cli/cli-flags-kebab-case.md".to_string()]
            );
            assert_eq!(docket.docketed_count(), 1);
            assert_eq!(docket.dismissed_count(), 1);
            assert_eq!(
                docket.cases[".canons/db/migrations-rollback.md"].colorability_summary,
                "Filing only adds CLI flag handling and touches zero database migrations."
            );
        }

        #[test]
        fn test_scoped_canon_omits_redundant_scope_line_from_docket_prompt() {
            let mut ws = WorkspaceSnapshot::new();
            let scoped_ref = "packages/ui/.canons/accessibility.md";
            ws = ws.with_canon_markdown(
                "---\ntriggers:\n  - \"**/*.tsx\"\n---\n# Accessible Buttons\n\nButtons MUST provide an accessible label.\n",
                &ParseCanonOptions::with_source_ref(scoped_ref),
            );
            assert_eq!(
                ws.canons[scoped_ref].scope.as_deref(),
                Some("packages/ui"),
                "internal monorepo package scope must remain intact on Canon"
            );

            let provider = RecordingMockProvider::with_payload(json!({
                "cases": [
                    {
                        "canon_path": scoped_ref,
                        "colorability_summary": "Touches UI button component.",
                        "colorability_score": 0.90
                    }
                ]
            }));
            let caseload = sample_filing_caseload(vec![scoped_ref.to_string()]);
            execute_docket(&DocketOptions::default(), caseload, &ws, &provider)
                .expect("execute docket for scoped canon");

            let prompt = &provider.captured_requests()[0].user_prompt;
            assert!(prompt.contains("### Canon: `packages/ui/.canons/accessibility.md`"));
            assert!(
                !prompt.contains("- Scope:"),
                "docket prompt must not emit redundant `- Scope:` line: {prompt}"
            );
        }

        struct PerBatchDocketProvider {
            requests: Mutex<Vec<GenerationRequest>>,
            scores: BTreeMap<String, f64>,
        }

        impl StructuredModelProvider for PerBatchDocketProvider {
            fn generate_structured(
                &self,
                request: &GenerationRequest,
            ) -> Result<GenerationResponse, ProviderError> {
                self.requests
                    .lock()
                    .expect("lock requests")
                    .push(request.clone());
                let mut cases = Vec::new();
                for line in request.user_prompt.lines() {
                    if let Some(rest) = line.strip_prefix("### Canon: `")
                        && let Some(canon_ref) = rest.strip_suffix('`')
                    {
                        let score = self.scores.get(canon_ref).copied().unwrap_or(0.0);
                        cases.push(json!({
                            "canon_path": canon_ref,
                            "colorability_summary": format!("Evaluated {canon_ref}."),
                            "colorability_score": score
                        }));
                    }
                }
                Ok(GenerationResponse {
                    payload: json!({ "cases": cases }),
                    thoughts: Vec::new(),
                    usage: TokenUsage {
                        prompt_tokens: 310,
                        completion_tokens: 72,
                        reasoning_tokens: 0,
                        total_tokens: 382,
                    },
                    latency_ms: 48,
                    resolved_model: "gemini-4-hydrogen".to_string(),
                })
            }
        }

        struct PrefixCapturingProvider {
            prompts: Mutex<Vec<String>>,
        }

        impl StructuredModelProvider for PrefixCapturingProvider {
            fn generate_structured(
                &self,
                request: &GenerationRequest,
            ) -> Result<GenerationResponse, ProviderError> {
                self.prompts
                    .lock()
                    .expect("lock prompts")
                    .push(request.user_prompt.clone());
                let mut cases = Vec::new();
                for line in request.user_prompt.lines() {
                    if let Some(rest) = line.strip_prefix("### Canon: `")
                        && let Some(canon_ref) = rest.strip_suffix('`')
                    {
                        cases.push(json!({
                            "canon_path": canon_ref,
                            "colorability_summary": "Evaluated.",
                            "colorability_score": 0.80
                        }));
                    }
                }
                Ok(GenerationResponse {
                    payload: json!({ "cases": cases }),
                    thoughts: Vec::new(),
                    usage: TokenUsage::default(),
                    latency_ms: 20,
                    resolved_model: "gemini-4-hydrogen".to_string(),
                })
            }
        }

        fn build_large_60_file_caseload() -> (WorkspaceSnapshot, Caseload, Vec<String>) {
            let mut ws = WorkspaceSnapshot::new();
            let mut candidates = Vec::new();
            let mut triggers_join = BTreeMap::new();
            for i in 0..30 {
                let canon_ref = format!(".canons/pack/rule-{i:03}.md");
                ws = ws.with_canon_markdown(
                    &format!("# Rule {i}\n\nRule {i} MUST hold.\n"),
                    &ParseCanonOptions::with_source_ref(canon_ref.clone()),
                );
                triggers_join.insert(canon_ref.clone(), vec!["src/mod_000.rs".to_string()]);
                candidates.push(canon_ref);
            }

            let mut diffs = BTreeMap::new();
            let mut target_files = Vec::new();
            let patch_line =
                "+pub const FEATURE_CONSTANT_ENTRY: &str = \"value-segment-padding-0123456789\";\n";
            let large_patch = format!("@@ -0,0 +1,52 @@\n{}", patch_line.repeat(52));
            assert!(large_patch.len() > 3_900);

            for idx in 0..60 {
                let path = format!("src/mod_{idx:03}.rs");
                target_files.push(path.clone());
                diffs.insert(
                    path.clone(),
                    FileArtifact {
                        path,
                        status: FileChangeStatus::Modified,
                        previous_path: None,
                        lines_added: 52,
                        lines_deleted: 0,
                        patch: Some(large_patch.clone()),
                        patch_omission_reason: None,
                        content: None,
                        content_omission_reason: None,
                    },
                );
            }

            let caseload = Caseload::new()
                .with_intake(CaseloadIntake {
                    pr_title: Some("feat: touch 60 modules".to_string()),
                    diffs,
                    ..CaseloadIntake::default()
                })
                .with_discovery(CaseloadDiscovery {
                    target_files: target_files.clone(),
                    candidate_canons: candidates,
                    active_exhibits: ActiveExhibits {
                        files: target_files.clone(),
                        ..ActiveExhibits::default()
                    },
                    triggers_join,
                    ..CaseloadDiscovery::default()
                })
                .with_validation(CaseloadValidation::default())
                .with_config(
                    CaseloadConfig::default()
                        .with_node_model(HeuristicNode::Docket, DEFAULT_GENERATIVE_SCREENING_MODEL),
                );
            (ws, caseload, target_files)
        }

        #[test]
        fn test_constant_size_wire_schema_and_default_batching_across_60_candidate_canons() {
            let mut ws = WorkspaceSnapshot::new();
            let mut candidates = Vec::new();
            let mut scores = BTreeMap::new();
            for i in 0..60 {
                let path = format!(".canons/pack/rule-{i:03}.md");
                ws = ws.with_canon_markdown(
                    &format!("# Rule {i}\n\nRule {i} MUST hold.\n"),
                    &ParseCanonOptions::with_source_ref(path.clone()),
                );
                let score = if i == 30 { 0.70 } else { 0.69 };
                scores.insert(path.clone(), score);
                candidates.push(path);
            }

            let baseline_schema = build_docket_schema();
            let provider = PerBatchDocketProvider {
                requests: Mutex::new(Vec::new()),
                scores: scores.clone(),
            };
            let caseload = sample_filing_caseload(candidates);

            let enriched = execute_docket(
                &DocketOptions {
                    threshold: 0.70,
                    ..DocketOptions::default()
                },
                caseload.clone(),
                &ws,
                &provider,
            )
            .expect("60-canon docket succeeds across default bounded batches");

            let mut requests = provider.requests.lock().expect("lock requests").clone();
            requests.sort_by(|a, b| a.call_label.cmp(&b.call_label));
            assert_eq!(
                requests.len(),
                3,
                "60 canons must partition into 3 balanced batches of 20 under DEFAULT_DOCKET_BATCH_SIZE=25"
            );
            let labels: Vec<Option<&str>> =
                requests.iter().map(|r| r.call_label.as_deref()).collect();
            assert_eq!(
                labels,
                vec![Some("batch 1/3"), Some("batch 2/3"), Some("batch 3/3")]
            );
            for req in &requests {
                assert_eq!(
                    req.response_schema, baseline_schema,
                    "wire schema must remain O(1) constant size regardless of candidate canon count"
                );
            }

            let docket = enriched.docket.as_ref().expect("docket present");
            assert_eq!(docket.cases.len(), 60);
            assert_eq!(
                docket.active_docket,
                vec![".canons/pack/rule-030.md".to_string()],
                "exact threshold boundary score == 0.70 must be docketed"
            );

            // Explicit `None` preserves single-turn unbounded execution (used by `consistency_bench --batch-size unbounded`)
            let unbounded_provider = PerBatchDocketProvider {
                requests: Mutex::new(Vec::new()),
                scores,
            };
            let unbounded = execute_docket(
                &DocketOptions {
                    threshold: 0.70,
                    max_canons_per_call: None,
                    ..DocketOptions::default()
                },
                caseload,
                &ws,
                &unbounded_provider,
            )
            .expect("explicit unbounded 60-canon docket succeeds");
            assert_eq!(
                unbounded_provider
                    .requests
                    .lock()
                    .expect("lock requests")
                    .len(),
                1
            );
            assert_eq!(unbounded, enriched);
        }

        #[test]
        fn test_generative_docket_bounds_large_multi_file_filing_prefix_fairly_and_caches_across_batches()
         {
            let (ws, caseload, target_files) = build_large_60_file_caseload();
            let provider = PrefixCapturingProvider {
                prompts: Mutex::new(Vec::new()),
            };
            execute_docket(&DocketOptions::default(), caseload, &ws, &provider)
                .expect("60-file 30-canon docket succeeds");

            let prompts = provider.prompts.lock().expect("lock prompts");
            assert_eq!(prompts.len(), 2, "30 canons split into 2 batches of 15");

            let prefixes: Vec<&str> = prompts
                .iter()
                .map(|p| {
                    p.split_once("## Candidate Canons")
                        .expect("candidate canons marker")
                        .0
                })
                .collect();
            assert_eq!(
                prefixes[0], prefixes[1],
                "shared filing prefix must remain byte-for-byte identical across concurrent batches for prefix caching"
            );
            assert!(
                prefixes[0].len() <= MAX_DOCKET_FILING_PREFIX_CHARS,
                "generative filing prefix ({} chars) must stay bounded within MAX_DOCKET_FILING_PREFIX_CHARS ({MAX_DOCKET_FILING_PREFIX_CHARS})",
                prefixes[0].len()
            );

            // Every single modified file (including the 60th file `src/mod_059.rs`) must retain its header AND a diff preview!
            for path in &target_files {
                let header = format!("### File: `{path}`");
                let pos = prefixes[0]
                    .find(&header)
                    .unwrap_or_else(|| panic!("missing modified file header `{header}`"));
                let after = &prefixes[0][pos..];
                let next_file = after[header.len()..]
                    .find("### File: `")
                    .map_or(after.len(), |idx| header.len() + idx);
                let section = &after[..next_file];
                assert!(
                    section.contains("```diff\n@@ -0,0 +1,52 @@"),
                    "file `{path}` must retain a fair-share diff preview block, got:\n{section}"
                );
            }
        }
    }

    mod malformed_and_failed_provider_responses {
        use super::*;

        #[test]
        fn test_rejects_non_array_missing_unknown_duplicate_and_out_of_range_payloads() {
            let ws = sample_workspace();
            let two_canons = vec![
                ".canons/cli/cli-flags-kebab-case.md".to_string(),
                ".canons/db/migrations-rollback.md".to_string(),
            ];

            let bad_payloads = vec![
                // Non-array cases
                json!({ "cases": {} }),
                // Non-object element
                json!({ "cases": ["not-an-object"] }),
                // Unknown canon_path
                json!({
                    "cases": [{
                        "canon_path": ".canons/unknown.md",
                        "colorability_summary": "Unknown",
                        "colorability_score": 0.5
                    }]
                }),
                // Duplicate canon_path
                json!({
                    "cases": [
                        {
                            "canon_path": ".canons/cli/cli-flags-kebab-case.md",
                            "colorability_summary": "First",
                            "colorability_score": 0.8
                        },
                        {
                            "canon_path": ".canons/cli/cli-flags-kebab-case.md",
                            "colorability_summary": "Duplicate",
                            "colorability_score": 0.8
                        }
                    ]
                }),
                // Missing second candidate canon
                json!({
                    "cases": [{
                        "canon_path": ".canons/cli/cli-flags-kebab-case.md",
                        "colorability_summary": "Only one",
                        "colorability_score": 0.8
                    }]
                }),
                // Out-of-range score > 1.0
                json!({
                    "cases": [
                        {
                            "canon_path": ".canons/cli/cli-flags-kebab-case.md",
                            "colorability_summary": "Too high",
                            "colorability_score": 1.2
                        },
                        {
                            "canon_path": ".canons/db/migrations-rollback.md",
                            "colorability_summary": "Ok",
                            "colorability_score": 0.1
                        }
                    ]
                }),
            ];

            for payload in bad_payloads {
                let provider = RecordingMockProvider::with_payload(payload);
                let caseload = sample_filing_caseload(two_canons.clone());
                let err = execute_docket(&DocketOptions::default(), caseload, &ws, &provider)
                    .expect_err("malformed payload must fail");
                assert!(
                    matches!(
                        err,
                        DocketError::Provider(ProviderError::SchemaDecode { .. })
                    ),
                    "expected SchemaDecode error, got: {err:?}"
                );
            }

            let transport_provider = RecordingMockProvider::with_error(ProviderError::Timeout {
                model_specifier: DEFAULT_GOOGLE_DISCRIMINATOR_MODEL.to_string(),
                timeout_ms: 5_000,
            });
            let err = execute_docket(
                &DocketOptions::default(),
                sample_filing_caseload(two_canons),
                &ws,
                &transport_provider,
            )
            .expect_err("provider timeout must propagate");
            assert!(matches!(
                err,
                DocketError::Provider(ProviderError::Timeout { .. })
            ));
        }
    }

    mod batching_and_concurrency {
        use super::*;
        use crate::telemetry::{
            PipelineStage, RecordingTelemetrySink, SteppingTelemetryClock, TelemetryEvent,
            TelemetrySession,
        };
        use std::time::Duration;

        struct OutOfOrderBatchProvider {
            call_order: Mutex<Vec<String>>,
            fail_on_canon: Option<String>,
        }

        impl StructuredModelProvider for OutOfOrderBatchProvider {
            fn generate_structured(
                &self,
                request: &GenerationRequest,
            ) -> Result<GenerationResponse, ProviderError> {
                let mut batch_canons = Vec::new();
                for line in request.user_prompt.lines() {
                    if let Some(rest) = line.strip_prefix("### Canon: `")
                        && let Some(canon_ref) = rest.strip_suffix('`')
                    {
                        batch_canons.push(canon_ref.to_string());
                    }
                }

                // Force early batches to sleep longer so later batches complete first out of order
                if batch_canons.iter().any(|c| c.contains("rule-000")) {
                    std::thread::sleep(Duration::from_millis(35));
                } else if batch_canons.iter().any(|c| c.contains("rule-002")) {
                    std::thread::sleep(Duration::from_millis(15));
                }

                if let Some(fail_target) = &self.fail_on_canon
                    && batch_canons.iter().any(|c| c == fail_target)
                {
                    return Err(ProviderError::Timeout {
                        model_specifier: request.model_specifier.clone(),
                        timeout_ms: request.timeout_ms,
                    });
                }

                self.call_order
                    .lock()
                    .expect("lock call_order")
                    .push(batch_canons.first().cloned().unwrap_or_default());

                let mut cases = Vec::new();
                for canon_ref in batch_canons.into_iter().rev() {
                    let is_even = canon_ref
                        .chars()
                        .rfind(|c| c.is_ascii_digit())
                        .is_some_and(|d| (d as u8).is_multiple_of(2));
                    let score = if is_even { 0.85 } else { 0.15 };
                    cases.push(json!({
                        "canon_path": canon_ref,
                        "colorability_summary": format!("Assessed {canon_ref}."),
                        "colorability_score": score
                    }));
                }

                Ok(GenerationResponse {
                    payload: json!({ "cases": cases }),
                    thoughts: vec!["Batch thought chunk.".to_string()],
                    usage: TokenUsage {
                        prompt_tokens: 100,
                        completion_tokens: 25,
                        reasoning_tokens: 0,
                        total_tokens: 125,
                    },
                    latency_ms: 15,
                    resolved_model: "gemini-2.5-flash-lite".to_string(),
                })
            }
        }

        #[test]
        fn test_partition_candidate_canons_handles_empty_unbounded_and_remainder_slices() {
            assert!(partition_candidate_canons(&[], Some(5)).is_empty());
            let five: Vec<String> = (0..5).map(|i| format!("c{i}")).collect();
            assert_eq!(partition_candidate_canons(&five, None), vec![five.clone()]);
            assert_eq!(
                partition_candidate_canons(&five, Some(10)),
                vec![five.clone()]
            );
            assert_eq!(
                partition_candidate_canons(&five, Some(2)),
                vec![
                    vec!["c0".to_string(), "c1".to_string()],
                    vec!["c2".to_string(), "c3".to_string()],
                    vec!["c4".to_string()],
                ]
            );
        }

        #[test]
        fn test_out_of_order_concurrent_batches_fold_deterministically_and_aggregate_telemetry() {
            let mut ws = WorkspaceSnapshot::new();
            let mut candidates = Vec::new();
            for i in 0..5 {
                let path = format!(".canons/pack/rule-{i:03}.md");
                ws = ws.with_canon_markdown(
                    &format!("# Rule {i}\n\nRule {i} MUST hold.\n"),
                    &ParseCanonOptions::with_source_ref(path.clone()),
                );
                candidates.push(path);
            }
            let caseload = sample_filing_caseload(candidates);

            // 1. Unbounded single-turn baseline
            let single_provider = OutOfOrderBatchProvider {
                call_order: Mutex::new(Vec::new()),
                fail_on_canon: None,
            };
            let single_res = execute_docket(
                &DocketOptions::default(),
                caseload.clone(),
                &ws,
                &single_provider,
            )
            .expect("single-turn baseline");

            // 2. Batched (batch size 2 => 3 batches: [0,1], [2,3], [4]) with concurrency 3 + telemetry
            let batched_provider = OutOfOrderBatchProvider {
                call_order: Mutex::new(Vec::new()),
                fail_on_canon: None,
            };
            let sink = RecordingTelemetrySink::new();
            let clock = SteppingTelemetryClock::new(1_767_225_600_000, 10);
            let session = TelemetrySession::new(&sink, &clock);

            let batched_res = session
                .run_stage(PipelineStage::Docket, |scope| {
                    let instrumented = scope.instrument_provider(&batched_provider);
                    execute_docket(
                        &DocketOptions {
                            max_canons_per_call: Some(2),
                            max_concurrent_calls: Some(3),
                            ..DocketOptions::default()
                        },
                        caseload.clone(),
                        &ws,
                        &instrumented,
                    )
                })
                .expect("concurrent batched docket");

            // Completion order was out-of-order (batch 2 finished before batch 0), yet Caseload is byte-for-byte identical
            let completion_order = batched_provider.call_order.lock().expect("lock").clone();
            assert_eq!(completion_order.len(), 3);
            assert_ne!(
                completion_order[0], ".canons/pack/rule-000.md",
                "batch 0 slept longest and must not finish first"
            );
            assert_eq!(batched_res, single_res);
            assert_eq!(
                serde_json::to_string(&batched_res).expect("json"),
                serde_json::to_string(&single_res).expect("json")
            );

            // Verify telemetry accumulated all 3 batch calls (3 * 125 = 375 total_tokens),
            // 3 Thought events, and monotonic 1/3..3/3 StageProgress events despite out-of-order completion.
            let events = sink.events();
            let progress_counts: Vec<(usize, usize)> = events
                .iter()
                .filter_map(|e| match e {
                    TelemetryEvent::StageProgress {
                        stage: PipelineStage::Docket,
                        completed,
                        total,
                        label,
                    } => {
                        assert!(!label.is_empty());
                        Some((*completed, *total))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(progress_counts, vec![(1, 3), (2, 3), (3, 3)]);
            let thought_count = events
                .iter()
                .filter(|e| matches!(e, TelemetryEvent::Thought { .. }))
                .count();
            assert_eq!(thought_count, 3);
            let stage_finish = events
                .iter()
                .find_map(|e| match e {
                    TelemetryEvent::StageFinish {
                        stage: PipelineStage::Docket,
                        usage,
                        ..
                    } => *usage,
                    _ => None,
                })
                .expect("Docket StageFinish usage");
            assert_eq!(
                stage_finish,
                TokenUsage {
                    prompt_tokens: 300,
                    completion_tokens: 75,
                    reasoning_tokens: 0,
                    total_tokens: 375,
                }
            );
        }

        #[test]
        fn test_concurrent_batch_error_annotates_batch_coordinates_on_timeout_and_decode_failure() {
            let mut ws = WorkspaceSnapshot::new();
            let mut candidates = Vec::new();
            for i in 0..4 {
                let path = format!(".canons/pack/rule-{i:03}.md");
                ws = ws.with_canon_markdown(
                    &format!("# Rule {i}\n\nRule {i} MUST hold.\n"),
                    &ParseCanonOptions::with_source_ref(path.clone()),
                );
                candidates.push(path);
            }
            let caseload = sample_filing_caseload(candidates);
            let failing_provider = OutOfOrderBatchProvider {
                call_order: Mutex::new(Vec::new()),
                fail_on_canon: Some(".canons/pack/rule-002.md".to_string()),
            };

            let err = execute_docket(
                &DocketOptions {
                    max_canons_per_call: Some(2),
                    max_concurrent_calls: Some(2),
                    ..DocketOptions::default()
                },
                caseload.clone(),
                &ws,
                &failing_provider,
            )
            .expect_err("failing second batch must fail execute_docket");
            assert!(matches!(
                err,
                DocketError::BatchProvider {
                    batch_index: 2,
                    batch_count: 2,
                    source: ProviderError::Timeout { .. },
                }
            ));
            assert!(
                err.to_string()
                    .contains("Docket screening failed (batch 2/2):"),
                "expected contextual batch 2/2 prefix, got: {err}"
            );

            // Multi-batch schema decode failure also surfaces batch k/N (including sequential concurrency=1)
            let malformed_provider = RecordingMockProvider::with_payload(json!({ "cases": [] }));
            let decode_err = execute_docket(
                &DocketOptions {
                    max_canons_per_call: Some(2),
                    max_concurrent_calls: Some(1),
                    ..DocketOptions::default()
                },
                caseload,
                &ws,
                &malformed_provider,
            )
            .expect_err("malformed multi-batch payload must fail");
            assert!(matches!(
                decode_err,
                DocketError::BatchProvider {
                    batch_index: 1,
                    batch_count: 2,
                    source: ProviderError::SchemaDecode { .. },
                }
            ));
            assert!(decode_err.to_string().contains("(batch 1/2):"));
        }
    }
}
