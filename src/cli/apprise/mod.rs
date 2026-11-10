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

//! CLI driving adapter for the `canon-clerk apprise` subcommand (`docs/architecture/nodes/apprise.md` §5).

mod guards;
mod render;
mod stages;

use crate::caseload::{Caseload, CaseloadApprisal};
use crate::cli::CliError;
use crate::cli::args::{CanonSelectionArgs, CaseloadIoArgs, FilingSourceArgs, ProviderConfigArgs};
use crate::cli::io::ensure_single_stdin_source;
use crate::discover::WorkspaceSnapshot;
use crate::provider::StructuredModelProvider;
use crate::telemetry::TelemetrySession;
use clap::{Args, ValueEnum};
use guards::{
    can_fast_forward_completed_apprisal, enforce_dual_input_guard,
    ensure_stage_prefixed_threshold_flags, resolve_apprise_threshold,
};
use render::{AppriseWatermarks, StageDisposition, render_apprise_output};
use stages::{
    AppriseBackend, execute_jit_configure_and_apprise, materialize_snapshot,
    resolve_apprise_discover, resolve_apprise_intake, resolve_apprise_validate,
};
use std::collections::BTreeMap;
use std::env;
use std::io::{Read, Write};

const APPRISE_EXAMPLES: &str = "\
Examples:
  # Apprise governing canons for inline design intent and prospective target paths:
  canon-clerk apprise --intent \"Refactor auth to round-robin between providers\" src/auth src/frontend

  # Stream newline-delimited prospective target paths via standard input ('-'):
  git diff origin/main --name-only | canon-clerk apprise --intent \"Rotate auth provider\" -

  # Emit tab-separated compact apprisal lines for shell scripting:
  canon-clerk apprise --intent \"Add CLI output flag\" src/cli --format compact

  # Emit enriched Caseload JSON for AI agent prompt injection:
  canon-clerk apprise --intent \"Add Kafka event bus\" src/events --json";

/// Output presentation format for `canon-clerk apprise`.
#[derive(ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AppriseFormat {
    /// Human-readable Pipeline Funnel Provenance receipt and Statutory Apprisal summary.
    #[default]
    Stylish,
    /// Full cumulative `Caseload` JSON envelope.
    Json,
    /// Tab-separated (`<canon>\t<status>\t<score>\t<summary>`) one-line-per-canon output.
    Compact,
}

/// Command-line arguments for `canon-clerk apprise`.
#[derive(Args, Debug, Clone, Default, PartialEq)]
#[command(
    about = "Prospective statutory notice: determine governing canons for planned design intent and target scope",
    long_about = "Statutory Apprisal stage of the Caseload DAG (Apprisal Track). Executes the guarded [intake, discover, validate, configure, apprise] closure to screen candidate canons against prospective architectural design intent and target scope in a single structured turn before implementation begins.",
    after_help = APPRISE_EXAMPLES
)]
pub struct AppriseArgs {
    #[command(flatten)]
    pub filing: FilingSourceArgs,

    #[command(flatten)]
    pub canons: CanonSelectionArgs,

    /// Prospective applicability score threshold in `[0.0, 1.0]` (default: `0.5`; env: `CANON_CLERK_APPRISE_THRESHOLD`).
    #[arg(
        long = "apprise-threshold",
        value_name = "FLOAT",
        allow_hyphen_values = true,
        help_heading = "Apprisal Options",
        help = "Prospective applicability threshold in [0.0, 1.0] (default: 0.5; fallback env: CANON_CLERK_APPRISE_THRESHOLD)"
    )]
    pub apprise_threshold: Option<f64>,

    /// Hidden capture rejecting ambiguous unqualified `--threshold` (`scheduling.md` §4 Pattern C).
    #[arg(long = "threshold", hide = true, allow_hyphen_values = true)]
    pub rejected_threshold: Option<String>,

    /// Hidden capture rejecting out-of-closure `--docket-threshold` (`scheduling.md` §4 Pattern C).
    #[arg(long = "docket-threshold", hide = true, allow_hyphen_values = true)]
    pub rejected_docket_threshold: Option<String>,

    /// Hidden capture rejecting out-of-closure `--admit-threshold` (`scheduling.md` §4 Pattern C).
    #[arg(long = "admit-threshold", hide = true, allow_hyphen_values = true)]
    pub rejected_admit_threshold: Option<String>,

    #[command(flatten)]
    pub provider: ProviderConfigArgs,

    /// Output format (`stylish`, `json`, or `compact`).
    #[arg(
        long = "format",
        value_enum,
        default_value_t = AppriseFormat::Stylish,
        help_heading = "Caseload & Output",
        help = "Format statutory apprisal output"
    )]
    pub format: AppriseFormat,

    #[command(flatten)]
    pub io: CaseloadIoArgs,
}

impl Eq for AppriseArgs {}

/// Executes `canon-clerk apprise` hermetically with an explicit [`TelemetrySession`].
pub fn run_apprise_with_telemetry(
    args: &AppriseArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace: &WorkspaceSnapshot,
    env: &BTreeMap<String, String>,
    provider: &dyn StructuredModelProvider,
    telemetry: &TelemetrySession<'_>,
) -> Result<Caseload, CliError> {
    run_apprise_core(
        args,
        stdin,
        stdout,
        env,
        AppriseBackend::Hermetic {
            workspace,
            provider,
        },
        telemetry,
    )
}

