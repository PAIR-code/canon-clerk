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

mod prompt;

use crate::caseload::{
    ApprisalStatus, CanonApprisalAssessment, Caseload, CaseloadApprisal, normalize_unit_score,
};
use crate::discover::WorkspaceSnapshot;
use crate::provider::{
    DEFAULT_GENERATION_TIMEOUT_MS, GenerationRequest, ProviderError, StructuredModelProvider,
};
use prompt::{APPRISE_SYSTEM_INSTRUCTION, build_apprise_schema, build_apprise_user_prompt};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;

/// Default prospective applicability score threshold for `apprise` (`docs/architecture/nodes/apprise.md` §4.4).
pub const DEFAULT_APPRISE_THRESHOLD: f64 = 0.5;

/// Environment variable name for overriding `--apprise-threshold` (`docs/architecture/nodes/apprise.md` §5).
pub const ENV_APPRISE_THRESHOLD: &str = "CANON_CLERK_APPRISE_THRESHOLD";

/// Domain input parameters for executing the `apprise` pipeline stage.
#[derive(Debug, Clone, PartialEq)]
pub struct AppriseOptions {
    /// Prospective applicability score threshold in `[0.0, 1.0]` (defaults to [`DEFAULT_APPRISE_THRESHOLD`]).
    pub threshold: f64,
    /// Per-request timeout ceiling in milliseconds (defaults to [`DEFAULT_GENERATION_TIMEOUT_MS`]).
    pub timeout_ms: u64,
}

impl Default for AppriseOptions {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_APPRISE_THRESHOLD,
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
        }
    }
}

/// Domain errors produced during `apprise` execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppriseError {
    /// Configured threshold is non-finite or outside `[0.0, 1.0]`.
    InvalidThreshold { detail: String },
    /// Prospective design `intent` is missing or blank on `.intake`.
    MissingIntent,
    /// Prerequisite `.discovery` namespace is missing from the incoming `Caseload`.
    MissingDiscovery,
    /// Upstream `.validation` recorded one or more candidate canon lint errors.
    ValidationFailed,
    /// Prerequisite `.config` namespace is missing when non-empty candidate canons require model screening.
    MissingConfig,
    /// A candidate canon listed in `.discovery.candidate_canons` is absent from the workspace snapshot.
    MissingCandidateCanon { canon_ref: String },
    /// Structured model provider returned a transport, HTTP, block, or schema decoding error.
    Provider(ProviderError),
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
            Self::MissingCandidateCanon { canon_ref } => write!(
                f,
                "Candidate canon `{canon_ref}` referenced in `.discovery` was not found in the workspace snapshot.\n  \
                 Hint: Re-run `canon-clerk discover` against the current workspace."
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

    let config = caseload
        .config
        .as_ref()
        .ok_or(AppriseError::MissingConfig)?;

    let user_prompt = build_apprise_user_prompt(intake, discovery, workspace)
        .map_err(|canon_ref| AppriseError::MissingCandidateCanon { canon_ref })?;
    let response_schema = build_apprise_schema(&discovery.candidate_canons);

    let request = GenerationRequest {
        model_specifier: config.screener_model.clone(),
        system_instruction: APPRISE_SYSTEM_INSTRUCTION.to_string(),
        user_prompt,
        response_schema,
        reasoning_budget: None,
        timeout_ms: options.timeout_ms,
    };

    let response = provider.generate_structured(&request)?;
    let assessments = decode_apprise_assessments(
        &response.payload,
        &discovery.candidate_canons,
        threshold,
        &config.screener_model,
    )?;

    Ok(caseload.with_apprisal(CaseloadApprisal { assessments }))
}

fn decode_apprise_assessments(
    payload: &Value,
    candidate_canons: &[String],
    threshold: f64,
    model_specifier: &str,
) -> Result<BTreeMap<String, CanonApprisalAssessment>, AppriseError> {
    let assessments_obj = payload
        .get("assessments")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            AppriseError::Provider(ProviderError::SchemaDecode {
                model_specifier: model_specifier.to_string(),
                message: "missing object field `assessments` in structured apprise response"
                    .to_string(),
            })
        })?;

    let mut out = BTreeMap::new();
    for canon_ref in candidate_canons {
        let assessment =
            decode_single_assessment(assessments_obj, canon_ref, threshold, model_specifier)?;
        out.insert(canon_ref.clone(), assessment);
    }

    Ok(out)
}

