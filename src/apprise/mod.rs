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

//! Core domain execution engine for the `apprise` Caseload DAG stage (`docs/architecture/nodes/apprise.md`).

mod decision;

use crate::caseload::{
    Caseload, CaseloadApprisal, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL,
    DEFAULT_LOCAL_DISCRIMINATOR_MODEL, DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL, HeuristicNode,
    normalize_unit_score,
};
use crate::configure::is_discriminative_model;
use crate::discover::WorkspaceSnapshot;
use crate::provider::{DEFAULT_GENERATION_TIMEOUT_MS, ProviderError, StructuredModelProvider};
use std::fmt;

/// Default prospective applicability score threshold for `apprise` (`docs/architecture/nodes/apprise.md` §4.3).
pub const DEFAULT_APPRISE_THRESHOLD: f64 = 0.5;

/// Default maximum concurrent in-flight `apprise` model calls when executing multiple batches.
pub const DEFAULT_APPRISE_MAX_CONCURRENT_CALLS: usize = 4;

/// Environment variable name for overriding `--apprise-threshold` (`docs/architecture/nodes/apprise.md` §5).
pub const ENV_APPRISE_THRESHOLD: &str = "CANON_CLERK_APPRISE_THRESHOLD";

/// Environment variable name for overriding `--apprise-batch-size` (`docs/architecture/nodes/apprise.md` §5).
pub const ENV_APPRISE_BATCH_SIZE: &str = "CANON_CLERK_APPRISE_BATCH_SIZE";

/// Environment variable name for overriding `--apprise-concurrency` (`docs/architecture/nodes/apprise.md` §5).
pub const ENV_APPRISE_CONCURRENCY: &str = "CANON_CLERK_APPRISE_CONCURRENCY";

/// Domain input parameters for executing the `apprise` pipeline stage.
#[derive(Debug, Clone, PartialEq)]
pub struct AppriseOptions {
    /// Prospective applicability score threshold in `[0.0, 1.0]` (defaults to [`DEFAULT_APPRISE_THRESHOLD`]).
    pub threshold: f64,
    /// Per-request timeout ceiling in milliseconds (defaults to [`DEFAULT_GENERATION_TIMEOUT_MS`]).
    pub timeout_ms: u64,
    /// Optional maximum candidate canons evaluated per `apprise` model call (`>= 1`; `None` defers to the model's `DecisionBudgetProfile`).
    pub max_canons_per_call: Option<usize>,
    /// Optional maximum concurrent in-flight `apprise` model calls when batched (`>= 1`; defaults to [`DEFAULT_APPRISE_MAX_CONCURRENT_CALLS`]).
    pub max_concurrent_calls: Option<usize>,
}

impl Default for AppriseOptions {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_APPRISE_THRESHOLD,
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
            max_canons_per_call: None,
            max_concurrent_calls: None,
        }
    }
}

/// Domain errors produced during `apprise` execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppriseError {
    /// Configured threshold is non-finite or outside `[0.0, 1.0]`.
    InvalidThreshold { detail: String },
    /// Configured `max_canons_per_call` is zero (`0`).
    InvalidBatchSize { detail: String },
    /// Configured `max_concurrent_calls` is zero (`0`).
    InvalidConcurrency { detail: String },
    /// Prospective design `intent` is missing or blank on `.intake`.
    MissingIntent,
    /// Prerequisite `.discovery` namespace is missing from the incoming `Caseload`.
    MissingDiscovery,
    /// Upstream `.validation` recorded one or more candidate canon lint errors.
    ValidationFailed,
    /// Prerequisite `.config` namespace is missing when non-empty candidate canons require model screening.
    MissingConfig,
    /// A generative model was configured on `apprise` instead of a discriminative `Noul` model.
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