/// Executes `canon-clerk apprise` hermetically against an in-memory [`WorkspaceSnapshot`],
/// explicit environment map, and hexagonal [`StructuredModelProvider`].
pub fn run_apprise_with_snapshot_and_provider(
    args: &AppriseArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace: &WorkspaceSnapshot,
    env: &BTreeMap<String, String>,
    provider: &dyn StructuredModelProvider,
) -> Result<Caseload, CliError> {
    run_apprise_with_telemetry(
        args,
        stdin,
        stdout,
        workspace,
        env,
        provider,
        &TelemetrySession::noop(),
    )
}

/// Executes `canon-clerk apprise` against the live working directory with an explicit [`TelemetrySession`].
pub fn run_apprise_live_with_telemetry(
    args: &AppriseArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    telemetry: &TelemetrySession<'_>,
) -> Result<Caseload, CliError> {
    let cwd = env::current_dir().map_err(|err| {
        CliError::runtime(format!(
            "Failed to determine current working directory: {err}"
        ))
    })?;
    let env_map: BTreeMap<String, String> = env::vars().collect();
    run_apprise_core(
        args,
        stdin,
        stdout,
        &env_map,
        AppriseBackend::LiveDisk {
            workspace_root: &cwd,
        },
        telemetry,
    )
}

/// Executes `canon-clerk apprise` using the current working directory, process environment,
/// and live [`GeminiModelProvider`].
pub fn run_apprise(
    args: &AppriseArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    run_apprise_live_with_telemetry(args, stdin, stdout, &TelemetrySession::noop())
}

fn run_apprise_core(
    args: &AppriseArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    env: &BTreeMap<String, String>,
    backend: AppriseBackend<'_>,
    telemetry: &TelemetrySession<'_>,
) -> Result<Caseload, CliError> {
    ensure_stage_prefixed_threshold_flags(args)?;
    ensure_single_stdin_source(&args.filing, args.io.caseload.as_deref())?;
    let threshold = resolve_apprise_threshold(args.apprise_threshold, env)?;

    let incoming = args.io.read_caseload(stdin)?;
    let intake_opts = args.filing.resolve_intake_options(stdin)?;

    if can_fast_forward_completed_apprisal(args, &intake_opts, incoming.as_ref())
        && let Some(completed) = incoming
    {
        telemetry.finish_pipeline();
        args.io.write_output_caseload(&completed)?;
        let wm = pre_populated_watermarks(threshold);
        render_apprise_output(args, &completed, &wm, stdout)?;
        return Ok(completed);
    }

    enforce_dual_input_guard(&intake_opts, &args.canons, incoming.as_ref())?;

    let (caseload_after_intake, intake_disp, target_changed) =
        resolve_apprise_intake(&intake_opts, incoming, telemetry)?;

    let snapshot = materialize_snapshot(&backend, &args.canons.canon, &caseload_after_intake)?;
    let total_workspace_canons = Some(snapshot.canons.len());

    let (caseload_after_discover, discover_disp) = resolve_apprise_discover(
        &args.canons,
        caseload_after_intake,
        &snapshot,
        target_changed,
        telemetry,
    )?;

    if caseload_after_discover
        .discovery
        .as_ref()
        .is_some_and(|d| d.candidate_canons.is_empty())
    {
        telemetry.finish_pipeline();
        return finish_zero_candidate_apprisal(
            args,
            caseload_after_discover,
            intake_disp,
            total_workspace_canons,
            threshold,
            stdout,
        );
    }

    let (caseload_after_validate, validate_disp) =
        resolve_apprise_validate(caseload_after_discover, &snapshot, telemetry)?;

    if caseload_after_validate
        .validation
        .as_ref()
        .is_some_and(|v| v.has_errors)
    {
        telemetry.finish_pipeline();
        return handle_validation_failure(
            args,
            &caseload_after_validate,
            intake_disp,
            discover_disp,
            total_workspace_canons,
            threshold,
            stdout,
        );
    }

    let (final_caseload, configure_disp) = execute_jit_configure_and_apprise(
        &args.provider,
        env,
        threshold,
        caseload_after_validate,
        &snapshot,
        &backend,
        telemetry,
    )?;
    telemetry.finish_pipeline();

    args.io.write_output_caseload(&final_caseload)?;
    let wm = AppriseWatermarks {
        intake: intake_disp,
        discover: discover_disp,
        validate: validate_disp,
        configure: configure_disp,
        apprise: StageDisposition::Executed,
        total_workspace_canons,
        threshold,
    };
    render_apprise_output(args, &final_caseload, &wm, stdout)?;
    Ok(final_caseload)
}