fn decode_single_assessment(
    assessments_obj: &serde_json::Map<String, Value>,
    canon_ref: &str,
    threshold: f64,
    model_specifier: &str,
) -> Result<CanonApprisalAssessment, AppriseError> {
    let entry = assessments_obj
        .get(canon_ref)
        .and_then(Value::as_object)
        .ok_or_else(|| {
            AppriseError::Provider(ProviderError::SchemaDecode {
                model_specifier: model_specifier.to_string(),
                message: format!("missing assessment entry for candidate canon `{canon_ref}`"),
            })
        })?;

    let raw_summary = entry
        .get("apprisal_summary")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppriseError::Provider(ProviderError::SchemaDecode {
                model_specifier: model_specifier.to_string(),
                message: format!("missing string `apprisal_summary` for `{canon_ref}`"),
            })
        })?;

    let raw_score = entry
        .get("apprisal_score")
        .and_then(Value::as_f64)
        .ok_or_else(|| {
            AppriseError::Provider(ProviderError::SchemaDecode {
                model_specifier: model_specifier.to_string(),
                message: format!("missing numeric `apprisal_score` for `{canon_ref}`"),
            })
        })?;

    let apprisal_score = normalize_unit_score(raw_score).map_err(|detail| {
        AppriseError::Provider(ProviderError::SchemaDecode {
            model_specifier: model_specifier.to_string(),
            message: format!("invalid `apprisal_score` for `{canon_ref}`: {detail}"),
        })
    })?;

    Ok(CanonApprisalAssessment {
        apprisal_summary: raw_summary.trim().to_string(),
        apprisal_score,
        apprisal_status: ApprisalStatus::from_score(apprisal_score, threshold),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::caseload::{
        CaseloadConfig, CaseloadDiscovery, CaseloadIntake, CaseloadValidation,
        DEFAULT_SCREENER_MODEL,
    };
    use crate::provider::{GenerationResponse, TokenUsage};
    use serde_json::json;
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

        fn recorded_requests(&self) -> Vec<GenerationRequest> {
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
                    resolved_model: "gemini-2.5-flash-lite".to_string(),
                    latency_ms: 95,
                    usage: TokenUsage {
                        prompt_tokens: 410,
                        completion_tokens: 90,
                        reasoning_tokens: 0,
                        total_tokens: 500,
                    },
                    thoughts: Vec::new(),
                }),
                Err(err) => Err(err.clone()),
            }
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
            .with_config(CaseloadConfig::default())
    }

    mod zero_candidate_and_prerequisite_guards {
        use super::*;

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
            let provider = RecordingMockProvider::with_error(ProviderError::MissingApiKey);

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
        fn test_rejects_missing_intent_discovery_config_validation_failure_and_bad_threshold() {
            let snapshot = fixture_snapshot();
            let provider = RecordingMockProvider::with_payload(json!({"assessments": {}}));

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
            assert!(bad_threshold_err.to_string().contains("Hint:"));

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

            let invalid_val = Caseload::new()
                .with_intake(CaseloadIntake {
                    intent: Some("Add cache".to_string()),
                    ..CaseloadIntake::default()
                })
                .with_discovery(CaseloadDiscovery {
                    candidate_canons: vec![".canons/auth/cache-expiry.md".to_string()],
                    ..CaseloadDiscovery::default()
                })
                .with_validation(CaseloadValidation {
                    has_errors: true,
                    results: BTreeMap::new(),
                });
            let val_err = execute_apprise(
                &AppriseOptions::default(),
                invalid_val,
                &snapshot,
                &provider,
            )
            .expect_err("validation failure must abort before provider call");
            assert_eq!(val_err, AppriseError::ValidationFailed);

            let no_cfg = Caseload::new()
                .with_intake(CaseloadIntake {
                    intent: Some("Add cache".to_string()),
                    ..CaseloadIntake::default()
                })
                .with_discovery(CaseloadDiscovery {
                    candidate_canons: vec![".canons/auth/cache-expiry.md".to_string()],
                    ..CaseloadDiscovery::default()
                });
            let cfg_err = execute_apprise(&AppriseOptions::default(), no_cfg, &snapshot, &provider)
                .expect_err("missing config must fail when candidates exist");
            assert_eq!(cfg_err, AppriseError::MissingConfig);
        }
    }

    mod aggregate_screening_and_threshold_gating {
        use super::*;

        #[test]
        fn test_single_turn_reason_first_schema_and_deterministic_threshold_derivation() {
            let snapshot = fixture_snapshot();
            let candidates = vec![
                ".canons/auth/cache-expiry.md".to_string(),
                ".canons/auth/db-rollback.md".to_string(),
            ];
            let caseload = fixture_caseload(candidates);
            let provider = RecordingMockProvider::with_payload(json!({
                "assessments": {
                    ".canons/auth/cache-expiry.md": {
                        "apprisal_summary": "In-memory TTL cache directly requires explicit expiration rules.",
                        "apprisal_score": 0.91
                    },
                    ".canons/auth/db-rollback.md": {
                        "apprisal_summary": "No SQL migrations or schema modifications are planned.",
                        "apprisal_score": 0.49
                    }
                }
            }));

            let result =
                execute_apprise(&AppriseOptions::default(), caseload, &snapshot, &provider)
                    .expect("apprise screening must succeed");

            let requests = provider.recorded_requests();
            assert_eq!(
                requests.len(),
                1,
                "must screen all candidates in a single turn"
            );
            let req = &requests[0];
            assert_eq!(req.model_specifier, DEFAULT_SCREENER_MODEL);
            assert_eq!(req.reasoning_budget, None);
            assert!(req.user_prompt.contains("Refactor auth provider dispatch"));
            assert!(req.user_prompt.contains(".canons/auth/cache-expiry.md"));
            assert!(req.user_prompt.contains(".canons/auth/db-rollback.md"));

            let item_schema = &req.response_schema["properties"]["assessments"]["properties"][".canons/auth/cache-expiry.md"];
            assert_eq!(
                item_schema["propertyOrdering"],
                json!(["apprisal_summary", "apprisal_score"]),
                "schema must enforce reason-first ordering"
            );
            assert_eq!(
                item_schema["required"],
                json!(["apprisal_summary", "apprisal_score"])
            );

            let apprisal = result.apprisal.expect("apprisal populated");
            assert_eq!(apprisal.applicable_count(), 1);
            assert_eq!(apprisal.dismissed_count(), 1);
            assert_eq!(
                apprisal.assessments[".canons/auth/cache-expiry.md"].apprisal_status,
                ApprisalStatus::Applicable
            );
            assert_eq!(
                apprisal.assessments[".canons/auth/db-rollback.md"].apprisal_status,
                ApprisalStatus::Dismissed
            );
        }

        #[test]
        fn test_custom_threshold_boundary_inclusivity() {
            let snapshot = fixture_snapshot();
            let candidates = vec![
                ".canons/auth/cache-expiry.md".to_string(),
                ".canons/auth/db-rollback.md".to_string(),
            ];
            let provider = RecordingMockProvider::with_payload(json!({
                "assessments": {
                    ".canons/auth/cache-expiry.md": {
                        "apprisal_summary": "Exact threshold match.",
                        "apprisal_score": 0.75
                    },
                    ".canons/auth/db-rollback.md": {
                        "apprisal_summary": "Below custom threshold.",
                        "apprisal_score": 0.74
                    }
                }
            }));

            let result = execute_apprise(
                &AppriseOptions {
                    threshold: 0.75,
                    ..AppriseOptions::default()
                },
                fixture_caseload(candidates),
                &snapshot,
                &provider,
            )
            .expect("custom threshold execution");

            let apprisal = result.apprisal.expect("apprisal populated");
            assert_eq!(
                apprisal.assessments[".canons/auth/cache-expiry.md"].apprisal_status,
                ApprisalStatus::Applicable
            );
            assert_eq!(
                apprisal.assessments[".canons/auth/db-rollback.md"].apprisal_status,
                ApprisalStatus::Dismissed
            );
        }
    }

    mod malformed_and_failed_provider_responses {
        use super::*;

        #[test]
        fn test_propagates_provider_errors_and_rejects_incomplete_or_out_of_range_payloads() {
            let snapshot = fixture_snapshot();
            let candidates = vec![".canons/auth/cache-expiry.md".to_string()];

            let auth_fail_provider = RecordingMockProvider::with_error(ProviderError::HttpStatus {
                status_code: 401,
                model_specifier: DEFAULT_SCREENER_MODEL.to_string(),
                detail: "API key not valid".to_string(),
                diagnostic: "HTTP 401: Invalid API key.\n  Hint: Check GEMINI_API_KEY.".to_string(),
            });
            let err = execute_apprise(
                &AppriseOptions::default(),
                fixture_caseload(candidates.clone()),
                &snapshot,
                &auth_fail_provider,
            )
            .expect_err("HTTP 401 must fail");
            assert!(err.to_string().contains("HTTP 401"));

            let out_of_range_provider = RecordingMockProvider::with_payload(json!({
                "assessments": {
                    ".canons/auth/cache-expiry.md": {
                        "apprisal_summary": "Score exceeds 1.0",
                        "apprisal_score": 1.4
                    }
                }
            }));
            let score_err = execute_apprise(
                &AppriseOptions::default(),
                fixture_caseload(candidates),
                &snapshot,
                &out_of_range_provider,
            )
            .expect_err("score > 1.0 in model response must fail schema decode");
            assert!(matches!(
                score_err,
                AppriseError::Provider(ProviderError::SchemaDecode { .. })
            ));
        }
    }
}