impl AppriseError {
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

impl fmt::Display for AppriseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidThreshold { detail } => write!(
                f,
                "Invalid apprise threshold: {detail}.\n  \
                 Hint: Pass a finite threshold in `[0.0, 1.0]` via `--apprise-threshold <FLOAT>` \
                 or `{ENV_APPRISE_THRESHOLD}` (default: `{DEFAULT_APPRISE_THRESHOLD}`)."
            ),
            Self::InvalidBatchSize { detail } => write!(
                f,
                "Invalid apprise batch size: {detail}.\n  \
                 Hint: Pass a positive integer (`>= 1`) via `--apprise-batch-size <COUNT>` \
                 or `{ENV_APPRISE_BATCH_SIZE}`."
            ),
            Self::InvalidConcurrency { detail } => write!(
                f,
                "Invalid apprise concurrency limit: {detail}.\n  \
                 Hint: Pass a positive integer (`>= 1`) via `--apprise-concurrency <COUNT>` \
                 or `{ENV_APPRISE_CONCURRENCY}` (default: `{DEFAULT_APPRISE_MAX_CONCURRENT_CALLS}`)."
            ),
            Self::MissingIntent => write!(
                f,
                "Prospective apprisal requires a non-empty architectural design intent.\n  \
                 Hint: Pass `--intent <TEXT>` alongside target paths or canon scope, \
                 or provide an upstream caseload (`--caseload <PATH|->`) with `.intake.intent` populated."
            ),
            Self::MissingDiscovery => write!(
                f,
                "Prospective apprisal requires a resolved target or canon discovery scope.\n  \
                 Hint: Provide prospective target paths (`src/...`, `-`, `--diff <PATH|->`, `--all-targets`), \
                 canon scope (`--all-canons`, `--canon <PATH|GLOB>`), or `--caseload <PATH|->`."
            ),
            Self::ValidationFailed => write!(
                f,
                "Cannot execute `apprise` because one or more candidate canons failed static validation.\n  \
                 Hint: Run `canon-clerk validate` to inspect and repair malformed candidate canons."
            ),
            Self::MissingConfig => write!(
                f,
                "Cannot execute `apprise` screening without a resolved `.config` namespace on the Caseload.\n  \
                 Hint: Run `canon-clerk configure` first or invoke `canon-clerk apprise` directly to resolve configuration JIT."
            ),
            Self::UnsupportedGenerativeModel { specifier } => write!(
                f,
                "Generative model `{specifier}` cannot be used for `apprise` \
                 (`apprise` requires a discriminative `Noul` decision model).\n  \
                 Hint: Configure a discriminator on `--apprise-model` such as `{DEFAULT_GOOGLE_DISCRIMINATOR_MODEL}` \
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
                "Apprise screening failed (batch {batch_index}/{batch_count}): {source}"
            ),
            Self::Provider(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for AppriseError {}

impl From<ProviderError> for AppriseError {
    fn from(err: ProviderError) -> Self {
        Self::Provider(err)
    }
}

fn validate_batching_options(options: &AppriseOptions) -> Result<(), AppriseError> {
    if options.max_canons_per_call == Some(0) {
        return Err(AppriseError::InvalidBatchSize {
            detail: "max_canons_per_call must be at least 1, got 0".to_string(),
        });
    }
    if options.max_concurrent_calls == Some(0) {
        return Err(AppriseError::InvalidConcurrency {
            detail: "max_concurrent_calls must be at least 1, got 0".to_string(),
        });
    }
    Ok(())
}

/// Executes the pure `apprise` domain stage against an in-memory [`WorkspaceSnapshot`] and
/// hexagonal [`StructuredModelProvider`] (`docs/architecture/nodes/apprise.md` §4).
pub fn execute_apprise(
    options: &AppriseOptions,
    caseload: Caseload,
    workspace: &WorkspaceSnapshot,
    provider: &dyn StructuredModelProvider,
) -> Result<Caseload, AppriseError> {
    let threshold = normalize_unit_score(options.threshold)
        .map_err(|detail| AppriseError::InvalidThreshold { detail })?;
    validate_batching_options(options)?;

    if caseload.apprisal.is_some() {
        return Ok(caseload);
    }

    let intake = caseload
        .intake
        .as_ref()
        .ok_or(AppriseError::MissingIntent)?;
    if !intake
        .intent
        .as_deref()
        .is_some_and(|text| !text.trim().is_empty())
    {
        return Err(AppriseError::MissingIntent);
    }

    let discovery = caseload
        .discovery
        .as_ref()
        .ok_or(AppriseError::MissingDiscovery)?;

    if discovery.candidate_canons.is_empty() {
        return Ok(caseload.with_apprisal(CaseloadApprisal::default()));
    }

    if caseload
        .validation
        .as_ref()
        .is_some_and(|val| val.has_errors)
    {
        return Err(AppriseError::ValidationFailed);
    }

    let node_cfg = caseload
        .config
        .as_ref()
        .and_then(|c| c.node(HeuristicNode::Apprise))
        .ok_or(AppriseError::MissingConfig)?;
    if !is_discriminative_model(&node_cfg.model) {
        return Err(AppriseError::UnsupportedGenerativeModel {
            specifier: node_cfg.model.clone(),
        });
    }

    decision::execute_apprise_decisions(options, caseload, threshold, workspace, provider)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::caseload::{
        ApprisalStatus, CaseloadConfig, CaseloadDiscovery, CaseloadIntake, CaseloadValidation,
        DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, DEFAULT_GOOGLE_GENERATOR_MODEL,
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

        fn recorded_requests(&self) -> Vec<DecisionRequest> {
            self.requests.lock().expect("lock requests").clone()
        }
    }

    impl StructuredModelProvider for RecordingDecisionProvider {
        fn generate_structured(
            &self,
            _request: &GenerationRequest,
        ) -> Result<GenerationResponse, ProviderError> {
            panic!("apprise must never invoke generate_structured")
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
                latency_ms: 95,
                usage: TokenUsage {
                    prompt_tokens: 410,
                    completion_tokens: 0,
                    reasoning_tokens: 0,
                    total_tokens: 410,
                },
            })
        }
    }

