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

mod prompt;

use crate::caseload::{
    CanonDocketAssessment, Caseload, CaseloadDocket, ColorabilityStatus,
    normalize_colorability_score,
};
use crate::discover::WorkspaceSnapshot;
use crate::provider::{
    DEFAULT_GENERATION_TIMEOUT_MS, GenerationRequest, ProviderError, StructuredModelProvider,
};
use prompt::{DOCKET_SYSTEM_INSTRUCTION, build_docket_schema, build_docket_user_prompt};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Default subject-matter jurisdiction threshold for `docket` (`docs/architecture/nodes/docket.md` §4.4).
pub const DEFAULT_DOCKET_THRESHOLD: f64 = 0.5;

/// Environment variable name for overriding `--docket-threshold` (`docs/architecture/nodes/docket.md` §5).
pub const ENV_DOCKET_THRESHOLD: &str = "CANON_CLERK_DOCKET_THRESHOLD";

/// Domain input parameters for executing the `docket` pipeline stage.
#[derive(Debug, Clone, PartialEq)]
pub struct DocketOptions {
    /// Subject-matter jurisdiction score threshold in `[0.0, 1.0]` (defaults to [`DEFAULT_DOCKET_THRESHOLD`]).
    pub threshold: f64,
    /// Per-request timeout ceiling in milliseconds (defaults to [`DEFAULT_GENERATION_TIMEOUT_MS`]).
    pub timeout_ms: u64,
}

impl Default for DocketOptions {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_DOCKET_THRESHOLD,
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
        }
    }
}

/// Domain errors produced during `docket` execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocketError {
    /// Configured threshold is non-finite or outside `[0.0, 1.0]`.
    InvalidThreshold { detail: String },
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
    /// Structured model provider returned a transport, HTTP, block, or schema decoding error.
    Provider(ProviderError),
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

    let config = caseload.config.as_ref().ok_or(DocketError::MissingConfig)?;

    let user_prompt = build_docket_user_prompt(intake, discovery, workspace)
        .map_err(|canon_ref| DocketError::MissingCandidateCanon { canon_ref })?;
    let response_schema = build_docket_schema();

    let request = GenerationRequest {
        model_specifier: config.screener_model.clone(),
        system_instruction: DOCKET_SYSTEM_INSTRUCTION.to_string(),
        user_prompt,
        response_schema,
        reasoning_budget: None,
        timeout_ms: options.timeout_ms,
    };

    let response = provider.generate_structured(&request)?;
    let cases = decode_docket_cases(
        &response.payload,
        &discovery.candidate_canons,
        threshold,
        &config.screener_model,
    )?;

    Ok(caseload.with_docket(CaseloadDocket::from_cases(cases)))
}

fn schema_decode_error(model_specifier: &str, message: impl Into<String>) -> DocketError {
    DocketError::Provider(ProviderError::SchemaDecode {
        model_specifier: model_specifier.to_string(),
        message: message.into(),
    })
}

fn decode_docket_cases(
    payload: &Value,
    candidate_canons: &[String],
    threshold: f64,
    model_specifier: &str,
) -> Result<BTreeMap<String, CanonDocketAssessment>, DocketError> {
    let cases_arr = payload
        .get("cases")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            schema_decode_error(
                model_specifier,
                "missing array field `cases` in structured docket response",
            )
        })?;

    let expected: BTreeSet<&str> = candidate_canons.iter().map(String::as_str).collect();
    let mut out = BTreeMap::new();

    for (index, raw_item) in cases_arr.iter().enumerate() {
        let (canon_path, assessment) =
            decode_single_case(raw_item, index, &expected, threshold, model_specifier)?;
        if out.contains_key(&canon_path) {
            return Err(schema_decode_error(
                model_specifier,
                format!("duplicate `canon_path` `{canon_path}` in structured docket response"),
            ));
        }
        out.insert(canon_path, assessment);
    }

    let missing: Vec<&str> = candidate_canons
        .iter()
        .map(String::as_str)
        .filter(|canon_ref| !out.contains_key(*canon_ref))
        .collect();
    if !missing.is_empty() {
        return Err(schema_decode_error(
            model_specifier,
            format!(
                "missing case entry for candidate canon(s): {}",
                missing.join(", ")
            ),
        ));
    }

    Ok(out)
}

