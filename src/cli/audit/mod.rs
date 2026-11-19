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

//! CLI driving adapter for the `canon-clerk audit` subcommand (`docs/architecture/nodes/audit.md` §5).

pub mod guards;
mod render;
mod stages;

use crate::caseload::{Caseload, CaseloadDocket, CaseloadEvidence, CaseloadVerdict, VerdictStatus};
use crate::cli::CliError;
use crate::cli::args::{
    CanonSelectionArgs, CaseloadIoArgs, FilingSourceArgs, ProviderConfigArgs, ProviderTransportArgs,
};
use crate::cli::closure::{
    ClosureBackend, materialize_closure_snapshot, resolve_cascade_discover, resolve_cascade_intake,
    resolve_closure_validate,
};
use crate::cli::io::ensure_single_stdin_source;
use crate::cli::node_args::{AuditNodeModelArgs, RejectedUnqualifiedTuningArgs};
use crate::cli::provenance::StageDisposition;
use crate::discover::WorkspaceSnapshot;
use crate::provider::StructuredModelProvider;
use crate::telemetry::TelemetrySession;
use clap::{Args, ValueEnum};
use guards::{
    can_fast_forward_completed_audit, enforce_filing_and_case_guard,
    ensure_stage_prefixed_threshold_flags, resolve_and_validate_audit_cli_params,
};
use render::{AuditWatermarks, render_audit_output, resolve_effective_format};
use stages::{AuditCascadeThresholds, execute_jit_configure_docket_admit_and_audit};
use std::collections::BTreeMap;
use std::env;
use std::io::{Read, Write};

const AUDIT_EXAMPLES: &str = "\
Examples:
  # Audit a unified diff piped on standard input across the full Caseload cascade:
  git diff origin/main | canon-clerk audit --diff -

  # Audit explicit target paths or newline-delimited stdin paths:
  canon-clerk audit src/cli src/audit
  git diff origin/main --name-only | canon-clerk audit -

  # Adjudicate an upstream admitted Caseload and emit JSON:
  canon-clerk audit --caseload caseload-6.json --json

  # Run in GitHub Actions annotation mode while archiving caseload.json for CI forensics:
  git diff origin/main | canon-clerk audit --diff - --format github --output-caseload caseload.json

  # Use TypeSafe Jev across docket, admit, and vet before escalating contested cases to Gemini Carbon:
  git diff origin/main | canon-clerk audit --diff - --provider typesafe --audit-model google:gemini-4-carbon";

/// Output presentation format for `canon-clerk audit`.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditFormat {
    /// Human-readable 8-stage Pipeline Funnel Provenance receipt, Verdict summary, and failing decrees.
    Stylish,
    /// Full cumulative `Caseload` JSON envelope.
    Json,
    /// Tab-separated violation diagnostic lines for failing cases.
    Compact,
    /// GitHub Actions workflow command annotations (`::error file=...::...`) and `$GITHUB_STEP_SUMMARY` report.
    Github,
}

/// Command-line arguments for `canon-clerk audit`.
#[derive(Args, Debug, Clone, Default, PartialEq)]
#[command(
    about = "Judicial adjudication: evaluate admitted exhibits against canon statutes and render verdicts",
    long_about = "Judicial Adjudication stage of the Caseload DAG (Cascade Spine terminus). Executes the guarded [intake, discover, validate, configure, docket, admit, vet, audit] closure to evaluate contested cases in isolated per-case reasoning trials, rendering compliance decrees and source line annotations.",
    after_help = AUDIT_EXAMPLES
)]
pub struct AuditArgs {
    #[command(flatten)]
    pub filing: FilingSourceArgs,

    #[command(flatten)]
    pub canons: CanonSelectionArgs,

    /// Substantive compliance score threshold in `[0.0, 1.0]` (default: `0.5`; env: `CANON_CLERK_AUDIT_THRESHOLD`).
    #[arg(
        long = "audit-threshold",
        value_name = "FLOAT",
        allow_hyphen_values = true,
        help_heading = "Audit Options",
        help = "Substantive compliance threshold in [0.0, 1.0] (default: 0.5; fallback env: CANON_CLERK_AUDIT_THRESHOLD)"
    )]
    pub audit_threshold: Option<f64>,

    /// Optional maximum concurrent in-flight per-case trial calls (default: `4`; env: `CANON_CLERK_AUDIT_CONCURRENCY`).
    #[arg(
        long = "audit-concurrency",
        value_name = "COUNT",
        help_heading = "Audit Options",
        help = "Maximum concurrent in-flight per-case trial calls (default: 4; fallback env: CANON_CLERK_AUDIT_CONCURRENCY)"
    )]
    pub audit_concurrency: Option<usize>,

    /// Upstream prima facie compliance vetting threshold in `[0.0, 1.0]` (default: `0.80`; env: `CANON_CLERK_VET_THRESHOLD`).
    #[arg(
        long = "vet-threshold",
        value_name = "FLOAT",
        allow_hyphen_values = true,
        help_heading = "Vet Options",
        help = "Upstream prima facie compliance threshold in [0.0, 1.0] (default: 0.80; fallback env: CANON_CLERK_VET_THRESHOLD)"
    )]
    pub vet_threshold: Option<f64>,

    /// Upstream maximum concurrent in-flight per-case `vet` calls (default: `4`; env: `CANON_CLERK_VET_CONCURRENCY`).
    #[arg(
        long = "vet-concurrency",
        value_name = "COUNT",
        help_heading = "Vet Options",
        help = "Maximum concurrent upstream vet calls (default: 4; fallback env: CANON_CLERK_VET_CONCURRENCY)"
    )]
    pub vet_concurrency: Option<usize>,

    /// Upstream evidentiary admissibility score threshold in `[0.0, 1.0]` (default: `0.5`; env: `CANON_CLERK_ADMIT_THRESHOLD`).
    #[arg(
        long = "admit-threshold",
        value_name = "FLOAT",
        allow_hyphen_values = true,
        help_heading = "Admit Options",
        help = "Upstream evidentiary admissibility threshold in [0.0, 1.0] (default: 0.5; fallback env: CANON_CLERK_ADMIT_THRESHOLD)"
    )]
    pub admit_threshold: Option<f64>,

    /// Upstream maximum concurrent in-flight per-case `admit` calls (default: `4`; env: `CANON_CLERK_ADMIT_CONCURRENCY`).
    #[arg(
        long = "admit-concurrency",
        value_name = "COUNT",
        help_heading = "Admit Options",
        help = "Maximum concurrent upstream admit calls (default: 4; fallback env: CANON_CLERK_ADMIT_CONCURRENCY)"
    )]
    pub admit_concurrency: Option<usize>,

    /// Upstream subject-matter jurisdiction threshold in `[0.0, 1.0]` (default: `0.5`; env: `CANON_CLERK_DOCKET_THRESHOLD`).
    #[arg(
        long = "docket-threshold",
        value_name = "FLOAT",
        allow_hyphen_values = true,
        help_heading = "Docket Options",
        help = "Upstream macro jurisdiction threshold in [0.0, 1.0] (default: 0.5; fallback env: CANON_CLERK_DOCKET_THRESHOLD)"
    )]
    pub docket_threshold: Option<f64>,

    /// Upstream maximum candidate canons per `docket` call (default: `25`; env: `CANON_CLERK_DOCKET_BATCH_SIZE`).
    #[arg(
        long = "docket-batch-size",
        value_name = "COUNT",
        help_heading = "Docket Options",
        help = "Maximum candidate canons per upstream docket call (default: 25; fallback env: CANON_CLERK_DOCKET_BATCH_SIZE)"
    )]
    pub docket_batch_size: Option<usize>,

    /// Upstream maximum concurrent in-flight `docket` calls when batched (default: `4`; env: `CANON_CLERK_DOCKET_CONCURRENCY`).
    #[arg(
        long = "docket-concurrency",
        value_name = "COUNT",
        help_heading = "Docket Options",
        help = "Maximum concurrent upstream docket calls when batched (default: 4; fallback env: CANON_CLERK_DOCKET_CONCURRENCY)"
    )]
    pub docket_concurrency: Option<usize>,

    #[command(flatten)]
    pub rejected_unqualified: RejectedUnqualifiedTuningArgs,

    /// Hidden capture rejecting out-of-closure `--apprise-threshold` (`scheduling.md` §4 Pattern C).
    #[arg(long = "apprise-threshold", hide = true, allow_hyphen_values = true)]
    pub rejected_apprise_threshold: Option<String>,

    /// Hidden capture rejecting out-of-closure `--apprise-batch-size`.
    #[arg(long = "apprise-batch-size", hide = true, allow_hyphen_values = true)]
    pub rejected_apprise_batch_size: Option<String>,

    /// Hidden capture rejecting out-of-closure `--apprise-concurrency`.
    #[arg(long = "apprise-concurrency", hide = true, allow_hyphen_values = true)]
    pub rejected_apprise_concurrency: Option<String>,

    #[command(flatten)]
    pub models: AuditNodeModelArgs,

    #[command(flatten)]
    pub provider: ProviderConfigArgs,

    #[command(flatten)]
    pub transport: ProviderTransportArgs,

    /// Output format (`stylish`, `json`, `compact`, or `github`; defaults to `github` when `GITHUB_ACTIONS=true`).
    #[arg(
        long = "format",
        value_enum,
        help_heading = "Caseload & Output",
        help = "Output formatting choice (defaults to 'github' when GITHUB_ACTIONS=true, else 'stylish')"
    )]
    pub format: Option<AuditFormat>,

    #[command(flatten)]
    pub io: CaseloadIoArgs,

    /// Propagated top-level `-v` / `--verbose` flag (`CANON_CLERK_VERBOSE`).
    #[arg(skip)]
    pub verbose: bool,
}