    fn fixture_snapshot() -> WorkspaceSnapshot {
        let cache_canon = "---\ntriggers:\n  - \"src/auth/**\"\n---\nCaching layers MUST configure explicit TTL expiration.\n";
        let db_canon = "---\ntriggers:\n  - \"src/auth/**\"\n---\nDatabase schema migrations MUST provide rollback scripts.\n";
        WorkspaceSnapshot::new()
            .with_file("src/auth/provider.rs")
            .with_canon_markdown(
                cache_canon,
                &ParseCanonOptions::with_source_ref(".canons/auth/cache-expiry.md"),
            )
            .with_canon_markdown(
                db_canon,
                &ParseCanonOptions::with_source_ref(".canons/auth/db-rollback.md"),
            )
    }

    fn fixture_caseload(candidate_canons: Vec<String>) -> Caseload {
        Caseload::new()
            .with_intake(CaseloadIntake {
                intent: Some(
                    "Refactor auth provider dispatch with in-memory TTL cache".to_string(),
                ),
                target_paths: vec!["src/auth".to_string()],
                ..CaseloadIntake::default()
            })
            .with_discovery(CaseloadDiscovery {
                target_files: vec!["src/auth/provider.rs".to_string()],
                candidate_canons,
                ..CaseloadDiscovery::default()
            })
            .with_validation(CaseloadValidation::default())
            .with_config(
                CaseloadConfig::default()
                    .with_node_model(HeuristicNode::Apprise, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL),
            )
    }

    #[test]
    fn test_empty_candidate_canons_short_circuits_with_zero_model_calls_and_no_config_needed() {
        let caseload = Caseload::new()
            .with_intake(CaseloadIntake {
                intent: Some("Update docs".to_string()),
                target_paths: vec!["docs/faq.md".to_string()],
                ..CaseloadIntake::default()
            })
            .with_discovery(CaseloadDiscovery {
                candidate_canons: Vec::new(),
                ..CaseloadDiscovery::default()
            });
        let provider = RecordingDecisionProvider::with_error(
            ProviderError::missing_api_key_for_slug("google"),
        );

        let enriched = execute_apprise(
            &AppriseOptions::default(),
            caseload,
            &WorkspaceSnapshot::new(),
            &provider,
        )
        .expect("zero candidate canons must short-circuit cleanly");

        assert!(provider.recorded_requests().is_empty());
        assert_eq!(
            enriched.apprisal,
            Some(CaseloadApprisal {
                assessments: BTreeMap::new()
            })
        );
    }