fn decode_single_case(
    raw_item: &Value,
    index: usize,
    expected: &BTreeSet<&str>,
    threshold: f64,
    model_specifier: &str,
) -> Result<(String, CanonDocketAssessment), DocketError> {
    let entry = raw_item.as_object().ok_or_else(|| {
        schema_decode_error(
            model_specifier,
            format!("expected object at `cases[{index}]` in structured docket response"),
        )
    })?;

    let canon_path = entry
        .get("canon_path")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            schema_decode_error(
                model_specifier,
                format!("missing string `canon_path` at `cases[{index}]`"),
            )
        })?;

    if !expected.contains(canon_path) {
        return Err(schema_decode_error(
            model_specifier,
            format!("unknown `canon_path` `{canon_path}` in structured docket response"),
        ));
    }

    let raw_summary = entry
        .get("colorability_summary")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            schema_decode_error(
                model_specifier,
                format!("missing string `colorability_summary` for `{canon_path}`"),
            )
        })?;

    let raw_score = entry
        .get("colorability_score")
        .and_then(Value::as_f64)
        .ok_or_else(|| {
            schema_decode_error(
                model_specifier,
                format!("missing numeric `colorability_score` for `{canon_path}`"),
            )
        })?;

    let colorability_score = normalize_colorability_score(raw_score).map_err(|detail| {
        schema_decode_error(
            model_specifier,
            format!("invalid `colorability_score` for `{canon_path}`: {detail}"),
        )
    })?;

    Ok((
        canon_path.to_string(),
        CanonDocketAssessment {
            colorability_summary: raw_summary.trim().to_string(),
            colorability_score,
            colorability_status: ColorabilityStatus::from_score(colorability_score, threshold),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::caseload::{
        ActiveExhibits, CaseloadConfig, CaseloadDiscovery, CaseloadIntake, CaseloadValidation,
        DEFAULT_SCREENER_MODEL, FileArtifact, FileChangeStatus, LinkedIssueContext,
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
            .with_config(CaseloadConfig::default())
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
            assert_eq!(req.model_specifier, DEFAULT_SCREENER_MODEL);
            assert_eq!(req.reasoning_budget, None);
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
        fn test_constant_size_wire_schema_across_60_candidate_canons_and_threshold_boundary() {
            let mut ws = WorkspaceSnapshot::new();
            let mut candidates = Vec::new();
            let mut wire_cases = Vec::new();
            for i in 0..60 {
                let path = format!(".canons/pack/rule-{i:03}.md");
                ws = ws.with_canon_markdown(
                    &format!("# Rule {i}\n\nRule {i} MUST hold.\n"),
                    &ParseCanonOptions::with_source_ref(path.clone()),
                );
                let score = if i == 30 { 0.70 } else { 0.69 };
                wire_cases.push(json!({
                    "canon_path": path,
                    "colorability_summary": format!("Evaluated rule {i}."),
                    "colorability_score": score
                }));
                candidates.push(path);
            }

            let baseline_schema = build_docket_schema();
            let provider = RecordingMockProvider::with_payload(json!({ "cases": wire_cases }));
            let caseload = sample_filing_caseload(candidates);

            let enriched = execute_docket(
                &DocketOptions {
                    threshold: 0.70,
                    ..DocketOptions::default()
                },
                caseload,
                &ws,
                &provider,
            )
            .expect("60-canon docket succeeds");

            let req = &provider.captured_requests()[0];
            assert_eq!(
                req.response_schema, baseline_schema,
                "wire schema must remain O(1) constant size regardless of candidate canon count"
            );

            let docket = enriched.docket.expect("docket present");
            assert_eq!(docket.cases.len(), 60);
            assert_eq!(
                docket.active_docket,
                vec![".canons/pack/rule-030.md".to_string()],
                "exact threshold boundary score == 0.70 must be docketed"
            );
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
                model_specifier: DEFAULT_SCREENER_MODEL.to_string(),
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
}
