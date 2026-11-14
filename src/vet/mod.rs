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

//! Core domain execution engine for the `vet` Caseload DAG stage (`docs/architecture/nodes/vet.md`).

mod decision;
mod structured;

use crate::audit::build_audit_case_user_prompt;
use crate::caseload::{Caseload, CaseloadVetting, TYPESAFE_PROVIDER, normalize_vetting_score};
use crate::configure::model_provider;
use crate::discover::WorkspaceSnapshot;
use crate::docket::ENV_MAX_CONCURRENT_CALLS;
use crate::provider::{DEFAULT_GENERATION_TIMEOUT_MS, ProviderError, StructuredModelProvider};
use std::fmt;

/// Default high-confidence prima facie compliance threshold for `vet` (`docs/architecture/nodes/vet.md` §3).
pub const DEFAULT_VET_THRESHOLD: f64 = 0.80;

/// Default maximum concurrent in-flight per-case vetting calls during `vet`.
pub const DEFAULT_VET_MAX_CONCURRENT_CALLS: usize = 4;

/// Environment variable name for overriding `--vet-threshold` (`docs/architecture/nodes/vet.md` §5).
pub const ENV_VET_THRESHOLD: &str = "CANON_CLERK_VET_THRESHOLD";

/// Environment variable name for overriding `--vet-concurrency` (`docs/architecture/nodes/vet.md` §5).
pub const ENV_VET_CONCURRENCY: &str = "CANON_CLERK_VET_CONCURRENCY";

/// Domain input parameters for executing the `vet` pipeline stage.
#[derive(Debug, Clone, PartialEq)]
pub struct VetOptions {
    /// Prima facie compliance score threshold in `[0.0, 1.0]` (defaults to [`DEFAULT_VET_THRESHOLD`]).
    pub threshold: f64,
    /// Per-request timeout ceiling in milliseconds (defaults to [`DEFAULT_GENERATION_TIMEOUT_MS`]).
    pub timeout_ms: u64,
    /// Optional maximum concurrent in-flight per-case vetting calls (`>= 1`; defaults to [`DEFAULT_VET_MAX_CONCURRENT_CALLS`]).
    pub max_concurrent_calls: Option<usize>,
    /// When `true` (cascading inside `canon-clerk audit`), cases exceeding the discriminative context
    /// window deterministically classify as `contested` so they escalate to `auditor_model`.
    pub allow_oversized_contest_fallback: bool,
}

impl Default for VetOptions {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_VET_THRESHOLD,
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
            max_concurrent_calls: None,
            allow_oversized_contest_fallback: false,
        }
    }
}

/// Domain errors produced during `vet` execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VetError {
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
    /// Direct prerequisite `.evidence` namespace is missing from the incoming `Caseload`.
    MissingEvidence,
    /// Prerequisite `.config` namespace is missing when active cases require vetting.
    MissingConfig,
    /// An active canon listed in `.evidence.active_cases` is absent from the workspace snapshot.
    MissingActiveCanon { canon_ref: String },
    /// Model provider returned a transport, HTTP, block, or schema decoding error.
    Provider(ProviderError),
}