    #[test]
    fn test_rejects_prerequisite_and_generator_model_guards() {
        let snapshot = fixture_snapshot();
        let provider = RecordingDecisionProvider::with_scores(BTreeMap::new());

        let bad_threshold_err = execute_apprise(
            &AppriseOptions {
                threshold: 1.25,
                ..AppriseOptions::default()
            },
            fixture_caseload(Vec::new()),
            &snapshot,
            &provider,
        )
        .expect_err("threshold > 1.0 must fail");
        assert!(matches!(
            bad_threshold_err,
            AppriseError::InvalidThreshold { .. }
        ));

        let no_intent_err = execute_apprise(
            &AppriseOptions::default(),
            Caseload::new().with_discovery(CaseloadDiscovery::default()),
            &snapshot,
            &provider,
        )
        .expect_err("missing intent must fail");
        assert_eq!(no_intent_err, AppriseError::MissingIntent);

        let no_disc_err = execute_apprise(
            &AppriseOptions::default(),
            Caseload::new().with_intake(CaseloadIntake {
                intent: Some("Add cache".to_string()),
                ..CaseloadIntake::default()
            }),
            &snapshot,
            &provider,
        )
        .expect_err("missing discovery must fail");
        assert_eq!(no_disc_err, AppriseError::MissingDiscovery);

        let mut gen_cfg = fixture_caseload(vec![".canons/auth/cache-expiry.md".to_string()]);
        gen_cfg.config = Some(
            CaseloadConfig::default()
                .with_node_model(HeuristicNode::Apprise, DEFAULT_GOOGLE_GENERATOR_MODEL),
        );
        let gen_err = execute_apprise(&AppriseOptions::default(), gen_cfg, &snapshot, &provider)
            .expect_err("generator model on apprise must be rejected");
        assert!(matches!(
            gen_err,
            AppriseError::UnsupportedGenerativeModel { .. }
        ));
        let msg = gen_err.to_string();
        assert!(msg.contains("google:gemini-decision-1"));
        assert!(msg.contains("typesafe:jev-latest"));
        assert!(msg.contains("local:kev-4b"));
    }

    #[test]
    fn test_discriminative_apprise_screens_and_gates_threshold() {
        let snapshot = fixture_snapshot();
        let cache = ".canons/auth/cache-expiry.md";
        let db = ".canons/auth/db-rollback.md";
        let caseload = fixture_caseload(vec![cache.to_string(), db.to_string()]);
        let provider = RecordingDecisionProvider::with_scores(BTreeMap::from([
            (cache.to_string(), 0.91),
            (db.to_string(), 0.09),
        ]));

        let out = execute_apprise(&AppriseOptions::default(), caseload, &snapshot, &provider)
            .expect("apprise succeeds");
        let apprisal = out.apprisal.expect("apprisal populated");
        assert_eq!(
            apprisal.assessments[cache].apprisal_status,
            ApprisalStatus::Applicable
        );
        assert_eq!(
            apprisal.assessments[db].apprisal_status,
            ApprisalStatus::Dismissed
        );
    }

    #[test]
    fn test_rejects_zero_batch_size_and_zero_concurrency_with_hints() {
        let snapshot = fixture_snapshot();
        let provider = RecordingDecisionProvider::with_scores(BTreeMap::new());
        let caseload = fixture_caseload(vec![".canons/auth/cache-expiry.md".to_string()]);

        let batch_err = execute_apprise(
            &AppriseOptions {
                max_canons_per_call: Some(0),
                ..AppriseOptions::default()
            },
            caseload.clone(),
            &snapshot,
            &provider,
        )
        .expect_err("zero batch size must fail");
        assert!(matches!(batch_err, AppriseError::InvalidBatchSize { .. }));
        assert!(batch_err.to_string().contains("--apprise-batch-size"));

        let conc_err = execute_apprise(
            &AppriseOptions {
                max_concurrent_calls: Some(0),
                ..AppriseOptions::default()
            },
            caseload,
            &snapshot,
            &provider,
        )
        .expect_err("zero concurrency must fail");
        assert!(matches!(conc_err, AppriseError::InvalidConcurrency { .. }));
        assert!(conc_err.to_string().contains("--apprise-concurrency"));
    }
}