fn finish_zero_candidate_apprisal(
    args: &AppriseArgs,
    caseload_after_discover: Caseload,
    intake_disp: StageDisposition,
    total_workspace_canons: Option<usize>,
    threshold: f64,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    let short_circuited = caseload_after_discover.with_apprisal(CaseloadApprisal::default());
    args.io.write_output_caseload(&short_circuited)?;
    let wm = AppriseWatermarks {
        intake: intake_disp,
        discover: StageDisposition::ZeroCandidateShortCircuit,
        validate: StageDisposition::SkippedZeroCandidates,
        configure: StageDisposition::SkippedZeroCandidates,
        apprise: StageDisposition::SkippedZeroCandidates,
        total_workspace_canons,
        threshold,
    };
    render_apprise_output(args, &short_circuited, &wm, stdout)?;
    Ok(short_circuited)
}

fn pre_populated_watermarks(threshold: f64) -> AppriseWatermarks {
    AppriseWatermarks {
        intake: StageDisposition::PrePopulated,
        discover: StageDisposition::PrePopulated,
        validate: StageDisposition::PrePopulated,
        configure: StageDisposition::PrePopulated,
        apprise: StageDisposition::PrePopulated,
        total_workspace_canons: None,
        threshold,
    }
}

fn handle_validation_failure(
    args: &AppriseArgs,
    caseload: &Caseload,
    intake_disp: StageDisposition,
    discover_disp: StageDisposition,
    total_workspace_canons: Option<usize>,
    threshold: f64,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    args.io.write_output_caseload(caseload)?;
    let wm = AppriseWatermarks {
        intake: intake_disp,
        discover: discover_disp,
        validate: StageDisposition::ValidationFailed,
        configure: StageDisposition::SkippedValidationFailed,
        apprise: StageDisposition::SkippedValidationFailed,
        total_workspace_canons,
        threshold,
    };
    if args.io.json || args.format != AppriseFormat::Compact {
        render_apprise_output(args, caseload, &wm, stdout)?;
    }

    let total_errors: usize = caseload
        .validation
        .as_ref()
        .map_or(1, |v| v.results.values().map(|m| m.error_count).sum());
    Err(CliError::runtime(format!(
        "Candidate canon validation failed with {total_errors} error(s); aborting `apprise` before model spend.\n  \
         Hint: Run `canon-clerk validate` to inspect and repair malformed candidate canons."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apprise::ENV_APPRISE_THRESHOLD;
    use crate::canon::ParseCanonOptions;
    use crate::caseload::{
        ApprisalStatus, CanonApprisalAssessment, CaseloadConfig, CaseloadDiscovery, CaseloadIntake,
        DEFAULT_SCREENER_MODEL,
    };
    use crate::cli::{Cli, Commands};
    use crate::configure::ENV_GEMINI_API_KEY;
    use crate::provider::{GenerationRequest, GenerationResponse, ProviderError, TokenUsage};
    use clap::Parser;
    use serde_json::{Value, json};
    use std::io::Cursor;
    use std::sync::Mutex;

    struct StubProvider {
        calls: Mutex<usize>,
        response: Result<Value, ProviderError>,
        thoughts: Vec<String>,
    }

    impl StubProvider {
        fn succeeding(payload: Value) -> Self {
            Self {
                calls: Mutex::new(0),
                response: Ok(payload),
                thoughts: Vec::new(),
            }
        }

        fn succeeding_with_thoughts(payload: Value, thoughts: Vec<String>) -> Self {
            Self {
                calls: Mutex::new(0),
                response: Ok(payload),
                thoughts,
            }
        }

        fn failing(err: ProviderError) -> Self {
            Self {
                calls: Mutex::new(0),
                response: Err(err),
                thoughts: Vec::new(),
            }
        }

        fn call_count(&self) -> usize {
            *self.calls.lock().expect("lock calls")
        }
    }

    impl StructuredModelProvider for StubProvider {
        fn generate_structured(
            &self,
            _request: &GenerationRequest,
        ) -> Result<GenerationResponse, ProviderError> {
            *self.calls.lock().expect("lock calls") += 1;
            match &self.response {
                Ok(payload) => Ok(GenerationResponse {
                    payload: payload.clone(),
                    resolved_model: "gemini-2.5-flash-lite".to_string(),
                    latency_ms: 82,
                    usage: TokenUsage {
                        prompt_tokens: 320,
                        completion_tokens: 74,
                        reasoning_tokens: 0,
                        total_tokens: 394,
                    },
                    thoughts: self.thoughts.clone(),
                }),
                Err(err) => Err(err.clone()),
            }
        }
    }

    fn test_env_with_key() -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        env.insert(
            ENV_GEMINI_API_KEY.to_string(),
            "synthetic-apprise-cli-test-key".to_string(),
        );
        env
    }

    fn valid_snapshot() -> WorkspaceSnapshot {
        let auth_canon = "---\ntriggers:\n  - \"src/auth/**\"\ntags:\n  - auth\n---\nAuthentication caches MUST define an explicit TTL expiration window.\n\nRationale: Prevents unbounded stale token retention across provider rotations.\n";
        let cli_canon = "---\ntriggers:\n  - \"src/cli/**\"\ntags:\n  - cli\n---\nCLI boolean flags MUST use positive polarity.\n\nRationale: Eliminates double-negative operator confusion.\n";
        WorkspaceSnapshot::new()
            .with_file("src/auth/login.rs")
            .with_file("src/cli/app.rs")
            .with_canon_markdown(
                auth_canon,
                &ParseCanonOptions::with_source_ref(
                    ".canons/auth/auth-caches-must-define-ttl-window.md",
                ),
            )
            .with_canon_markdown(
                cli_canon,
                &ParseCanonOptions::with_source_ref(
                    ".canons/cli/cli-flags-must-use-positive-polarity.md",
                ),
            )
    }

    mod cli_argument_parsing_and_threshold_guards {
        use super::*;

        #[test]
        fn test_parses_apprise_subcommand_flags() {
            let cli = Cli::try_parse_from([
                "canon-clerk",
                "apprise",
                "--intent",
                "Rotate auth provider",
                "src/auth",
                "--apprise-threshold",
                "0.65",
                "--screener-model",
                "flash-lite",
                "--format",
                "compact",
            ])
            .expect("parse apprise subcommand");

            match cli.command {
                Some(Commands::Apprise(args)) => {
                    assert_eq!(args.filing.intent.as_deref(), Some("Rotate auth provider"));
                    assert_eq!(args.filing.targets, vec!["src/auth".to_string()]);
                    assert_eq!(args.apprise_threshold, Some(0.65));
                    assert_eq!(args.provider.screener_model.as_deref(), Some("flash-lite"));
                    assert_eq!(args.format, AppriseFormat::Compact);
                }
                _ => panic!("expected Apprise subcommand"),
            }
        }

        #[test]
        fn test_rejects_unqualified_and_out_of_closure_threshold_flags_with_exit_2() {
            let snapshot = valid_snapshot();
            let provider = StubProvider::succeeding(json!({"assessments": {}}));

            for bad_flag in ["--threshold", "--docket-threshold", "--admit-threshold"] {
                let cli = Cli::try_parse_from([
                    "canon-clerk",
                    "apprise",
                    "--intent",
                    "Test",
                    "src/auth",
                    bad_flag,
                    "0.7",
                ])
                .expect("parse hidden rejected flag");
                let Some(Commands::Apprise(args)) = cli.command else {
                    panic!("expected Apprise");
                };
                let mut stdin = Cursor::new(b"");
                let mut stdout = Vec::new();
                let err = run_apprise_with_snapshot_and_provider(
                    &args,
                    &mut stdin,
                    &mut stdout,
                    &snapshot,
                    &test_env_with_key(),
                    &provider,
                )
                .expect_err("must reject invalid threshold flag");
                assert_eq!(err.exit_code(), 2);
                assert!(
                    err.to_string().contains("--apprise-threshold"),
                    "error for {bad_flag} must point to --apprise-threshold, got: {err}"
                );
            }
        }

        #[test]
        fn test_rejects_out_of_range_and_malformed_thresholds_via_flag_or_env() {
            let snapshot = valid_snapshot();
            let provider = StubProvider::succeeding(json!({"assessments": {}}));

            for bad_val in [-0.1, 1.5, f64::NAN, f64::INFINITY] {
                let args = AppriseArgs {
                    filing: FilingSourceArgs {
                        intent: Some("Test".to_string()),
                        targets: vec!["src/auth".to_string()],
                        ..FilingSourceArgs::default()
                    },
                    apprise_threshold: Some(bad_val),
                    ..AppriseArgs::default()
                };
                let err = run_apprise_with_snapshot_and_provider(
                    &args,
                    &mut Cursor::new(b""),
                    &mut Vec::new(),
                    &snapshot,
                    &test_env_with_key(),
                    &provider,
                )
                .expect_err("out-of-range flag threshold must fail");
                assert_eq!(err.exit_code(), 2);
                assert!(err.to_string().contains("Hint:"));
            }

            let mut bad_env = test_env_with_key();
            bad_env.insert(ENV_APPRISE_THRESHOLD.to_string(), "not-a-float".to_string());
            let args = AppriseArgs {
                filing: FilingSourceArgs {
                    intent: Some("Test".to_string()),
                    targets: vec!["src/auth".to_string()],
                    ..FilingSourceArgs::default()
                },
                ..AppriseArgs::default()
            };
            let env_err = run_apprise_with_snapshot_and_provider(
                &args,
                &mut Cursor::new(b""),
                &mut Vec::new(),
                &snapshot,
                &bad_env,
                &provider,
            )
            .expect_err("malformed env threshold must fail");
            assert_eq!(env_err.exit_code(), 2);
            assert!(env_err.to_string().contains(ENV_APPRISE_THRESHOLD));
        }
    }

    mod dual_input_guard_and_lazy_scheduling {
        use super::*;

        #[test]
        fn test_dual_input_guard_fails_with_exit_2_when_intent_or_scope_missing() {
            let snapshot = valid_snapshot();
            let provider = StubProvider::succeeding(json!({"assessments": {}}));

            // 1. Naked invocation (missing both)
            let naked_err = run_apprise_with_snapshot_and_provider(
                &AppriseArgs::default(),
                &mut Cursor::new(b""),
                &mut Vec::new(),
                &snapshot,
                &BTreeMap::new(),
                &provider,
            )
            .expect_err("naked apprise must fail");
            assert_eq!(naked_err.exit_code(), 2);
            assert!(naked_err.to_string().contains("Hint:"));

            // 2. Intent WITHOUT target or canon scope (prevents silent 0-target runs!)
            let intent_only = AppriseArgs {
                filing: FilingSourceArgs {
                    intent: Some("Add caching layer".to_string()),
                    ..FilingSourceArgs::default()
                },
                ..AppriseArgs::default()
            };
            let scope_err = run_apprise_with_snapshot_and_provider(
                &intent_only,
                &mut Cursor::new(b""),
                &mut Vec::new(),
                &snapshot,
                &BTreeMap::new(),
                &provider,
            )
            .expect_err("intent without scope must fail");
            assert_eq!(scope_err.exit_code(), 2);
            assert!(scope_err.to_string().contains("target or canon scope"));

            // 3. Target paths WITHOUT intent
            let targets_only = AppriseArgs {
                filing: FilingSourceArgs {
                    targets: vec!["src/auth".to_string()],
                    ..FilingSourceArgs::default()
                },
                ..AppriseArgs::default()
            };
            let intent_err = run_apprise_with_snapshot_and_provider(
                &targets_only,
                &mut Cursor::new(b""),
                &mut Vec::new(),
                &snapshot,
                &BTreeMap::new(),
                &provider,
            )
            .expect_err("targets without intent must fail");
            assert_eq!(intent_err.exit_code(), 2);
            assert!(intent_err.to_string().contains("--intent"));

            // 4. Blank whitespace intent with targets
            let blank_intent = AppriseArgs {
                filing: FilingSourceArgs {
                    intent: Some("   ".to_string()),
                    targets: vec!["src/auth".to_string()],
                    ..FilingSourceArgs::default()
                },
                ..AppriseArgs::default()
            };
            let blank_err = run_apprise_with_snapshot_and_provider(
                &blank_intent,
                &mut Cursor::new(b""),
                &mut Vec::new(),
                &snapshot,
                &BTreeMap::new(),
                &provider,
            )
            .expect_err("blank intent must fail");
            assert_eq!(blank_err.exit_code(), 2);
            assert_eq!(provider.call_count(), 0);
        }

        #[test]
        fn test_zero_candidate_run_exits_0_without_gemini_api_key_or_validate() {
            // Add a malformed canon outside the triggered path so we prove `validate` is ALSO skipped on 0 candidates!
            let broken_docs_canon = "---\ntriggers:\n  - \"docs/**\"\n---\nlowercase must keyword violates rfc 2119 lint.\n";
            let snapshot = valid_snapshot().with_canon_markdown(
                broken_docs_canon,
                &ParseCanonOptions::with_source_ref(".canons/docs/broken-canon.md"),
            );
            let provider = StubProvider::failing(ProviderError::MissingApiKey);

            let args = AppriseArgs {
                filing: FilingSourceArgs {
                    intent: Some("Add README typo fix in untriggered scripts dir".to_string()),
                    targets: vec!["scripts/release.sh".to_string()],
                    ..FilingSourceArgs::default()
                },
                ..AppriseArgs::default()
            };
            let mut stdout = Vec::new();

            // Notice empty env: NO GEMINI_API_KEY!
            let caseload = run_apprise_with_snapshot_and_provider(
                &args,
                &mut Cursor::new(b""),
                &mut stdout,
                &snapshot,
                &BTreeMap::new(),
                &provider,
            )
            .expect("0-candidate apprisal must succeed with exit 0 and zero credentials");

            assert_eq!(provider.call_count(), 0);
            assert!(caseload.validation.is_none(), "validate must be skipped");
            assert!(caseload.config.is_none(), "configure must be skipped");
            assert_eq!(
                caseload.apprisal,
                Some(CaseloadApprisal {
                    assessments: BTreeMap::new()
                })
            );

            let rendered = String::from_utf8(stdout).expect("utf8");
            assert!(rendered.contains("Pipeline Provenance:"));
            assert!(rendered.contains("0 candidate canons matched"));
            assert!(rendered.contains("(fast-forward: zero candidate canons)"));
        }

        #[test]
        fn test_invalid_candidate_canon_exits_1_before_checking_gemini_api_key() {
            let malformed_canon = "---\ntriggers:\n  - \"src/auth/**\"\nunknown_key: true\n---\nAuth caches must expire.\n";
            let snapshot = WorkspaceSnapshot::new()
                .with_file("src/auth/login.rs")
                .with_canon_markdown(
                    malformed_canon,
                    &ParseCanonOptions::with_source_ref(".canons/auth/bad-canon.md"),
                );
            let provider = StubProvider::failing(ProviderError::MissingApiKey);

            let args = AppriseArgs {
                filing: FilingSourceArgs {
                    intent: Some("Refactor auth cache".to_string()),
                    targets: vec!["src/auth".to_string()],
                    ..FilingSourceArgs::default()
                },
                ..AppriseArgs::default()
            };
            let mut stdout = Vec::new();

            // Empty env (no GEMINI_API_KEY) — must still exit 1 on validation error, NOT exit 2!
            let err = run_apprise_with_snapshot_and_provider(
                &args,
                &mut Cursor::new(b""),
                &mut stdout,
                &snapshot,
                &BTreeMap::new(),
                &provider,
            )
            .expect_err("malformed candidate canon must fail with exit 1");

            assert_eq!(err.exit_code(), 1);
            assert!(err.to_string().contains("Hint:"));
            assert_eq!(provider.call_count(), 0);

            let rendered = String::from_utf8(stdout).expect("utf8");
            assert!(rendered.contains("✗ validate"));
            assert!(rendered.contains("(fast-forward: canon validation failed)"));
        }
    }

    mod execution_formats_and_fast_forwarding {
        use super::*;

        fn canned_assessments_payload() -> Value {
            json!({
                "assessments": {
                    ".canons/auth/auth-caches-must-define-ttl-window.md": {
                        "apprisal_summary": "Planned provider cache requires an explicit TTL window.",
                        "apprisal_score": 0.93
                    },
                    ".canons/cli/cli-flags-must-use-positive-polarity.md": {
                        "apprisal_summary": "No CLI flags are introduced or modified.",
                        "apprisal_score": 0.12
                    }
                }
            })
        }

        #[test]
        fn test_stylish_compact_and_json_formats_with_stdin_targets_and_env_threshold() {
            let snapshot = valid_snapshot();
            let provider = StubProvider::succeeding(canned_assessments_payload());
            let mut env = test_env_with_key();
            env.insert(ENV_APPRISE_THRESHOLD.to_string(), "0.60".to_string());

            // 1. Stylish format reading newline-delimited target paths from positional '-'
            let stylish_args = AppriseArgs {
                filing: FilingSourceArgs {
                    intent: Some("Add auth token cache".to_string()),
                    targets: vec!["-".to_string()],
                    ..FilingSourceArgs::default()
                },
                format: AppriseFormat::Stylish,
                ..AppriseArgs::default()
            };
            let mut stylish_out = Vec::new();
            let caseload = run_apprise_with_snapshot_and_provider(
                &stylish_args,
                &mut Cursor::new(b"src/auth/login.rs\nsrc/cli/app.rs\n"),
                &mut stylish_out,
                &snapshot,
                &env,
                &provider,
            )
            .expect("stylish apprise");

            let stylish_text = String::from_utf8(stylish_out).expect("utf8");
            assert!(stylish_text.contains("Pipeline Provenance:"));
            assert!(stylish_text.contains("✓ intake"));
            assert!(stylish_text.contains("✓ discover"));
            assert!(stylish_text.contains("✓ validate"));
            assert!(stylish_text.contains("✓ configure"));
            assert!(
                stylish_text
                    .contains("✓ apprise   : 1 canon(s) applicable (1 dismissed, threshold 0.60)")
            );
            assert!(stylish_text.contains("Statutory Apprisal (1 applicable, 1 dismissed):"));
            assert!(
                stylish_text
                    .contains(".canons/auth/auth-caches-must-define-ttl-window.md [score: 0.93]")
            );
            assert!(!stylish_text.contains("cli-flags-must-use-positive-polarity.md [score:"));

            // 2. Compact format (`<canon>\t<status>\t<score>\t<summary>`)
            let compact_args = AppriseArgs {
                filing: FilingSourceArgs {
                    intent: Some("Add auth token cache".to_string()),
                    targets: vec![
                        "src/auth/login.rs".to_string(),
                        "src/cli/app.rs".to_string(),
                    ],
                    ..FilingSourceArgs::default()
                },
                format: AppriseFormat::Compact,
                ..AppriseArgs::default()
            };
            let mut compact_out = Vec::new();
            run_apprise_with_snapshot_and_provider(
                &compact_args,
                &mut Cursor::new(b""),
                &mut compact_out,
                &snapshot,
                &env,
                &provider,
            )
            .expect("compact apprise");
            let compact_text = String::from_utf8(compact_out).expect("utf8");
            let lines: Vec<&str> = compact_text.lines().collect();
            assert_eq!(lines.len(), 2);
            assert_eq!(
                lines[0],
                ".canons/auth/auth-caches-must-define-ttl-window.md\tapplicable\t0.93\tPlanned provider cache requires an explicit TTL window."
            );
            assert_eq!(
                lines[1],
                ".canons/cli/cli-flags-must-use-positive-polarity.md\tdismissed\t0.12\tNo CLI flags are introduced or modified."
            );

            // 3. JSON output via `--json`
            let json_args = AppriseArgs {
                io: CaseloadIoArgs {
                    json: true,
                    ..CaseloadIoArgs::default()
                },
                ..compact_args
            };
            let mut json_out = Vec::new();
            run_apprise_with_snapshot_and_provider(
                &json_args,
                &mut Cursor::new(b""),
                &mut json_out,
                &snapshot,
                &env,
                &provider,
            )
            .expect("json apprise");
            let decoded: Caseload = serde_json::from_slice(&json_out).expect("valid Caseload JSON");
            assert_eq!(decoded, caseload);
        }

        #[test]
        fn test_caseload_fast_forwarding_and_provider_error_mapping() {
            let snapshot = valid_snapshot();

            // A. Pre-populated `.apprisal` fast-forwards completely without credentials or model calls
            let mut assessments = BTreeMap::new();
            assessments.insert(
                ".canons/auth/auth-caches-must-define-ttl-window.md".to_string(),
                CanonApprisalAssessment {
                    apprisal_summary: "Cached upstream.".to_string(),
                    apprisal_score: 0.88,
                    apprisal_status: ApprisalStatus::Applicable,
                },
            );
            let completed_upstream = Caseload::new()
                .with_intake(CaseloadIntake {
                    intent: Some("Cached intent".to_string()),
                    target_paths: vec!["src/auth".to_string()],
                    ..CaseloadIntake::default()
                })
                .with_discovery(CaseloadDiscovery {
                    candidate_canons: vec![
                        ".canons/auth/auth-caches-must-define-ttl-window.md".to_string(),
                    ],
                    ..CaseloadDiscovery::default()
                })
                .with_config(CaseloadConfig::default())
                .with_apprisal(CaseloadApprisal { assessments });
            let raw_completed =
                serde_json::to_vec(&completed_upstream).expect("serialize upstream");

            let ff_provider = StubProvider::failing(ProviderError::MissingApiKey);
            let ff_args = AppriseArgs {
                io: CaseloadIoArgs {
                    caseload: Some("-".to_string()),
                    json: true,
                    ..CaseloadIoArgs::default()
                },
                ..AppriseArgs::default()
            };
            let ff_result = run_apprise_with_snapshot_and_provider(
                &ff_args,
                &mut Cursor::new(raw_completed),
                &mut Vec::new(),
                &snapshot,
                &BTreeMap::new(),
                &ff_provider,
            )
            .expect("pre-populated apprisal must fast-forward without API key");
            assert_eq!(ff_result, completed_upstream);
            assert_eq!(ff_provider.call_count(), 0);

            // B. Missing credentials or upstream provider errors map to exit code 2
            let active_args = AppriseArgs {
                filing: FilingSourceArgs {
                    intent: Some("Rotate auth provider".to_string()),
                    targets: vec!["src/auth".to_string()],
                    ..FilingSourceArgs::default()
                },
                ..AppriseArgs::default()
            };
            let no_key_err = run_apprise_with_snapshot_and_provider(
                &active_args,
                &mut Cursor::new(b""),
                &mut Vec::new(),
                &snapshot,
                &BTreeMap::new(),
                &ff_provider,
            )
            .expect_err("missing GEMINI_API_KEY when candidates exist must exit 2");
            assert_eq!(no_key_err.exit_code(), 2);
            assert!(no_key_err.to_string().contains(ENV_GEMINI_API_KEY));

            let net_fail_provider = StubProvider::failing(ProviderError::Timeout {
                model_specifier: DEFAULT_SCREENER_MODEL.to_string(),
                timeout_ms: 60_000,
            });
            let provider_err = run_apprise_with_snapshot_and_provider(
                &active_args,
                &mut Cursor::new(b""),
                &mut Vec::new(),
                &snapshot,
                &test_env_with_key(),
                &net_fail_provider,
            )
            .expect_err("provider timeout must map to exit 2");
            assert_eq!(provider_err.exit_code(), 2);
            assert!(provider_err.to_string().contains("timed out"));
        }
    }

    mod telemetry_instrumentation {
        use super::*;
        use crate::caseload::CaseloadValidation;
        use crate::telemetry::{
            PipelineStage, RecordingTelemetrySink, SteppingTelemetryClock, TelemetryEvent,
        };

        #[test]
        fn test_full_apprise_closure_emits_ordered_events_thoughts_tokens_and_preserves_caseload() {
            let snapshot = valid_snapshot();
            let env = test_env_with_key();
            let payload = json!({
                "assessments": {
                    ".canons/auth/auth-caches-must-define-ttl-window.md": {
                        "apprisal_summary": "Planned provider cache requires TTL.",
                        "apprisal_score": 0.91
                    }
                }
            });
            let provider = StubProvider::succeeding_with_thoughts(
                payload,
                vec![
                    "Inspecting auth cache TTL canon.".to_string(),
                    "Jurisdiction confirmed.".to_string(),
                ],
            );

            let args = AppriseArgs {
                filing: FilingSourceArgs {
                    intent: Some("Add auth token cache".to_string()),
                    targets: vec!["src/auth/login.rs".to_string()],
                    ..FilingSourceArgs::default()
                },
                io: CaseloadIoArgs {
                    json: true,
                    ..CaseloadIoArgs::default()
                },
                ..AppriseArgs::default()
            };

            let mut baseline_stdout = Vec::new();
            run_apprise_with_snapshot_and_provider(
                &args,
                &mut Cursor::new(b""),
                &mut baseline_stdout,
                &snapshot,
                &env,
                &provider,
            )
            .expect("baseline apprise");

            let sink = RecordingTelemetrySink::new();
            let clock = SteppingTelemetryClock::new(1_767_225_600_000, 10);
            let session = TelemetrySession::new(&sink, &clock);
            let mut instrumented_stdout = Vec::new();
            run_apprise_with_telemetry(
                &args,
                &mut Cursor::new(b""),
                &mut instrumented_stdout,
                &snapshot,
                &env,
                &provider,
                &session,
            )
            .expect("instrumented apprise");

            // Substantive Caseload JSON on stdout MUST be bit-for-bit identical and free of telemetry
            assert_eq!(instrumented_stdout, baseline_stdout);
            let json_text = String::from_utf8(instrumented_stdout).expect("utf8 json");
            assert!(!json_text.contains("duration_ms"));
            assert!(!json_text.contains("total_tokens"));
            assert!(!json_text.contains("timestamp"));

            let events = sink.events();
            let stages_and_kinds: Vec<String> = events
                .iter()
                .map(|ev| match ev {
                    TelemetryEvent::StageStart { stage, .. } => format!("start:{stage}"),
                    TelemetryEvent::Thought { stage, delta } => format!("thought:{stage}:{delta}"),
                    TelemetryEvent::StageFinish { stage, usage, .. } => {
                        format!("finish:{stage}:{}", usage.map_or(0, |u| u.total_tokens))
                    }
                    TelemetryEvent::PipelineFinish { total_tokens, .. } => {
                        format!("pipeline:{total_tokens}")
                    }
                })
                .collect();

            assert_eq!(
                stages_and_kinds,
                vec![
                    "start:intake",
                    "finish:intake:0",
                    "start:discover",
                    "finish:discover:0",
                    "start:validate",
                    "finish:validate:0",
                    "start:configure",
                    "finish:configure:0",
                    "start:apprise",
                    "thought:apprise:Inspecting auth cache TTL canon.",
                    "thought:apprise:Jurisdiction confirmed.",
                    "finish:apprise:394",
                    "pipeline:394",
                ]
            );
        }

        #[test]
        fn test_fast_forwarded_stages_are_omitted_from_telemetry() {
            let snapshot = valid_snapshot();
            let env = test_env_with_key();
            let provider = StubProvider::succeeding(json!({
                "assessments": {
                    ".canons/auth/auth-caches-must-define-ttl-window.md": {
                        "apprisal_summary": "Applicable.",
                        "apprisal_score": 0.85
                    }
                }
            }));

            let upstream = Caseload::new()
                .with_intake(CaseloadIntake {
                    intent: Some("Cached intent".to_string()),
                    target_paths: vec!["src/auth/login.rs".to_string()],
                    ..CaseloadIntake::default()
                })
                .with_discovery(CaseloadDiscovery {
                    candidate_canons: vec![
                        ".canons/auth/auth-caches-must-define-ttl-window.md".to_string(),
                    ],
                    ..CaseloadDiscovery::default()
                })
                .with_validation(CaseloadValidation::default())
                .with_config(CaseloadConfig::default());
            let raw_upstream = serde_json::to_vec(&upstream).expect("serialize upstream");

            let ff_sink = RecordingTelemetrySink::new();
            let ff_clock = SteppingTelemetryClock::new(1_767_225_600_000, 5);
            let ff_session = TelemetrySession::new(&ff_sink, &ff_clock);
            let ff_args = AppriseArgs {
                io: CaseloadIoArgs {
                    caseload: Some("-".to_string()),
                    json: true,
                    ..CaseloadIoArgs::default()
                },
                ..AppriseArgs::default()
            };
            run_apprise_with_telemetry(
                &ff_args,
                &mut Cursor::new(raw_upstream),
                &mut Vec::new(),
                &snapshot,
                &env,
                &provider,
                &ff_session,
            )
            .expect("fast-forwarded upstream stages");

            let ff_events = ff_sink.events();
            assert_eq!(ff_events.len(), 3);
            assert!(matches!(
                ff_events[0],
                TelemetryEvent::StageStart {
                    stage: PipelineStage::Apprise,
                    ..
                }
            ));
            assert!(matches!(
                ff_events[1],
                TelemetryEvent::StageFinish {
                    stage: PipelineStage::Apprise,
                    usage: Some(u),
                    ..
                } if u.total_tokens == 394
            ));
            assert!(matches!(
                ff_events[2],
                TelemetryEvent::PipelineFinish {
                    total_tokens: 394,
                    ..
                }
            ));
        }

        #[test]
        fn test_zero_candidate_short_circuit_omits_downstream_stages_from_telemetry() {
            let snapshot = valid_snapshot();
            let provider = StubProvider::failing(ProviderError::MissingApiKey);
            let sc_sink = RecordingTelemetrySink::new();
            let sc_clock = SteppingTelemetryClock::new(1_767_225_600_000, 5);
            let sc_session = TelemetrySession::new(&sc_sink, &sc_clock);
            let sc_args = AppriseArgs {
                filing: FilingSourceArgs {
                    intent: Some("Update docs".to_string()),
                    targets: vec!["docs/readme.md".to_string()],
                    ..FilingSourceArgs::default()
                },
                ..AppriseArgs::default()
            };
            run_apprise_with_telemetry(
                &sc_args,
                &mut Cursor::new(b""),
                &mut Vec::new(),
                &snapshot,
                &BTreeMap::new(),
                &provider,
                &sc_session,
            )
            .expect("zero-candidate short-circuit");

            let sc_stages: Vec<PipelineStage> = sc_sink
                .events()
                .into_iter()
                .filter_map(|e| match e {
                    TelemetryEvent::StageStart { stage, .. } => Some(stage),
                    _ => None,
                })
                .collect();
            assert_eq!(
                sc_stages,
                vec![PipelineStage::Intake, PipelineStage::Discover]
            );
        }
    }
}