impl Eq for AuditArgs {}

/// Executes `canon-clerk audit` hermetically with an explicit [`TelemetrySession`].
pub fn run_audit_with_telemetry(
    args: &AuditArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace: &WorkspaceSnapshot,
    env: &BTreeMap<String, String>,
    provider: &dyn StructuredModelProvider,
    telemetry: &TelemetrySession<'_>,
) -> Result<Caseload, CliError> {
    run_audit_core(
        args,
        stdin,
        stdout,
        env,
        ClosureBackend::Hermetic {
            workspace,
            provider,
        },
        telemetry,
    )
}

/// Executes `canon-clerk audit` hermetically against an in-memory [`WorkspaceSnapshot`],
/// explicit environment map, and hexagonal [`StructuredModelProvider`].
pub fn run_audit_with_snapshot_and_provider(
    args: &AuditArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace: &WorkspaceSnapshot,
    env: &BTreeMap<String, String>,
    provider: &dyn StructuredModelProvider,
) -> Result<Caseload, CliError> {
    run_audit_with_telemetry(
        args,
        stdin,
        stdout,
        workspace,
        env,
        provider,
        &TelemetrySession::noop(),
    )
}

/// Executes `canon-clerk audit` against the live working directory with an explicit [`TelemetrySession`].
pub fn run_audit_live_with_telemetry(
    args: &AuditArgs,
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
    run_audit_core(
        args,
        stdin,
        stdout,
        &env_map,
        ClosureBackend::LiveDisk {
            workspace_root: &cwd,
        },
        telemetry,
    )
}

/// Executes `canon-clerk audit` using the current working directory, process environment,
/// and live [`GeminiModelProvider`].
pub fn run_audit(
    args: &AuditArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    run_audit_live_with_telemetry(args, stdin, stdout, &TelemetrySession::noop())
}

fn run_audit_core(
    args: &AuditArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    env: &BTreeMap<String, String>,
    backend: ClosureBackend<'_>,
    telemetry: &TelemetrySession<'_>,
) -> Result<Caseload, CliError> {
    ensure_stage_prefixed_threshold_flags(args)?;
    args.provider.validate_thinking_flags()?;
    ensure_single_stdin_source(&args.filing, args.io.caseload.as_deref())?;
    let (thresholds, stored_config) = resolve_and_validate_audit_cli_params(args, env)?;

    let incoming = args.io.read_caseload(stdin)?;
    let intake_opts = args.filing.resolve_intake_options(stdin)?;

    if can_fast_forward_completed_audit(args, &intake_opts, incoming.as_ref())
        && let Some(completed) = incoming
    {
        telemetry.finish_pipeline();
        let wm = AuditWatermarks::pre_populated(
            thresholds.docket_threshold,
            thresholds.admit_threshold,
            thresholds.vet_threshold,
            thresholds.audit_threshold,
            completed.vetting.is_some(),
        );
        return finalize_verdict_and_exit_code(args, completed, &wm, env, stdout);
    }

    enforce_filing_and_case_guard(&intake_opts, &args.canons, incoming.as_ref())?;

    let (caseload_after_intake, intake_disp, intake_changed) =
        resolve_cascade_intake("audit", &intake_opts, incoming, telemetry)?;

    let mut snapshot =
        materialize_closure_snapshot(&backend, &args.canons.canon, &caseload_after_intake)?;
    let total_workspace_canons = Some(snapshot.canons.len());

    let (caseload_after_discover, discover_disp) = resolve_cascade_discover(
        &args.canons,
        caseload_after_intake,
        &snapshot,
        intake_changed,
        telemetry,
    )?;

    if caseload_after_discover
        .discovery
        .as_ref()
        .is_some_and(|d| d.candidate_canons.is_empty())
    {
        telemetry.finish_pipeline();
        return finish_zero_candidate_audit(
            args,
            caseload_after_discover,
            intake_disp,
            total_workspace_canons,
            thresholds,
            env,
            stdout,
        );
    }

    let (caseload_after_validate, validate_disp) =
        resolve_closure_validate(caseload_after_discover, &snapshot, telemetry)?;

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
            thresholds,
            env,
            stdout,
        );
    }

    let outcome = execute_jit_configure_docket_admit_and_audit(
        args,
        env,
        stored_config,
        thresholds,
        caseload_after_validate,
        &mut snapshot,
        &backend,
        telemetry,
    )?;
    telemetry.finish_pipeline();

    let wm = AuditWatermarks {
        intake: intake_disp,
        discover: discover_disp,
        validate: validate_disp,
        configure: outcome.configure_disp,
        docket: outcome.docket_disp,
        admit: outcome.admit_disp,
        vet: outcome.vet_disp,
        audit: outcome.audit_disp,
        total_workspace_canons,
        docket_threshold: thresholds.docket_threshold,
        admit_threshold: thresholds.admit_threshold,
        vet_threshold: thresholds.vet_threshold,
        audit_threshold: thresholds.audit_threshold,
    };
    finalize_verdict_and_exit_code(args, outcome.caseload, &wm, env, stdout)
}

fn finalize_verdict_and_exit_code(
    args: &AuditArgs,
    caseload: Caseload,
    wm: &AuditWatermarks,
    env: &BTreeMap<String, String>,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    args.io.write_output_caseload(&caseload)?;
    render_audit_output(args, &caseload, wm, env, stdout)?;
    if caseload
        .verdict
        .as_ref()
        .is_some_and(|v| v.status == VerdictStatus::Fail)
    {
        return Err(CliError::silent(1));
    }
    Ok(caseload)
}

fn finish_zero_candidate_audit(
    args: &AuditArgs,
    caseload_after_discover: Caseload,
    intake_disp: StageDisposition,
    total_workspace_canons: Option<usize>,
    thresholds: AuditCascadeThresholds,
    env: &BTreeMap<String, String>,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    let short_circuited = caseload_after_discover
        .with_docket(CaseloadDocket::default())
        .with_evidence(CaseloadEvidence::default())
        .with_verdict(CaseloadVerdict::default());
    args.io.write_output_caseload(&short_circuited)?;
    let wm = AuditWatermarks::zero_candidates(intake_disp, total_workspace_canons, thresholds);
    render_audit_output(args, &short_circuited, &wm, env, stdout)?;
    Ok(short_circuited)
}

