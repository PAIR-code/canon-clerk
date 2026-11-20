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
mod slices;
mod structured;

use crate::audit::build_audit_case_user_prompt;
use crate::canon::Canon;
use crate::caseload::{
    Caseload, CaseloadDiscovery, CaseloadIntake, CaseloadVetting, ExhibitAssessment, HeuristicNode,
    normalize_vetting_score,
};
use crate::configure::is_discriminative_model;
use crate::discover::WorkspaceSnapshot;
use crate::provider::{DEFAULT_GENERATION_TIMEOUT_MS, ProviderError, StructuredModelProvider};
use slices::{DiscriminativeCasePlan, build_discriminative_case_plan};
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
}

impl Default for VetOptions {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_VET_THRESHOLD,
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
            max_concurrent_calls: None,
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

struct PreparedVettingCase {
    canon_ref: String,
    user_prompt: String,
    discriminative_plan: DiscriminativeCasePlan,
}

fn validate_vet_options(options: &VetOptions) -> Result<f64, VetError> {
    let threshold = normalize_vetting_score(options.threshold)
        .map_err(|detail| VetError::InvalidThreshold { detail })?;
    if options.max_concurrent_calls == Some(0) {
        return Err(VetError::InvalidConcurrency {
            detail: format!(
                "max_concurrent_calls must be at least 1, got 0 (see `{ENV_VET_CONCURRENCY}`)"
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

    let mut active_entries = Vec::new();
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
            active_entries.push((canon_ref.clone(), canon, admitted));
        }
    }

    if active_entries.is_empty() {
        return Ok(caseload.with_vetting(CaseloadVetting::default()));
    }

    let node_cfg = caseload
        .config
        .as_ref()
        .and_then(|c| c.node(HeuristicNode::Vet))
        .ok_or(VetError::MissingConfig)?;
    let is_discriminative = is_discriminative_model(&node_cfg.model);
    let prepared_cases = prepare_vetting_cases(
        active_entries,
        &node_cfg.model,
        is_discriminative,
        intake,
        discovery,
        workspace,
    );

    let concurrency_limit = options
        .max_concurrent_calls
        .unwrap_or(DEFAULT_VET_MAX_CONCURRENT_CALLS);

    let cases = if is_discriminative {
        decision::execute_discriminative_vetting(
            &prepared_cases,
            &decision::DiscriminativeVetParams {
                vet_model: &node_cfg.model,
                threshold,
                concurrency_limit,
                timeout_ms: options.timeout_ms,
            },
            provider,
        )?
    } else {
        structured::execute_generative_vetting(
            &prepared_cases,
            &structured::GenerativeVetParams {
                vet_node: node_cfg,
                threshold,
                concurrency_limit,
                timeout_ms: options.timeout_ms,
            },
            provider,
        )?
    };

    Ok(caseload.with_vetting(CaseloadVetting::from_cases(cases)))
}

fn prepare_vetting_cases<'a>(
    active_entries: Vec<(String, &'a Canon, Vec<&'a ExhibitAssessment>)>,
    vet_model: &str,
    is_discriminative: bool,
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
    workspace: &WorkspaceSnapshot,
) -> Vec<PreparedVettingCase> {
    let max_state_chars = decision::max_discriminative_state_chars(vet_model);
    active_entries
        .into_iter()
        .map(|(canon_ref, canon, admitted)| {
            let (user_prompt, _) = build_audit_case_user_prompt(
                &canon_ref, canon, &admitted, intake, discovery, workspace,
            );
            let discriminative_plan = if is_discriminative {
                build_discriminative_case_plan(
                    &canon_ref,
                    canon,
                    &admitted,
                    intake,
                    discovery,
                    workspace,
                    max_state_chars,
                )
            } else {
                DiscriminativeCasePlan::Slices(Vec::new())
            };
            PreparedVettingCase {
                canon_ref,
                user_prompt,
                discriminative_plan,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::caseload::{
        AdmissibilityStatus, CanonDocketAssessment, CaseloadConfig, CaseloadDiscovery,
        CaseloadDocket, CaseloadEvidence, CaseloadIntake, CaseloadValidation, ColorabilityStatus,
        DEFAULT_GENERATIVE_SCREENING_MODEL, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL,
        DEFAULT_SCREENING_REASONING_BUDGET, DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL, DiscoveryMode,
        ExhibitAssessment, FileArtifact, FileChangeStatus, VettingStatus,
    };
    use crate::provider::{
        DecisionRequest, DecisionResponse, GenerationRequest, GenerationResponse, TokenUsage,
    };
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    struct MockVetProvider {
        decision_scores: BTreeMap<String, f64>,
        question_scores: BTreeMap<String, f64>,
        max_tokens_trigger: Option<String>,
        structured_routes: BTreeMap<String, Value>,
        decision_requests: Mutex<Vec<DecisionRequest>>,
        structured_requests: Mutex<Vec<GenerationRequest>>,
    }

    impl MockVetProvider {
        fn empty() -> Self {
            Self {
                decision_scores: BTreeMap::new(),
                question_scores: BTreeMap::new(),
                max_tokens_trigger: None,
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
            if let Some(ref trigger) = self.max_tokens_trigger
                && request.state.contains(trigger)
            {
                return Err(ProviderError::HttpStatus {
                    model_specifier: request.model_specifier.clone(),
                    status_code: 400,
                    detail: "max_tokens_exceeded".to_string(),
                    diagnostic: "HTTP 400 max_tokens_exceeded".to_string(),
                });
            }
            let default_score = self
                .decision_scores
                .iter()
                .find(|(k, _)| request.state.contains(k.as_str()))
                .map(|(_, s)| *s)
                .unwrap_or(0.95);
            let mut scores = BTreeMap::new();
            for key in request.questions.keys() {
                let s = self
                    .question_scores
                    .get(key)
                    .copied()
                    .unwrap_or(default_score);
                scores.insert(key.clone(), s);
            }
            Ok(DecisionResponse {
                scores,
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

    fn sample_fixture(vet_model: &str) -> (WorkspaceSnapshot, Caseload) {
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
            .with_config(CaseloadConfig::default().with_node_model(HeuristicNode::Vet, vet_model))
            .with_docket(CaseloadDocket::from_cases(docket_cases))
            .with_evidence(CaseloadEvidence::from_exhibits(exhibits));
        (ws, caseload)
    }

    mod guards_and_short_circuits {
        use super::*;

        #[test]
        fn test_empty_evidence_short_circuits_without_config_or_provider_calls() {
            let (ws, mut caseload) = sample_fixture(DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL);
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
            let (ws, _) = sample_fixture(DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL);
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
            let (ws, caseload) = sample_fixture(DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL);
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

        fn multi_slice_fixture(canon_ref: &str) -> (WorkspaceSnapshot, Caseload) {
            let ws = WorkspaceSnapshot::new()
                .with_canon_markdown(
                    "---\ntriggers:\n  - \"src/**\"\nreferences:\n  - \"README.md\"\n---\nDeveloped features MUST be reflected in README.md.\n",
                    &ParseCanonOptions::with_source_ref(canon_ref),
                )
                .with_file_content("README.md", "# Project\n");
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
            let mut admitted_items = vec![ExhibitAssessment {
                file_path: "README.md".to_string(),
                admissibility_summary: "companion doc".to_string(),
                admissibility_score: 0.96,
                admissibility_status: AdmissibilityStatus::Admitted,
            }];
            let big_body: String = (1..=350)
                .map(|ln| format!("+pub fn generated_item_{ln}() -> usize {{ {ln} }}\n"))
                .collect();
            for idx in 1..=6 {
                let path = format!("src/mod_{idx}.rs");
                let marker = if idx == 5 {
                    "// SLICE_VIOLATION_MARKER\n"
                } else {
                    ""
                };
                diffs.insert(
                    path.clone(),
                    FileArtifact {
                        path: path.clone(),
                        previous_path: None,
                        status: FileChangeStatus::Added,
                        lines_added: 351,
                        lines_deleted: 0,
                        patch: Some(format!("@@ -0,0 +1,351 @@\n+{marker}{big_body}")),
                        patch_omission_reason: None,
                        content: None,
                        content_omission_reason: None,
                    },
                );
                admitted_items.push(ExhibitAssessment {
                    file_path: path,
                    admissibility_summary: "domain module".to_string(),
                    admissibility_score: 0.90,
                    admissibility_status: AdmissibilityStatus::Admitted,
                });
            }
            let caseload = Caseload::new()
                .with_intake(CaseloadIntake {
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
                        .with_node_model(HeuristicNode::Vet, DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL),
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
                    admitted_items,
                )])));
            (ws, caseload)
        }

        #[test]
        fn test_multi_slice_discriminative_vetting_pins_admitted_references_and_takes_min_score() {
            let canon_ref = ".canons/readme/broad-rule.md";
            let (ws, caseload) = multi_slice_fixture(canon_ref);

            let mut clean_provider = MockVetProvider::empty();
            clean_provider
                .decision_scores
                .insert("SLICE_VIOLATION_MARKER".to_string(), 0.88);
            let vetted_out = execute_vet(
                &VetOptions::default(),
                caseload.clone(),
                &ws,
                &clean_provider,
            )
            .expect("multi-slice clean vet");
            let reqs = clean_provider.decision_requests.lock().unwrap();
            assert!(reqs.len() >= 2, "expected multiple context-budgeted slices");
            assert!(
                reqs.iter().all(|r| {
                    r.state.contains("src/mod_1.rs")
                        && r.state.contains("src/mod_6.rs")
                        && r.state.contains("PINNED_README_DIFF_SENTINEL")
                }),
                "every slice must carry the full path manifest and pinned reference diffs"
            );
            let clean_assessment = &vetted_out.vetting.unwrap().cases[canon_ref];
            assert_eq!(clean_assessment.vetting_status, VettingStatus::Vetted);
            assert!((clean_assessment.vetting_score - 0.88).abs() < 1e-9);
            assert!(clean_assessment.vetting_summary.contains("across"));
            assert!(clean_assessment.contested_exhibits.is_empty());

            let mut failing_provider = MockVetProvider::empty();
            failing_provider
                .decision_scores
                .insert("SLICE_VIOLATION_MARKER".to_string(), 0.22);
            let contested_out = execute_vet(
                &VetOptions::default(),
                caseload.clone(),
                &ws,
                &failing_provider,
            )
            .expect("multi-slice contested vet");
            let contested_assessment = &contested_out.vetting.unwrap().cases[canon_ref];
            assert_eq!(
                contested_assessment.vetting_status,
                VettingStatus::Contested
            );
            assert!((contested_assessment.vetting_score - 0.22).abs() < 1e-9);
            assert!(
                !contested_assessment.contested_exhibits.is_empty()
                    && contested_assessment
                        .contested_exhibits
                        .contains(&"src/mod_5.rs".to_string())
                    && !contested_assessment
                        .contested_exhibits
                        .contains(&"src/mod_1.rs".to_string())
                    && !contested_assessment
                        .contested_exhibits
                        .contains(&"README.md".to_string()),
                "partially contested multi-slice vet must record strictly the failing slice's exhibits, got {:?}",
                contested_assessment.contested_exhibits
            );

            let mut all_failing_provider = MockVetProvider::empty();
            all_failing_provider
                .decision_scores
                .insert("PINNED_README_DIFF_SENTINEL".to_string(), 0.15);
            let all_failed_out =
                execute_vet(&VetOptions::default(), caseload, &ws, &all_failing_provider)
                    .expect("all slices contested vet");
            let all_failed_assessment = &all_failed_out.vetting.unwrap().cases[canon_ref];
            assert_eq!(
                all_failed_assessment.vetting_status,
                VettingStatus::Contested
            );
            assert!(
                all_failed_assessment.contested_exhibits.is_empty(),
                "when zero slices clear threshold, contested_exhibits must remain empty"
            );
        }

        #[test]
        fn test_max_tokens_exceeded_binds_case_over_as_contested_without_aborting_siblings() {
            let (ws, caseload) = sample_fixture(DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL);
            let mut provider = MockVetProvider::empty();
            provider.max_tokens_trigger = Some(".canons/cli/contested-rule.md".to_string());

            let out = execute_vet(&VetOptions::default(), caseload, &ws, &provider)
                .expect("oversized case falls back to contested without aborting");
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
                vetting.cases[".canons/cli/contested-rule.md"].vetting_score,
                0.0
            );
        }

        #[test]
        fn test_standalone_generative_vetting_omits_reasoning_budget_and_decodes_scores() {
            let (ws, caseload) = sample_fixture(DEFAULT_GENERATIVE_SCREENING_MODEL);
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
            assert!(
                reqs.iter()
                    .all(|r| r.reasoning_budget == DEFAULT_SCREENING_REASONING_BUDGET)
            );

            let vetting = out.vetting.expect("vetting");
            assert_eq!(vetting.vetted_count(), 1);
            assert_eq!(vetting.contested_count(), 1);
        }

        #[test]
        fn test_default_gemini_decision_model_executes_discriminative_noul_vetting() {
            let (ws, caseload) = sample_fixture(DEFAULT_GOOGLE_DISCRIMINATOR_MODEL);
            let mut provider = MockVetProvider::empty();
            provider
                .decision_scores
                .insert(".canons/cli/clean-rule.md".to_string(), 0.96);
            provider
                .decision_scores
                .insert(".canons/cli/contested-rule.md".to_string(), 0.08);

            let out = execute_vet(&VetOptions::default(), caseload, &ws, &provider)
                .expect("gemini-decision-1 discriminative vet");
            assert!(
                provider.structured_requests.lock().unwrap().is_empty(),
                "default gemini-decision-1 must not call generate_structured"
            );
            assert_eq!(provider.decision_requests.lock().unwrap().len(), 2);

            let vetting = out.vetting.expect("vetting");
            assert_eq!(
                vetting.vetted_cases,
                vec![".canons/cli/clean-rule.md".to_string()]
            );
            assert_eq!(
                vetting.contested_cases,
                vec![".canons/cli/contested-rule.md".to_string()]
            );
        }

        fn span_localization_fixture(canon_ref: &str) -> (WorkspaceSnapshot, Caseload) {
            let mut big_file = String::from("use std::io;\n\n");
            for i in 1..=3 {
                big_file.push_str(&format!("fn section_{i}() {{\n"));
                for j in 1..=45 {
                    big_file.push_str(&format!("    let v_{i}_{j} = {j};\n"));
                }
                big_file.push_str("}\n\n");
            }
            let ws = WorkspaceSnapshot::new()
                .with_canon_markdown(
                    "---\ntriggers:\n  - \"src/**\"\n---\nProduction code MUST NOT call unwrap.\n",
                    &ParseCanonOptions::with_source_ref(canon_ref),
                )
                .with_file_content("src/big.rs", &big_file)
                .with_file_content("src/clean.rs", "pub fn ok() -> usize { 1 }\n");

            let exhibits = BTreeMap::from([(
                canon_ref.to_string(),
                vec![
                    ExhibitAssessment {
                        file_path: "src/big.rs".to_string(),
                        admissibility_summary: "big file".to_string(),
                        admissibility_score: 0.95,
                        admissibility_status: AdmissibilityStatus::Admitted,
                    },
                    ExhibitAssessment {
                        file_path: "src/clean.rs".to_string(),
                        admissibility_summary: "clean file".to_string(),
                        admissibility_score: 0.95,
                        admissibility_status: AdmissibilityStatus::Admitted,
                    },
                ],
            )]);
            let caseload = Caseload::new()
                .with_intake(CaseloadIntake {
                    target_paths: vec!["src/big.rs".to_string(), "src/clean.rs".to_string()],
                    ..CaseloadIntake::default()
                })
                .with_discovery(CaseloadDiscovery {
                    mode: DiscoveryMode::Triggered,
                    candidate_canons: vec![canon_ref.to_string()],
                    ..CaseloadDiscovery::default()
                })
                .with_validation(CaseloadValidation::default())
                .with_config(CaseloadConfig::default().with_node_model(
                    HeuristicNode::Vet,
                    DEFAULT_GOOGLE_DISCRIMINATOR_MODEL,
                ))
                .with_docket(CaseloadDocket::from_cases(BTreeMap::from([(
                    canon_ref.to_string(),
                    CanonDocketAssessment {
                        colorability_summary: "docketed".to_string(),
                        colorability_score: 0.95,
                        colorability_status: ColorabilityStatus::Docketed,
                    },
                )])))
                .with_evidence(CaseloadEvidence::from_exhibits(exhibits));
            (ws, caseload)
        }

        #[test]
        fn test_single_pass_span_localization_records_spans_and_prunes_clean_files() {
            let canon_ref = ".canons/core/no-unwrap.md";
            let (ws, caseload) = span_localization_fixture(canon_ref);

            let mut tier1 = MockVetProvider::empty();
            tier1.question_scores.insert("compliant".to_string(), 0.25);
            tier1
                .question_scores
                .insert("file:src/big.rs".to_string(), 0.22);
            tier1
                .question_scores
                .insert("file:src/clean.rs".to_string(), 0.96);
            tier1.question_scores.insert("span:S2".to_string(), 0.14);

            let out = execute_vet(&VetOptions::default(), caseload, &ws, &tier1)
                .expect("tier 1 vet");
            let reqs = tier1.decision_requests.lock().unwrap();
            assert_eq!(reqs.len(), 1, "whole + files + spans packed in 1 forward pass");
            assert!(reqs[0].state.contains("=== [SPAN S2: src/big.rs"));
            assert!(reqs[0].questions.contains_key("file:src/big.rs"));
            assert!(reqs[0].questions.contains_key("span:S2"));

            let assessment = &out.vetting.unwrap().cases[canon_ref];
            assert_eq!(assessment.vetting_status, VettingStatus::Contested);
            assert_eq!(assessment.contested_exhibits, vec!["src/big.rs".to_string()]);
            assert_eq!(assessment.contested_spans.len(), 1);
            assert_eq!(assessment.contested_spans[0].path, "src/big.rs");
            assert!((assessment.contested_spans[0].score - 0.14).abs() < 1e-9);
        }

        #[test]
        fn test_three_tier_fail_safe_fallback_handles_washout_and_omission_canons() {
            let canon_ref = ".canons/core/no-unwrap.md";
            let (ws, caseload) = span_localization_fixture(canon_ref);

            // Tier 2 (Jev washout): file_score < threshold and 0 hot spans -> keep full file, empty spans
            let mut tier2 = MockVetProvider::empty();
            tier2.question_scores.insert("compliant".to_string(), 0.30);
            tier2
                .question_scores
                .insert("file:src/big.rs".to_string(), 0.30);
            tier2
                .question_scores
                .insert("file:src/clean.rs".to_string(), 0.95);
            let out2 = execute_vet(&VetOptions::default(), caseload.clone(), &ws, &tier2)
                .expect("tier 2 vet");
            let a2 = &out2.vetting.unwrap().cases[canon_ref];
            assert_eq!(a2.vetting_status, VettingStatus::Contested);
            assert_eq!(a2.contested_exhibits, vec!["src/big.rs".to_string()]);
            assert!(a2.contested_spans.is_empty());

            // Tier 3 (omission canon): whole_score < threshold and 0 files/spans < threshold -> full slice fallback
            let mut tier3 = MockVetProvider::empty();
            tier3.question_scores.insert("compliant".to_string(), 0.20);
            let out3 = execute_vet(&VetOptions::default(), caseload, &ws, &tier3)
                .expect("tier 3 vet");
            let a3 = &out3.vetting.unwrap().cases[canon_ref];
            assert_eq!(a3.vetting_status, VettingStatus::Contested);
            assert!(a3.contested_exhibits.is_empty());
            assert!(a3.contested_spans.is_empty());
        }
    }
}