impl fmt::Display for VetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidThreshold { detail } => write!(
                f,
                "Invalid vet threshold: {detail}.\n  \
                 Hint: Pass a finite threshold in `[0.0, 1.0]` via `--vet-threshold <FLOAT>` \
                 or `{ENV_VET_THRESHOLD}` (default: `{DEFAULT_VET_THRESHOLD:.2}`)."
            ),
            Self::InvalidConcurrency { detail } => write!(
                f,
                "Invalid vet concurrency limit: {detail}.\n  \
                 Hint: Pass a positive integer (`>= 1`) via `--vet-concurrency <COUNT>` \
                 or `{ENV_VET_CONCURRENCY}` (default: `{DEFAULT_VET_MAX_CONCURRENT_CALLS}`)."
            ),
            Self::MissingIntake => write!(
                f,
                "Prima facie compliance vetting (`vet`) requires an underlying filing in `.intake` (Case-or-Controversy invariant).\n  \
                 Hint: Pipe a diff (`--diff -`), specify target paths (`src/...`, `-`, `--all-targets`), \
                 or pass `--caseload <PATH|->` with `.intake` populated."
            ),
            Self::MissingDiscovery => write!(
                f,
                "Prima facie compliance vetting (`vet`) requires a resolved `.discovery` namespace.\n  \
                 Hint: Run `canon-clerk discover` first or invoke `canon-clerk vet` with a filing source."
            ),
            Self::ValidationFailed => write!(
                f,
                "Cannot execute `vet` because one or more candidate canons failed static validation.\n  \
                 Hint: Run `canon-clerk validate` to inspect and repair malformed candidate canons."
            ),
            Self::MissingEvidence => write!(
                f,
                "Prima facie compliance vetting (`vet`) requires a resolved `.evidence` namespace from `admit`.\n  \
                 Hint: Run `canon-clerk admit` first or invoke `canon-clerk vet` directly to execute screening upstream."
            ),
            Self::MissingConfig => write!(
                f,
                "Cannot execute `vet` without a resolved `.config` namespace on the Caseload.\n  \
                 Hint: Run `canon-clerk configure` first or invoke `canon-clerk vet` directly to resolve configuration JIT."
            ),
            Self::MissingActiveCanon { canon_ref } => write!(
                f,
                "Active case canon `{canon_ref}` referenced in `.evidence.active_cases` was not found in the workspace snapshot.\n  \
                 Hint: Re-run `canon-clerk admit` or `canon-clerk vet` against the current workspace."
            ),
            Self::Provider(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for VetError {}

impl From<ProviderError> for VetError {
    fn from(err: ProviderError) -> Self {
        Self::Provider(err)
    }
}

pub(super) struct PreparedVettingCase {
    pub(super) canon_ref: String,
    pub(super) user_prompt: String,
}

fn validate_vet_options(options: &VetOptions) -> Result<f64, VetError> {
    let threshold = normalize_vetting_score(options.threshold)
        .map_err(|detail| VetError::InvalidThreshold { detail })?;
    if options.max_concurrent_calls == Some(0) {
        return Err(VetError::InvalidConcurrency {
            detail: format!(
                "max_concurrent_calls must be at least 1, got 0 (see `{ENV_VET_CONCURRENCY}` / `{ENV_MAX_CONCURRENT_CALLS}`)"
            ),
        });
    }
    Ok(threshold)
}

/// Executes the pure `vet` domain stage against an in-memory [`WorkspaceSnapshot`] and
/// hexagonal [`StructuredModelProvider`] (`docs/architecture/nodes/vet.md` §4).
pub fn execute_vet(
    options: &VetOptions,
    caseload: Caseload,
    workspace: &WorkspaceSnapshot,
    provider: &dyn StructuredModelProvider,
) -> Result<Caseload, VetError> {
    let threshold = validate_vet_options(options)?;

    if caseload.vetting.is_some() {
        return Ok(caseload);
    }

    let intake = caseload.intake.as_ref().ok_or(VetError::MissingIntake)?;
    let discovery = caseload
        .discovery
        .as_ref()
        .ok_or(VetError::MissingDiscovery)?;

    if caseload
        .validation
        .as_ref()
        .is_some_and(|val| val.has_errors)
    {
        return Err(VetError::ValidationFailed);
    }

    let evidence = caseload
        .evidence
        .as_ref()
        .ok_or(VetError::MissingEvidence)?;
    if evidence.active_cases.is_empty() {
        return Ok(caseload.with_vetting(CaseloadVetting::default()));
    }

    let mut prepared_cases = Vec::new();
    for canon_ref in &evidence.active_cases {
        let canon =
            workspace
                .canons
                .get(canon_ref)
                .ok_or_else(|| VetError::MissingActiveCanon {
                    canon_ref: canon_ref.clone(),
                })?;
        let admitted = evidence.admitted_exhibits_for(canon_ref);
        if !admitted.is_empty() {
            let (user_prompt, _) = build_audit_case_user_prompt(
                canon_ref, canon, &admitted, intake, discovery, workspace,
            );
            prepared_cases.push(PreparedVettingCase {
                canon_ref: canon_ref.clone(),
                user_prompt,
            });
        }
    }

    if prepared_cases.is_empty() {
        return Ok(caseload.with_vetting(CaseloadVetting::default()));
    }

    let config = caseload.config.as_ref().ok_or(VetError::MissingConfig)?;
    let concurrency_limit = options
        .max_concurrent_calls
        .unwrap_or(DEFAULT_VET_MAX_CONCURRENT_CALLS);

    let cases = if model_provider(&config.screener_model) == TYPESAFE_PROVIDER {
        decision::execute_discriminative_vetting(
            &prepared_cases,
            &decision::DiscriminativeVetParams {
                screener_model: &config.screener_model,
                threshold,
                concurrency_limit,
                timeout_ms: options.timeout_ms,
                allow_oversized_contest_fallback: options.allow_oversized_contest_fallback,
            },
            provider,
        )?
    } else {
        structured::execute_generative_vetting(
            &prepared_cases,
            &structured::GenerativeVetParams {
                screener_model: &config.screener_model,
                threshold,
                concurrency_limit,
                timeout_ms: options.timeout_ms,
            },
            provider,
        )?
    };

    Ok(caseload.with_vetting(CaseloadVetting::from_cases(cases)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::caseload::{
        AdmissibilityStatus, CanonDocketAssessment, CaseloadConfig, CaseloadDiscovery,
        CaseloadDocket, CaseloadEvidence, CaseloadIntake, CaseloadValidation, ColorabilityStatus,
        DEFAULT_AUDITOR_MODEL, DEFAULT_SCREENER_MODEL, DiscoveryMode, ExhibitAssessment,
        FileArtifact, FileChangeStatus, VettingStatus,
    };
    use crate::configure::DEFAULT_JEV_MODEL;
    use crate::provider::{
        DecisionRequest, DecisionResponse, GenerationRequest, GenerationResponse, TokenUsage,
    };
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    struct MockVetProvider {
        decision_scores: BTreeMap<String, f64>,
        structured_routes: BTreeMap<String, Value>,
        decision_requests: Mutex<Vec<DecisionRequest>>,
        structured_requests: Mutex<Vec<GenerationRequest>>,
    }

    impl MockVetProvider {
        fn empty() -> Self {
            Self {
                decision_scores: BTreeMap::new(),
                structured_routes: BTreeMap::new(),
                decision_requests: Mutex::new(Vec::new()),
                structured_requests: Mutex::new(Vec::new()),
            }
        }
    }

    impl StructuredModelProvider for MockVetProvider {
        fn generate_structured(
            &self,
            request: &GenerationRequest,
        ) -> Result<GenerationResponse, ProviderError> {
            self.structured_requests
                .lock()
                .expect("lock")
                .push(request.clone());
            for (needle, payload) in &self.structured_routes {
                if request.user_prompt.contains(needle) {
                    return Ok(GenerationResponse {
                        payload: payload.clone(),
                        resolved_model: "gemini-2.5-flash-lite".to_string(),
                        latency_ms: 110,
                        usage: TokenUsage {
                            prompt_tokens: 200,
                            completion_tokens: 30,
                            reasoning_tokens: 0,
                            total_tokens: 230,
                        },
                        thoughts: Vec::new(),
                    });
                }
            }
            Err(ProviderError::SchemaDecode {
                model_specifier: request.model_specifier.clone(),
                message: "no route".to_string(),
            })
        }

        fn evaluate_decisions(
            &self,
            request: &DecisionRequest,
        ) -> Result<DecisionResponse, ProviderError> {
            self.decision_requests
                .lock()
                .expect("lock")
                .push(request.clone());
            let score = self
                .decision_scores
                .iter()
                .find(|(k, _)| request.state.contains(k.as_str()))
                .map(|(_, s)| *s)
                .unwrap_or(0.95);
            Ok(DecisionResponse {
                scores: BTreeMap::from([(decision::COMPLIANCE_QUESTION_KEY.to_string(), score)]),
                resolved_model: "jev-1.14.0".to_string(),
                latency_ms: 85,
                usage: TokenUsage {
                    prompt_tokens: 180,
                    completion_tokens: 0,
                    reasoning_tokens: 0,
                    total_tokens: 180,
                },
            })
        }
    }

    fn sample_fixture(screener_model: &str) -> (WorkspaceSnapshot, Caseload) {
        let clean_ref = ".canons/cli/clean-rule.md";
        let contested_ref = ".canons/cli/contested-rule.md";
        let ws = WorkspaceSnapshot::new()
            .with_canon_markdown(
                "---\ntriggers:\n  - \"src/cli/**\"\ntags:\n  - cli\n---\nCLI flags MUST use kebab-case.\n",
                &ParseCanonOptions::with_source_ref(clean_ref),
            )
            .with_canon_markdown(
                "---\ntriggers:\n  - \"src/cli/**\"\ntags:\n  - cli\n---\nCLI errors MUST suggest remediations.\n",
                &ParseCanonOptions::with_source_ref(contested_ref),
            );
        let diffs = BTreeMap::from([(
            "src/cli/app.rs".to_string(),
            FileArtifact {
                path: "src/cli/app.rs".to_string(),
                previous_path: None,
                status: FileChangeStatus::Modified,
                lines_added: 1,
                lines_deleted: 0,
                patch: Some("@@ -1,1 +1,2 @@\n+// flag\n".to_string()),
                patch_omission_reason: None,
                content: None,
                content_omission_reason: None,
            },
        )]);
        let docket_cases = BTreeMap::from([
            (
                clean_ref.to_string(),
                CanonDocketAssessment {
                    colorability_summary: "ok".to_string(),
                    colorability_score: 0.9,
                    colorability_status: ColorabilityStatus::Docketed,
                },
            ),
            (
                contested_ref.to_string(),
                CanonDocketAssessment {
                    colorability_summary: "ok".to_string(),
                    colorability_score: 0.9,
                    colorability_status: ColorabilityStatus::Docketed,
                },
            ),
        ]);
        let item = ExhibitAssessment {
            file_path: "src/cli/app.rs".to_string(),
            admissibility_summary: "relevant".to_string(),
            admissibility_score: 0.92,
            admissibility_status: AdmissibilityStatus::Admitted,
        };
        let exhibits = BTreeMap::from([
            (clean_ref.to_string(), vec![item.clone()]),
            (contested_ref.to_string(), vec![item]),
        ]);
        let caseload = Caseload::new()
            .with_intake(CaseloadIntake {
                diffs,
                ..CaseloadIntake::default()
            })
            .with_discovery(CaseloadDiscovery {
                mode: DiscoveryMode::Triggered,
                candidate_canons: vec![clean_ref.to_string(), contested_ref.to_string()],
                ..CaseloadDiscovery::default()
            })
            .with_validation(CaseloadValidation::default())
            .with_config(CaseloadConfig {
                screener_model: screener_model.to_string(),
                auditor_model: DEFAULT_AUDITOR_MODEL.to_string(),
                reasoning_budget: Some(4096),
            })
            .with_docket(CaseloadDocket::from_cases(docket_cases))
            .with_evidence(CaseloadEvidence::from_exhibits(exhibits));
        (ws, caseload)
    }

    mod guards_and_short_circuits {
        use super::*;

        #[test]
        fn test_empty_evidence_short_circuits_without_config_or_provider_calls() {
            let (ws, mut caseload) = sample_fixture(DEFAULT_JEV_MODEL);
            caseload.config = None;
            caseload.evidence = Some(CaseloadEvidence::default());
            let provider = MockVetProvider::empty();

            let out = execute_vet(&VetOptions::default(), caseload, &ws, &provider)
                .expect("empty active_cases short-circuits");
            assert!(provider.decision_requests.lock().unwrap().is_empty());
            let vetting = out.vetting.expect("vetting attached");
            assert!(vetting.is_all_vetted());
            assert!(vetting.cases.is_empty());
        }

        #[test]
        fn test_validates_options_prerequisites_and_fast_forwards_populated_vetting() {
            let (ws, _) = sample_fixture(DEFAULT_JEV_MODEL);
            let provider = MockVetProvider::empty();
            assert!(matches!(
                execute_vet(
                    &VetOptions {
                        threshold: 1.5,
                        ..VetOptions::default()
                    },
                    Caseload::new(),
                    &ws,
                    &provider
                )
                .unwrap_err(),
                VetError::InvalidThreshold { .. }
            ));
            assert!(matches!(
                execute_vet(
                    &VetOptions {
                        max_concurrent_calls: Some(0),
                        ..VetOptions::default()
                    },
                    Caseload::new(),
                    &ws,
                    &provider
                )
                .unwrap_err(),
                VetError::InvalidConcurrency { .. }
            ));
            assert_eq!(
                execute_vet(&VetOptions::default(), Caseload::new(), &ws, &provider).unwrap_err(),
                VetError::MissingIntake
            );

            let pre = Caseload::new().with_vetting(CaseloadVetting::default());
            let ff = execute_vet(&VetOptions::default(), pre.clone(), &ws, &provider).expect("ff");
            assert_eq!(ff, pre);
        }
    }

    mod discriminative_and_generative_vetting {
        use super::*;

        #[test]
        fn test_discriminative_jev_vetting_partitions_vetted_and_contested_at_080() {
            let (ws, caseload) = sample_fixture(DEFAULT_JEV_MODEL);
            let mut provider = MockVetProvider::empty();
            provider
                .decision_scores
                .insert(".canons/cli/clean-rule.md".to_string(), 0.94);
            provider
                .decision_scores
                .insert(".canons/cli/contested-rule.md".to_string(), 0.65);

            let out = execute_vet(
                &VetOptions {
                    max_concurrent_calls: Some(2),
                    ..VetOptions::default()
                },
                caseload,
                &ws,
                &provider,
            )
            .expect("jev vet");
            let vetting = out.vetting.expect("vetting");
            assert_eq!(
                vetting.vetted_cases,
                vec![".canons/cli/clean-rule.md".to_string()]
            );
            assert_eq!(
                vetting.contested_cases,
                vec![".canons/cli/contested-rule.md".to_string()]
            );
            assert_eq!(
                vetting.cases[".canons/cli/contested-rule.md"].vetting_status,
                VettingStatus::Contested
            );
            assert!(provider.structured_requests.lock().unwrap().is_empty());
        }

        #[test]
        fn test_standalone_generative_vetting_omits_reasoning_budget_and_decodes_scores() {
            let (ws, caseload) = sample_fixture(DEFAULT_SCREENER_MODEL);
            let mut provider = MockVetProvider::empty();
            provider.structured_routes.insert(
                ".canons/cli/clean-rule.md".to_string(),
                json!({
                    "vetting_summary": "Flags use kebab-case.",
                    "vetting_score": 0.92
                }),
            );
            provider.structured_routes.insert(
                ".canons/cli/contested-rule.md".to_string(),
                json!({
                    "vetting_summary": "Error lacks Hint remediation.",
                    "vetting_score": 0.12
                }),
            );

            let out = execute_vet(&VetOptions::default(), caseload, &ws, &provider)
                .expect("generative vet");
            let reqs = provider.structured_requests.lock().unwrap();
            assert_eq!(reqs.len(), 2);
            assert!(reqs.iter().all(|r| r.reasoning_budget.is_none()));

            let vetting = out.vetting.expect("vetting");
            assert_eq!(vetting.vetted_count(), 1);
            assert_eq!(vetting.contested_count(), 1);
        }
    }
}