#[allow(
    clippy::too_many_arguments,
    reason = "Internal CLI helper formatting validation failure provenance watermarks across the 8-stage audit closure"
)]
fn handle_validation_failure(
    args: &AuditArgs,
    caseload: &Caseload,
    intake_disp: StageDisposition,
    discover_disp: StageDisposition,
    total_workspace_canons: Option<usize>,
    thresholds: AuditCascadeThresholds,
    env: &BTreeMap<String, String>,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    args.io.write_output_caseload(caseload)?;
    let wm = AuditWatermarks::validation_failed(
        intake_disp,
        discover_disp,
        total_workspace_canons,
        thresholds,
    );
    if resolve_effective_format(args, env) != AuditFormat::Compact {
        render_audit_output(args, caseload, &wm, env, stdout)?;
    }

    let total_errors: usize = caseload
        .validation
        .as_ref()
        .map_or(1, |v| v.results.values().map(|m| m.error_count).sum());
    Err(CliError::runtime(format!(
        "Candidate canon validation failed with {total_errors} error(s); aborting `audit` before model spend.\n  \
         Hint: Run `canon-clerk validate` to inspect and repair malformed candidate canons."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::cli::{Cli, Commands};
    use crate::configure::ENV_GEMINI_API_KEY;
    use crate::provider::{GenerationRequest, GenerationResponse, ProviderError, TokenUsage};
    use crate::telemetry::{
        PipelineStage, RecordingTelemetrySink, SteppingTelemetryClock, TelemetryEvent,
    };
    use clap::{CommandFactory, Parser};
    use serde_json::{Value, json};
    use std::fs;
    use std::io::Cursor;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct FullCascadeProvider {
        calls: Mutex<usize>,
        docket_payload: Value,
        admit_payload: Value,
        audit_payload: Value,
    }

    impl FullCascadeProvider {
        fn new(docket_payload: Value, admit_payload: Value, audit_payload: Value) -> Self {
            Self {
                calls: Mutex::new(0),
                docket_payload,
                admit_payload,
                audit_payload,
            }
        }

        fn call_count(&self) -> usize {
            *self.calls.lock().expect("lock calls")
        }
    }

    impl StructuredModelProvider for FullCascadeProvider {
        fn generate_structured(
            &self,
            request: &GenerationRequest,
        ) -> Result<GenerationResponse, ProviderError> {
            *self.calls.lock().expect("lock calls") += 1;
            let payload = if request.system_instruction.contains("Macro Subject-Matter") {
                self.docket_payload.clone()
            } else if request
                .system_instruction
                .contains("Micro Evidentiary Admissibility")
            {
                self.admit_payload.clone()
            } else {
                self.audit_payload.clone()
            };
            Ok(GenerationResponse {
                payload,
                thoughts: vec!["Deliberating on canon compliance...".to_string()],
                usage: TokenUsage {
                    prompt_tokens: 250,
                    completion_tokens: 60,
                    reasoning_tokens: 90,
                    total_tokens: 400,
                },
                latency_ms: 80,
                resolved_model: "gemini-2.5-pro".to_string(),
            })
        }
    }

    fn sample_workspace() -> WorkspaceSnapshot {
        WorkspaceSnapshot::new()
            .with_canon_markdown(
                "---\ntriggers:\n  - \"src/cli/**\"\ninspect:\n  - diff\n  - pr_title?\n---\n# CLI Flags Kebab Case\n\nCLI flags MUST use kebab-case naming.\n",
                &ParseCanonOptions::with_source_ref(".canons/cli/cli-flags-kebab-case.md"),
            )
            .with_file_content(
                "src/cli/audit.rs",
                "pub struct AuditArgs {\n    #[arg(long = \"audit-threshold\")]\n    pub threshold: f64,\n}\n",
            )
    }

    fn parse_audit(argv: &[&str]) -> AuditArgs {
        let mut cli = Cli::try_parse_from(argv).expect("parse audit args");
        if let Some(cmd) = &mut cli.command {
            cmd.apply_global_verbose(cli.verbose);
        }
        match cli.command {
            Some(Commands::Audit(args)) => args,
            _ => panic!("expected Audit subcommand"),
        }
    }

    mod cli_flags_and_guards {
        use super::*;

        #[test]
        fn test_help_groups_and_stage_prefixed_flag_scoping() {
            let mut cmd = Cli::command();
            let sub = cmd.find_subcommand_mut("audit").expect("audit subcommand");
            let help = sub.render_help().to_string();
            assert!(help.contains("Audit Options:"));
            assert!(help.contains("Vet Options:"));
            assert!(help.contains("Admit Options:"));
            assert!(help.contains("Docket Options:"));
            assert!(help.contains("CANON_CLERK_AUDIT_THRESHOLD"));
            assert!(help.contains("CANON_CLERK_AUDIT_CONCURRENCY"));
            assert!(help.contains("Examples:"));

            let ws = sample_workspace();
            let provider = FullCascadeProvider::new(json!({}), json!({}), json!({}));
            let env = BTreeMap::new();

            for bad_argv in [
                vec![
                    "canon-clerk",
                    "audit",
                    "src/cli/audit.rs",
                    "--threshold",
                    "0.7",
                ],
                vec![
                    "canon-clerk",
                    "audit",
                    "src/cli/audit.rs",
                    "--apprise-threshold",
                    "0.7",
                ],
                vec![
                    "canon-clerk",
                    "audit",
                    "src/cli/audit.rs",
                    "--apprise-batch-size",
                    "10",
                ],
                vec![
                    "canon-clerk",
                    "audit",
                    "src/cli/audit.rs",
                    "--apprise-concurrency",
                    "2",
                ],
                vec!["canon-clerk", "audit"],
                vec!["canon-clerk", "audit", "--all-canons"],
                vec![
                    "canon-clerk",
                    "audit",
                    "src/cli/audit.rs",
                    "--audit-concurrency",
                    "0",
                ],
                vec![
                    "canon-clerk",
                    "audit",
                    "src/cli/audit.rs",
                    "--audit-threshold",
                    "1.4",
                ],
                vec![
                    "canon-clerk",
                    "audit",
                    "src/cli/audit.rs",
                    "--vet-threshold",
                    "1.2",
                ],
            ] {
                let args = parse_audit(&bad_argv);
                let mut stdin = Cursor::new(b"");
                let mut stdout = Vec::new();
                let err = run_audit_with_snapshot_and_provider(
                    &args,
                    &mut stdin,
                    &mut stdout,
                    &ws,
                    &env,
                    &provider,
                )
                .expect_err("guard must fail with usage error");
                assert_eq!(err.exit_code(), 2, "failed on argv: {bad_argv:?}");
                assert!(err.to_string().contains("Hint:"));
            }
        }

        #[test]
        fn test_zero_candidate_and_validation_failure_precede_gemini_api_key() {
            let ws = sample_workspace();
            let provider = FullCascadeProvider::new(json!({}), json!({}), json!({}));
            let mut empty_env = BTreeMap::new();
            empty_env.insert(
                "XDG_CONFIG_HOME".to_string(),
                "/nonexistent-canon-clerk-xdg-dir".to_string(),
            );

            let zero_args = parse_audit(&["canon-clerk", "audit", "README.md"]);
            let mut stdout = Vec::new();
            let zero_out = run_audit_with_snapshot_and_provider(
                &zero_args,
                &mut Cursor::new(b""),
                &mut stdout,
                &ws,
                &empty_env,
                &provider,
            )
            .expect("0-candidate short-circuit must succeed without API key");
            assert_eq!(provider.call_count(), 0);
            assert!(zero_out.verdict.expect("verdict").is_pass());
            let rendered = String::from_utf8(stdout).expect("utf8");
            assert!(rendered.contains("⊘ audit     : (fast-forward: zero candidate canons)"));

            let broken_ws = WorkspaceSnapshot::new()
                .with_canon_markdown(
                    "---\nunknown_frontmatter_key: true\ntriggers:\n  - \"src/**\"\n---\n# Bad Canon\n\nRule must hold.\n",
                    &ParseCanonOptions::with_source_ref(".canons/broken.md"),
                )
                .with_file("src/cli/audit.rs");
            let val_args = parse_audit(&["canon-clerk", "audit", "src/cli/audit.rs"]);
            let mut out_val = Vec::new();
            let val_err = run_audit_with_snapshot_and_provider(
                &val_args,
                &mut Cursor::new(b""),
                &mut out_val,
                &broken_ws,
                &empty_env,
                &provider,
            )
            .expect_err("malformed canon must exit 1");
            assert_eq!(val_err.exit_code(), 1);
            assert_eq!(provider.call_count(), 0);
            assert!(
                String::from_utf8(out_val)
                    .expect("utf8")
                    .contains("⊘ audit     : (fast-forward: canon validation failed)")
            );
        }

        #[test]
        fn test_conflicting_or_invalid_thinking_flags_fail_fast_with_exit_code_2() {
            let ws = sample_workspace();
            let provider = FullCascadeProvider::new(json!({}), json!({}), json!({}));
            let mut env = BTreeMap::new();
            env.insert(
                "XDG_CONFIG_HOME".to_string(),
                "/nonexistent-canon-clerk-xdg-dir".to_string(),
            );

            for (argv, expected_substr) in [
                (
                    vec![
                        "canon-clerk",
                        "audit",
                        "src/cli/audit.rs",
                        "--thinking-level",
                        "low",
                        "--reasoning-budget",
                        "2048",
                    ],
                    "mutually exclusive",
                ),
                (
                    vec![
                        "canon-clerk",
                        "audit",
                        "src/cli/audit.rs",
                        "--thinking-level",
                        "turbo",
                    ],
                    "Invalid thinking level",
                ),
            ] {
                let args = parse_audit(&argv);
                let mut stdout = Vec::new();
                let err = run_audit_with_snapshot_and_provider(
                    &args,
                    &mut Cursor::new(b""),
                    &mut stdout,
                    &ws,
                    &env,
                    &provider,
                )
                .expect_err("conflicting or invalid thinking flags must fail fast");
                assert_eq!(err.exit_code(), 2, "failed on argv: {argv:?}");
                assert!(err.to_string().contains(expected_substr));
                assert!(err.to_string().contains("Hint:"));
                assert_eq!(provider.call_count(), 0);
            }
        }
    }

    mod cascade_execution_and_github_ci_output {
        use super::*;

        fn mixed_audit_caseload_bytes() -> Vec<u8> {
            use crate::caseload::{
                AnnotationLevel, CanonAdjudication, CaseloadIntake, CaseloadVerdict,
                CodeAnnotation, VerdictStatus,
            };

            let completed = Caseload::new()
                .with_intake(CaseloadIntake {
                    target_paths: vec!["src/cli/audit.rs".to_string()],
                    pr_title: Some("Update audit flags".to_string()),
                    ..CaseloadIntake::default()
                })
                .with_verdict(CaseloadVerdict {
                    status: VerdictStatus::Fail,
                    summary: "1 passed, 2 failed across 3 active case(s)".to_string(),
                    adjudications: vec![
                        CanonAdjudication {
                            canon_path: ".canons/cli/cli-flags-kebab-case.md".to_string(),
                            compliance_summary: "Rename `--auditThreshold` to `--audit-threshold`."
                                .to_string(),
                            compliance_score: 0.08,
                            status: VerdictStatus::Fail,
                            annotations: vec![CodeAnnotation {
                                path: "src/cli/audit.rs".to_string(),
                                start_line: 2,
                                end_line: 2,
                                start_column: Some(5),
                                end_column: Some(34),
                                annotation_level: AnnotationLevel::Failure,
                                message: "Use kebab-case flag name.".to_string(),
                                title: Some("CLI Flags Kebab Case".to_string()),
                            }],
                        },
                        CanonAdjudication {
                            canon_path: ".canons/cli/cli-help-grouping.md".to_string(),
                            compliance_summary: "[Slice 1/2] Compliant. | [Slice 2/2] Compliant."
                                .to_string(),
                            compliance_score: 0.96,
                            status: VerdictStatus::Pass,
                            annotations: Vec::new(),
                        },
                        CanonAdjudication {
                            canon_path: ".canons/git/pr-title-conventional-commits.md".to_string(),
                            compliance_summary: "Prefix PR title with `fix(cli):` or `feat(cli):`."
                                .to_string(),
                            compliance_score: 0.12,
                            status: VerdictStatus::Fail,
                            annotations: Vec::new(),
                        },
                    ],
                });
            serde_json::to_vec(&completed).expect("serialize mixed caseload")
        }

        fn passing_cascade_provider() -> FullCascadeProvider {
            FullCascadeProvider::new(
                json!({
                    "cases": [{
                        "canon_path": ".canons/cli/cli-flags-kebab-case.md",
                        "colorability_summary": "Touches CLI flag definition.",
                        "colorability_score": 0.94
                    }]
                }),
                json!({
                    "exhibits": [{
                        "file_path": "src/cli/audit.rs",
                        "admissibility_summary": "Defines `--audit-threshold` flag.",
                        "admissibility_score": 0.96
                    }]
                }),
                json!({
                    "compliance_summary": "`--audit-threshold` adheres strictly to kebab-case.",
                    "compliance_score": 0.97,
                    "annotations": []
                }),
            )
        }

        #[test]
        fn test_passing_seven_stage_cascade_emits_telemetry_and_fast_forwards() {
            let ws = sample_workspace();
            let provider = passing_cascade_provider();
            let args = parse_audit(&["canon-clerk", "audit", "src/cli/audit.rs"]);
            let env = BTreeMap::from([
                (ENV_GEMINI_API_KEY.to_string(), "test-key".to_string()),
                (
                    crate::configure::ENV_DOCKET_MODEL.to_string(),
                    "google:gemini-4-hydrogen".to_string(),
                ),
                (
                    crate::configure::ENV_ADMIT_MODEL.to_string(),
                    "google:gemini-4-hydrogen".to_string(),
                ),
                (
                    crate::configure::ENV_VET_MODEL.to_string(),
                    "google:gemini-4-hydrogen".to_string(),
                ),
            ]);
            let mut stdout = Vec::new();
            let sink = RecordingTelemetrySink::new();
            let clock = SteppingTelemetryClock::new(1_767_225_600_000, 30);
            let session = TelemetrySession::new(&sink, &clock);
            let caseload = run_audit_with_telemetry(
                &args,
                &mut Cursor::new(b""),
                &mut stdout,
                &ws,
                &env,
                &provider,
                &session,
            )
            .expect("passing 7-stage audit cascade");

            assert_eq!(provider.call_count(), 3);
            let text = String::from_utf8(stdout).expect("utf8");
            assert!(text.contains("✓ audit     : 1 passed, 0 failed across 1 trial(s)"));
            assert!(text.contains("Verdict: PASS"));
            assert!(!text.contains("[pass]"));

            let stages: Vec<PipelineStage> = sink
                .events()
                .into_iter()
                .filter_map(|e| match e {
                    TelemetryEvent::StageStart { stage, .. } => Some(stage),
                    _ => None,
                })
                .collect();
            assert_eq!(
                stages,
                vec![
                    PipelineStage::Intake,
                    PipelineStage::Discover,
                    PipelineStage::Validate,
                    PipelineStage::Configure,
                    PipelineStage::Docket,
                    PipelineStage::Admit,
                    PipelineStage::Audit,
                ]
            );

            let serialized = serde_json::to_vec(&caseload).expect("serialize completed caseload");
            let mut verbose_out = Vec::new();
            run_audit_with_snapshot_and_provider(
                &parse_audit(&["canon-clerk", "audit", "-v", "--caseload", "-"]),
                &mut Cursor::new(&serialized),
                &mut verbose_out,
                &ws,
                &BTreeMap::new(),
                &provider,
            )
            .expect("fast-forward completed audit with -v");
            assert!(
                String::from_utf8(verbose_out)
                    .expect("utf8")
                    .contains("  - .canons/cli/cli-flags-kebab-case.md [pass] (score: 0.97)")
            );

            let mut compact_out = Vec::new();
            let ff_compact = parse_audit(&[
                "canon-clerk",
                "audit",
                "-v",
                "--caseload",
                "-",
                "--format",
                "compact",
            ]);
            run_audit_with_snapshot_and_provider(
                &ff_compact,
                &mut Cursor::new(&serialized),
                &mut compact_out,
                &ws,
                &BTreeMap::new(),
                &provider,
            )
            .expect("fast-forward completed audit");
            assert_eq!(provider.call_count(), 3);
            assert!(compact_out.is_empty());
        }

        #[test]
        fn test_mixed_audit_stylish_and_compact_suppress_passing_rows_by_default() {
            let ws = sample_workspace();
            let provider = FullCascadeProvider::new(json!({}), json!({}), json!({}));
            let payload = mixed_audit_caseload_bytes();

            let mut default_out = Vec::new();
            let err = run_audit_with_snapshot_and_provider(
                &parse_audit(&["canon-clerk", "audit", "--caseload", "-"]),
                &mut Cursor::new(&payload),
                &mut default_out,
                &ws,
                &BTreeMap::new(),
                &provider,
            )
            .expect_err("mixed verdict must exit 1");
            assert_eq!(err.exit_code(), 1);
            let default_text = String::from_utf8(default_out).expect("utf8");
            assert!(
                default_text.contains("Verdict: FAIL (1 passed, 2 failed across 3 active case(s))")
            );
            assert!(default_text.contains("  - .canons/cli/cli-flags-kebab-case.md [fail]"));
            assert!(default_text.contains("-> src/cli/audit.rs:2:5 [failure]"));
            assert!(
                default_text.contains("  - .canons/git/pr-title-conventional-commits.md [fail]")
            );
            assert!(!default_text.contains(".canons/cli/cli-help-grouping.md"));
            assert!(!default_text.contains("[pass]"));

            let mut env_verbose_out = Vec::new();
            let verbose_env =
                BTreeMap::from([("CANON_CLERK_VERBOSE".to_string(), "1".to_string())]);
            let _ = run_audit_with_snapshot_and_provider(
                &parse_audit(&["canon-clerk", "audit", "--caseload", "-"]),
                &mut Cursor::new(&payload),
                &mut env_verbose_out,
                &ws,
                &verbose_env,
                &provider,
            )
            .expect_err("mixed verdict must exit 1");
            assert!(
                String::from_utf8(env_verbose_out)
                    .expect("utf8")
                    .contains("  - .canons/cli/cli-help-grouping.md [pass] (score: 0.96)")
            );

            let mut compact_out = Vec::new();
            let _ = run_audit_with_snapshot_and_provider(
                &parse_audit(&[
                    "canon-clerk",
                    "audit",
                    "--caseload",
                    "-",
                    "--format",
                    "compact",
                ]),
                &mut Cursor::new(&payload),
                &mut compact_out,
                &ws,
                &BTreeMap::new(),
                &provider,
            )
            .expect_err("mixed verdict must exit 1");
            let compact_text = String::from_utf8(compact_out).expect("utf8");
            assert_eq!(
                compact_text.lines().collect::<Vec<_>>(),
                vec![
                    "src/cli/audit.rs\t2:5\tfailure\t.canons/cli/cli-flags-kebab-case.md\tUse kebab-case flag name.",
                    ".canons/git/pr-title-conventional-commits.md\t0:0\tfailure\t.canons/git/pr-title-conventional-commits.md\tPrefix PR title with `fix(cli):` or `feat(cli):`.",
                ]
            );
        }

        #[test]
        fn test_mixed_audit_github_step_summary_collapses_passing_cases() {
            let ws = sample_workspace();
            let provider = FullCascadeProvider::new(json!({}), json!({}), json!({}));
            let payload = mixed_audit_caseload_bytes();

            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let tmp_dir = std::env::temp_dir().join(format!("canon-clerk-audit-mixed-ci-{nanos}"));
            let step_summary = tmp_dir.join("step-summary.md");
            let gh_env = BTreeMap::from([
                ("GITHUB_ACTIONS".to_string(), "true".to_string()),
                (
                    "GITHUB_STEP_SUMMARY".to_string(),
                    step_summary.display().to_string(),
                ),
            ]);
            let mut gh_stdout = Vec::new();
            let _ = run_audit_with_snapshot_and_provider(
                &parse_audit(&["canon-clerk", "audit", "--caseload", "-"]),
                &mut Cursor::new(&payload),
                &mut gh_stdout,
                &ws,
                &gh_env,
                &provider,
            )
            .expect_err("mixed verdict must exit 1");

            let summary_md = fs::read_to_string(&step_summary).expect("read GITHUB_STEP_SUMMARY");
            let details_idx = summary_md
                .find("<details>\n<summary>Passing cases (1)</summary>")
                .expect("collapses passing cases behind <details>");
            let fail_idx = summary_md
                .find("| `.canons/cli/cli-flags-kebab-case.md` | `fail` | `0.08` |")
                .expect("renders failing decree row");
            let pass_idx = summary_md
                .find("| `.canons/cli/cli-help-grouping.md` | `pass` | `0.96` |")
                .expect("renders passing decree inside <details>");
            assert!(fail_idx < details_idx && details_idx < pass_idx);
            let _ = fs::remove_dir_all(&tmp_dir);
        }

        #[test]
        fn test_failing_verdict_exits_1_and_satisfies_github_actions_tripartite_contract() {
            let ws = sample_workspace();
            let provider = FullCascadeProvider::new(
                json!({
                    "cases": [{
                        "canon_path": ".canons/cli/cli-flags-kebab-case.md",
                        "colorability_summary": "Touches CLI flag definition.",
                        "colorability_score": 0.94
                    }]
                }),
                json!({
                    "exhibits": [{
                        "file_path": "src/cli/audit.rs",
                        "admissibility_summary": "Defines `--auditThreshold` flag.",
                        "admissibility_score": 0.96
                    }]
                }),
                json!({
                    "compliance_summary": "`--auditThreshold` violates kebab-case; rename to `--audit-threshold`.",
                    "compliance_score": 0.08,
                    "annotations": [{
                        "path": "src/cli/audit.rs",
                        "start_line": 2,
                        "end_line": 2,
                        "start_column": 5,
                        "end_column": 34,
                        "annotation_level": "failure",
                        "title": "CLI Flags Kebab Case",
                        "message": "Rename `--auditThreshold` to `--audit-threshold`."
                    }]
                }),
            );

            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let tmp_dir = std::env::temp_dir().join(format!("canon-clerk-audit-ci-{nanos}"));
            let step_summary = tmp_dir.join("step-summary.md");
            let archived_caseload = tmp_dir.join("caseload.json");

            let mut env = BTreeMap::new();
            env.insert(ENV_GEMINI_API_KEY.to_string(), "test-key".to_string());
            env.insert("GITHUB_ACTIONS".to_string(), "true".to_string());
            env.insert(
                "GITHUB_STEP_SUMMARY".to_string(),
                step_summary.display().to_string(),
            );

            let archived_str = archived_caseload.display().to_string();
            let args = parse_audit(&[
                "canon-clerk",
                "audit",
                "--docket-model",
                "google:gemini-4-hydrogen",
                "--admit-model",
                "google:gemini-4-hydrogen",
                "--vet-model",
                "google:gemini-4-hydrogen",
                "src/cli/audit.rs",
                "--output-caseload",
                &archived_str,
            ]);
            let mut stdout = Vec::new();
            let err = run_audit_with_snapshot_and_provider(
                &args,
                &mut Cursor::new(b""),
                &mut stdout,
                &ws,
                &env,
                &provider,
            )
            .expect_err("failing verdict must exit 1");

            assert_eq!(err.exit_code(), 1);
            assert!(err.is_silent());

            // 1. Inline workflow command annotation on stdout
            let gh_stdout = String::from_utf8(stdout).expect("utf8");
            assert!(gh_stdout.contains(
                "::error file=src/cli/audit.rs,line=2,endLine=2,col=5,endColumn=34,title=CLI Flags Kebab Case::Rename `--auditThreshold` to `--audit-threshold`."
            ));

            // 2. Step Summary Markdown report written to $GITHUB_STEP_SUMMARY
            let summary_md = fs::read_to_string(&step_summary).expect("read GITHUB_STEP_SUMMARY");
            assert!(summary_md.contains("### Canon Clerk Audit Gate: FAIL"));
            assert!(summary_md.contains("✗ audit     : 0 passed, 1 failed across 1 trial(s)"));
            assert!(
                summary_md.contains("| `.canons/cli/cli-flags-kebab-case.md` | `fail` | `0.08` |")
            );

            // 3. Full Caseload JSON archived to --output-caseload despite exit code 1
            let archived_raw =
                fs::read_to_string(&archived_caseload).expect("read archived caseload.json");
            let archived: Caseload =
                serde_json::from_str(&archived_raw).expect("parse archived Caseload");
            assert_eq!(
                archived.verdict.expect("verdict").status,
                VerdictStatus::Fail
            );

            let _ = fs::remove_dir_all(&tmp_dir);
        }

        #[test]
        fn test_vet_handoff_surfaces_immediate_audit_entry_incremental_progress_and_pure_stdout_across_formats()
         {
            use crate::caseload::{
                AdmissibilityStatus, CanonDocketAssessment, CanonVettingAssessment, CaseloadConfig,
                CaseloadDiscovery, CaseloadDocket, CaseloadEvidence, CaseloadIntake,
                CaseloadValidation, CaseloadVetting, ColorabilityStatus, DiscoveryMode,
                ExhibitAssessment, VettingStatus,
            };
            use crate::cli::telemetry::{StderrProgressSink, StderrTelemetryOptions};

            let ws = sample_workspace().with_canon_markdown(
                "---\ntriggers:\n  - \"src/cli/**\"\ninspect:\n  - diff\n---\n# CLI Help Grouping\n\nCLI flags MUST group by concern.\n",
                &ParseCanonOptions::with_source_ref(".canons/cli/cli-help-grouping.md"),
            );

            let provider = FullCascadeProvider::new(
                json!({}),
                json!({}),
                json!({
                    "compliance_summary": "Flags comply with CLI rules.",
                    "compliance_score": 0.94,
                    "annotations": []
                }),
            );

            let rule_a = ".canons/cli/cli-flags-kebab-case.md";
            let rule_b = ".canons/cli/cli-help-grouping.md";
            let admitted_item = ExhibitAssessment {
                file_path: "src/cli/audit.rs".to_string(),
                admissibility_summary: "CLI flag definition.".to_string(),
                admissibility_score: 0.95,
                admissibility_status: AdmissibilityStatus::Admitted,
            };
            let docket_cases = BTreeMap::from([
                (
                    rule_a.to_string(),
                    CanonDocketAssessment {
                        colorability_summary: "ok".to_string(),
                        colorability_score: 0.9,
                        colorability_status: ColorabilityStatus::Docketed,
                    },
                ),
                (
                    rule_b.to_string(),
                    CanonDocketAssessment {
                        colorability_summary: "ok".to_string(),
                        colorability_score: 0.9,
                        colorability_status: ColorabilityStatus::Docketed,
                    },
                ),
            ]);
            let evidence_exhibits = BTreeMap::from([
                (rule_a.to_string(), vec![admitted_item.clone()]),
                (rule_b.to_string(), vec![admitted_item]),
            ]);
            let contested_cases = BTreeMap::from([
                (
                    rule_a.to_string(),
                    CanonVettingAssessment {
                        vetting_summary: "Contested in vet.".to_string(),
                        vetting_score: 0.35,
                        vetting_status: VettingStatus::Contested,
                        contested_exhibits: Vec::new(),
                    },
                ),
                (
                    rule_b.to_string(),
                    CanonVettingAssessment {
                        vetting_summary: "Contested in vet.".to_string(),
                        vetting_score: 0.40,
                        vetting_status: VettingStatus::Contested,
                        contested_exhibits: Vec::new(),
                    },
                ),
            ]);
            let handoff_caseload = Caseload::new()
                .with_intake(CaseloadIntake {
                    target_paths: vec!["src/cli/audit.rs".to_string()],
                    ..CaseloadIntake::default()
                })
                .with_discovery(CaseloadDiscovery {
                    mode: DiscoveryMode::Triggered,
                    target_files: vec!["src/cli/audit.rs".to_string()],
                    candidate_canons: vec![rule_a.to_string(), rule_b.to_string()],
                    ..CaseloadDiscovery::default()
                })
                .with_validation(CaseloadValidation::default())
                .with_config(CaseloadConfig::default())
                .with_docket(CaseloadDocket::from_cases(docket_cases))
                .with_evidence(CaseloadEvidence::from_exhibits(evidence_exhibits))
                .with_vetting(CaseloadVetting::from_cases(contested_cases));
            let handoff_bytes = serde_json::to_vec(&handoff_caseload).expect("serialize handoff");

            let mut env = BTreeMap::new();
            env.insert(ENV_GEMINI_API_KEY.to_string(), "test-key".to_string());

            // 1. Interactive TTY (without -v) immediately emits `-> audit started` and 1/2, 2/2 progress
            let tty_sink = StderrProgressSink::new(
                Vec::<u8>::new(),
                StderrTelemetryOptions {
                    is_tty: true,
                    verbose: false,
                    quiet: false,
                    is_ci: false,
                    no_color: true,
                },
            );
            let clock = SteppingTelemetryClock::new(1_767_225_600_000, 25);
            let tty_session = TelemetrySession::new(&tty_sink, &clock);
            let mut stylish_stdout = Vec::new();
            let args_stylish = parse_audit(&["canon-clerk", "audit", "--caseload", "-"]);
            run_audit_with_telemetry(
                &args_stylish,
                &mut Cursor::new(&handoff_bytes),
                &mut stylish_stdout,
                &ws,
                &env,
                &provider,
                &tty_session,
            )
            .expect("interactive handoff audit");
            tty_session.finish_pipeline();

            let tty_stderr = String::from_utf8(tty_sink.into_inner()).expect("utf8 stderr");
            let stderr_lines: Vec<&str> = tty_stderr.lines().collect();
            assert_eq!(
                stderr_lines[0], "[telemetry] -> audit started",
                "handoff into audit must immediately announce stage entry before trials return: {tty_stderr}"
            );
            assert!(tty_stderr.contains("[telemetry] [audit 1/2]"));
            assert!(tty_stderr.contains("[telemetry] [audit 2/2]"));
            assert!(tty_stderr.contains("[telemetry] ok audit"));

            let stylish_text = String::from_utf8(stylish_stdout).expect("utf8 stylish");
            assert!(!stylish_text.contains("[telemetry]"));

            // 2. Verify stdout remains 100% pure across compact, json, and github formats under -v
            for fmt_name in ["compact", "json", "github"] {
                let verbose_sink = StderrProgressSink::new(
                    Vec::<u8>::new(),
                    StderrTelemetryOptions {
                        is_tty: false,
                        verbose: true,
                        quiet: false,
                        is_ci: false,
                        no_color: true,
                    },
                );
                let v_session = TelemetrySession::new(&verbose_sink, &clock);
                let mut fmt_stdout = Vec::new();
                let fmt_args = parse_audit(&[
                    "canon-clerk",
                    "audit",
                    "--caseload",
                    "-",
                    "--format",
                    fmt_name,
                ]);
                run_audit_with_telemetry(
                    &fmt_args,
                    &mut Cursor::new(&handoff_bytes),
                    &mut fmt_stdout,
                    &ws,
                    &env,
                    &provider,
                    &v_session,
                )
                .expect("format audit run");
                let out_str = String::from_utf8(fmt_stdout).expect("utf8 stdout");
                assert!(
                    !out_str.contains("[telemetry]"),
                    "stdout in `{fmt_name}` format must remain 100% free of stderr telemetry"
                );
                if fmt_name == "json" {
                    let _: Caseload =
                        serde_json::from_str(&out_str).expect("pure valid Caseload JSON on stdout");
                }
                let v_stderr = String::from_utf8(verbose_sink.into_inner()).expect("utf8 stderr");
                assert!(v_stderr.contains("[telemetry] -> audit started"));
                assert!(v_stderr.contains("[telemetry] [audit 1/2]"));
                assert!(v_stderr.contains("[telemetry] [audit 2/2]"));
            }
        }

        struct HybridCascadeProvider {
            decision_calls: Mutex<usize>,
            audit_calls: Mutex<usize>,
            vet_score: f64,
        }

        impl StructuredModelProvider for HybridCascadeProvider {
            fn generate_structured(
                &self,
                _request: &GenerationRequest,
            ) -> Result<GenerationResponse, ProviderError> {
                *self.audit_calls.lock().expect("lock") += 1;
                Ok(GenerationResponse {
                    payload: json!({
                        "compliance_summary": "Escalated contested case passes on full review.",
                        "compliance_score": 0.91,
                        "annotations": []
                    }),
                    thoughts: Vec::new(),
                    usage: TokenUsage {
                        prompt_tokens: 200,
                        completion_tokens: 40,
                        reasoning_tokens: 20,
                        total_tokens: 260,
                    },
                    latency_ms: 90,
                    resolved_model: crate::caseload::DEFAULT_GOOGLE_GENERATOR_MODEL.to_string(),
                })
            }

            fn evaluate_decisions(
                &self,
                request: &crate::provider::DecisionRequest,
            ) -> Result<crate::provider::DecisionResponse, ProviderError> {
                *self.decision_calls.lock().expect("lock") += 1;
                let mut scores = BTreeMap::new();
                for key in request.questions.keys() {
                    let s = if key == "compliant" {
                        self.vet_score
                    } else {
                        0.95
                    };
                    scores.insert(key.clone(), s);
                }
                Ok(crate::provider::DecisionResponse {
                    scores,
                    resolved_model: "jev-1.14.0".to_string(),
                    latency_ms: 35,
                    usage: TokenUsage {
                        prompt_tokens: 100,
                        completion_tokens: 0,
                        reasoning_tokens: 0,
                        total_tokens: 100,
                    },
                })
            }
        }

        #[test]
        fn test_default_gemini_decision_auto_activates_vet_while_typesafe_only_exits_2() {
            use crate::caseload::{
                CaseloadConfig, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, DEFAULT_GOOGLE_GENERATOR_MODEL,
                HeuristicNode, ResolvedNodeConfig,
            };
            use crate::configure::ENV_TYPESAFE_API_KEY;

            let ws = sample_workspace();
            let diff = "--- a/src/cli/audit.rs\n+++ b/src/cli/audit.rs\n@@ -1,3 +1,4 @@\n pub struct AuditArgs {\n+    // comment\n     #[arg(long = \"audit-threshold\")]\n     pub threshold: f64,\n";

            // 1a. TYPESAFE_API_KEY alone without `--provider` defaults to `google` and exits 2 asking for GEMINI_API_KEY
            let ts_only_env = BTreeMap::from([
                (ENV_TYPESAFE_API_KEY.to_string(), "ts-secret".to_string()),
                (
                    "XDG_CONFIG_HOME".to_string(),
                    "/nonexistent-canon-clerk-xdg-dir".to_string(),
                ),
            ]);
            let provider_unused = HybridCascadeProvider {
                decision_calls: Mutex::new(0),
                audit_calls: Mutex::new(0),
                vet_score: 0.95,
            };
            let ts_only_err = run_audit_with_snapshot_and_provider(
                &parse_audit(&["canon-clerk", "audit", "--diff", "-"]),
                &mut Cursor::new(diff.as_bytes()),
                &mut Vec::new(),
                &ws,
                &ts_only_env,
                &provider_unused,
            )
            .expect_err("audit with TYPESAFE_API_KEY alone must exit 2");
            assert_eq!(ts_only_err.exit_code(), 2);
            assert!(ts_only_err.to_string().contains(ENV_GEMINI_API_KEY));
            assert_eq!(*provider_unused.decision_calls.lock().expect("lock"), 0);

            // 1b. `--provider typesafe` on `audit` without `--audit-model` fails fast with UnprovisionedAuditModel (exit 2)
            let unprovisioned_err = run_audit_with_snapshot_and_provider(
                &parse_audit(&[
                    "canon-clerk",
                    "audit",
                    "--diff",
                    "-",
                    "--provider",
                    "typesafe",
                ]),
                &mut Cursor::new(diff.as_bytes()),
                &mut Vec::new(),
                &ws,
                &ts_only_env,
                &provider_unused,
            )
            .expect_err("audit --provider typesafe without --audit-model must exit 2");
            assert_eq!(unprovisioned_err.exit_code(), 2);
            assert!(unprovisioned_err.to_string().contains("--audit-model"));
            assert_eq!(*provider_unused.decision_calls.lock().expect("lock"), 0);

            // 2. GEMINI_API_KEY alone with zero model flags auto-selects google:gemini-decision-1 + google:gemini-4-carbon,
            //    automatically executes pre-trial `vet` before `audit`, and escalates contested cases to Carbon
            let google_only_env = BTreeMap::from([
                (ENV_GEMINI_API_KEY.to_string(), "gemini-secret".to_string()),
                (
                    "XDG_CONFIG_HOME".to_string(),
                    "/nonexistent-canon-clerk-xdg-dir".to_string(),
                ),
            ]);
            let google_dec_provider = HybridCascadeProvider {
                decision_calls: Mutex::new(0),
                audit_calls: Mutex::new(0),
                vet_score: 0.30,
            };
            let google_caseload = run_audit_with_snapshot_and_provider(
                &parse_audit(&["canon-clerk", "audit", "--diff", "-"]),
                &mut Cursor::new(diff.as_bytes()),
                &mut Vec::new(),
                &ws,
                &google_only_env,
                &google_dec_provider,
            )
            .expect("single-key Google zero-flag audit succeeds with auto-vet");

            let expected_closure_cfg = CaseloadConfig {
                provider: "google".to_string(),
                nodes: BTreeMap::from([
                    (
                        HeuristicNode::Docket,
                        ResolvedNodeConfig::new(DEFAULT_GOOGLE_DISCRIMINATOR_MODEL),
                    ),
                    (
                        HeuristicNode::Admit,
                        ResolvedNodeConfig::new(DEFAULT_GOOGLE_DISCRIMINATOR_MODEL),
                    ),
                    (
                        HeuristicNode::Vet,
                        ResolvedNodeConfig::new(DEFAULT_GOOGLE_DISCRIMINATOR_MODEL),
                    ),
                    (
                        HeuristicNode::Audit,
                        ResolvedNodeConfig::new(DEFAULT_GOOGLE_GENERATOR_MODEL),
                    ),
                ]),
            };
            assert_eq!(google_caseload.config, Some(expected_closure_cfg));
            assert!(
                google_caseload.vetting.is_some(),
                "vet must auto-activate on default google:gemini-decision-1"
            );
            assert_eq!(*google_dec_provider.decision_calls.lock().expect("lock"), 3);
            assert_eq!(*google_dec_provider.audit_calls.lock().expect("lock"), 1);
        }

        #[test]
        fn test_mixed_provider_typesafe_closure_skips_all_vetted_audit_trials() {
            use crate::caseload::{
                DEFAULT_GOOGLE_GENERATOR_MODEL, DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL, HeuristicNode,
            };
            use crate::configure::ENV_TYPESAFE_API_KEY;

            let ws = sample_workspace();
            let diff = "--- a/src/cli/audit.rs\n+++ b/src/cli/audit.rs\n@@ -1,3 +1,4 @@\n pub struct AuditArgs {\n+    // comment\n     #[arg(long = \"audit-threshold\")]\n     pub threshold: f64,\n";
            let hybrid_env = BTreeMap::from([
                (ENV_TYPESAFE_API_KEY.to_string(), "ts-secret".to_string()),
                (ENV_GEMINI_API_KEY.to_string(), "gemini-secret".to_string()),
                (
                    "XDG_CONFIG_HOME".to_string(),
                    "/nonexistent-canon-clerk-xdg-dir".to_string(),
                ),
            ]);
            let hybrid_provider = HybridCascadeProvider {
                decision_calls: Mutex::new(0),
                audit_calls: Mutex::new(0),
                vet_score: 0.95,
            };
            let caseload = run_audit_with_snapshot_and_provider(
                &parse_audit(&[
                    "canon-clerk",
                    "audit",
                    "--diff",
                    "-",
                    "--provider",
                    "typesafe",
                    "--audit-model",
                    DEFAULT_GOOGLE_GENERATOR_MODEL,
                ]),
                &mut Cursor::new(diff.as_bytes()),
                &mut Vec::new(),
                &ws,
                &hybrid_env,
                &hybrid_provider,
            )
            .expect("explicit --provider typesafe --audit-model google:gemini-4-carbon succeeds");

            let cfg = caseload.config.expect("config");
            assert_eq!(cfg.provider, "typesafe");
            assert_eq!(
                cfg.model_for(HeuristicNode::Docket),
                Some(DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL)
            );
            assert_eq!(
                cfg.model_for(HeuristicNode::Vet),
                Some(DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL)
            );
            assert_eq!(
                cfg.model_for(HeuristicNode::Audit),
                Some(DEFAULT_GOOGLE_GENERATOR_MODEL)
            );
            assert!(caseload.vetting.is_some());
            assert_eq!(*hybrid_provider.decision_calls.lock().expect("lock"), 3);
            assert_eq!(*hybrid_provider.audit_calls.lock().expect("lock"), 0);
        }

        #[test]
        fn test_partial_closure_caseload_handoff_preserves_upstream_nodes() {
            use crate::caseload::{
                DEFAULT_GOOGLE_GENERATOR_MODEL, DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL, HeuristicNode,
            };
            use crate::cli::vet::{VetArgs, run_vet_with_snapshot_and_provider};
            use crate::configure::ENV_TYPESAFE_API_KEY;

            let ws = sample_workspace();
            let diff = "--- a/src/cli/audit.rs\n+++ b/src/cli/audit.rs\n@@ -1,3 +1,4 @@\n pub struct AuditArgs {\n+    // comment\n     #[arg(long = \"audit-threshold\")]\n     pub threshold: f64,\n";
            let ts_only_env = BTreeMap::from([
                (ENV_TYPESAFE_API_KEY.to_string(), "ts-secret".to_string()),
                ("XDG_CONFIG_HOME".to_string(), "/nonexistent".to_string()),
            ]);
            let vet_provider = HybridCascadeProvider {
                decision_calls: Mutex::new(0),
                audit_calls: Mutex::new(0),
                vet_score: 0.35,
            };
            let vet_args = VetArgs {
                filing: FilingSourceArgs {
                    diff: Some("-".to_string()),
                    ..FilingSourceArgs::default()
                },
                provider: ProviderConfigArgs {
                    provider: Some("typesafe".to_string()),
                    ..ProviderConfigArgs::default()
                },
                io: CaseloadIoArgs {
                    json: true,
                    ..CaseloadIoArgs::default()
                },
                ..VetArgs::default()
            };
            let mut vet_json = Vec::new();
            let vet_err = run_vet_with_snapshot_and_provider(
                &vet_args,
                &mut Cursor::new(diff.as_bytes()),
                &mut vet_json,
                &ws,
                &ts_only_env,
                &vet_provider,
            )
            .expect_err("contested vet exits 1 while emitting JSON caseload");
            assert_eq!(vet_err.exit_code(), 1);
            assert_eq!(*vet_provider.decision_calls.lock().expect("lock"), 3);

            let google_only_env = BTreeMap::from([
                (ENV_GEMINI_API_KEY.to_string(), "gemini-secret".to_string()),
                ("XDG_CONFIG_HOME".to_string(), "/nonexistent".to_string()),
            ]);
            let audit_handoff_provider = HybridCascadeProvider {
                decision_calls: Mutex::new(0),
                audit_calls: Mutex::new(0),
                vet_score: 0.35,
            };
            let mut handoff_stdout = Vec::new();
            let final_caseload = run_audit_with_snapshot_and_provider(
                &parse_audit(&[
                    "canon-clerk",
                    "audit",
                    "--caseload",
                    "-",
                    "--audit-model",
                    DEFAULT_GOOGLE_GENERATOR_MODEL,
                ]),
                &mut Cursor::new(&vet_json),
                &mut handoff_stdout,
                &ws,
                &google_only_env,
                &audit_handoff_provider,
            )
            .expect("partial-closure audit handoff with only GEMINI_API_KEY succeeds");
            assert_eq!(
                *audit_handoff_provider.decision_calls.lock().expect("lock"),
                0
            );
            assert_eq!(*audit_handoff_provider.audit_calls.lock().expect("lock"), 1);
            let final_cfg = final_caseload.config.expect("final config");
            assert_eq!(
                final_cfg.model_for(HeuristicNode::Docket),
                Some(DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL)
            );
            assert_eq!(
                final_cfg.model_for(HeuristicNode::Audit),
                Some(DEFAULT_GOOGLE_GENERATOR_MODEL)
            );
            let receipt = String::from_utf8(handoff_stdout).expect("utf8");
            assert!(
                receipt.contains(
                    "✓ configure : docket, admit, vet 'typesafe:jev-latest'; audit 'google:gemini-4-carbon'"
                ),
                "unexpected provenance receipt:\n{receipt}"
            );
        }

        #[test]
        fn test_zero_code_custom_keyless_systemone_and_google_audit_cascade_executes_without_stage_edits()
         {
            use crate::caseload::{DEFAULT_GOOGLE_GENERATOR_MODEL, HeuristicNode};
            use std::fs;
            use std::os::unix::fs::PermissionsExt;
            use std::time::{SystemTime, UNIX_EPOCH};

            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let xdg_dir = std::env::temp_dir().join(format!("canon-clerk-audit-custom-{unique}"));
            let cfg_dir = xdg_dir.join("canon-clerk");
            fs::create_dir_all(&cfg_dir).expect("create xdg dir");
            let cfg_file = cfg_dir.join("config.json");
            fs::write(
                &cfg_file,
                r#"{
                    "providers": {
                        "local-dev": {
                            "protocol": "systemone",
                            "baseUrl": "http://127.0.0.1:8080/v1",
                            "defaultDiscriminator": "local-dev:kev-0.8b"
                        },
                        "google": {
                            "apiKey": "cfg-google-audit-secret"
                        }
                    },
                    "defaults": {
                        "provider": "local-dev"
                    },
                    "nodes": {
                        "audit": { "model": "google:gemini-4-carbon" }
                    }
                }"#,
            )
            .expect("write custom hybrid config");
            fs::set_permissions(&cfg_file, fs::Permissions::from_mode(0o600)).expect("chmod 0600");

            let ws = sample_workspace();
            let diff = "--- a/src/cli/audit.rs\n+++ b/src/cli/audit.rs\n@@ -1,3 +1,4 @@\n pub struct AuditArgs {\n+    // comment\n     #[arg(long = \"audit-threshold\")]\n     pub threshold: f64,\n";
            let env =
                BTreeMap::from([("XDG_CONFIG_HOME".to_string(), xdg_dir.display().to_string())]);
            let hybrid_provider = HybridCascadeProvider {
                decision_calls: Mutex::new(0),
                audit_calls: Mutex::new(0),
                vet_score: 0.35,
            };
            let mut stdout = Vec::new();
            let caseload = run_audit_with_snapshot_and_provider(
                &parse_audit(&["canon-clerk", "audit", "--diff", "-"]),
                &mut Cursor::new(diff.as_bytes()),
                &mut stdout,
                &ws,
                &env,
                &hybrid_provider,
            )
            .expect("hybrid custom local-dev screening + google audit succeeds");

            let cfg = caseload.config.expect("config");
            assert_eq!(cfg.provider, "local-dev");
            assert_eq!(
                cfg.model_for(HeuristicNode::Docket),
                Some("local-dev:kev-0.8b")
            );
            assert_eq!(
                cfg.model_for(HeuristicNode::Admit),
                Some("local-dev:kev-0.8b")
            );
            assert_eq!(
                cfg.model_for(HeuristicNode::Vet),
                Some("local-dev:kev-0.8b")
            );
            assert_eq!(
                cfg.model_for(HeuristicNode::Audit),
                Some(DEFAULT_GOOGLE_GENERATOR_MODEL)
            );
            assert_eq!(*hybrid_provider.decision_calls.lock().expect("lock"), 3);
            assert_eq!(*hybrid_provider.audit_calls.lock().expect("lock"), 1);

            let _ = fs::remove_dir_all(&xdg_dir);
        }
    }
}
