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

mod finalize;
mod github_check;
pub mod guards;
mod render;
mod stage_options;
mod stages;

use crate::caseload::Caseload;
use crate::cli::CliError;
use crate::cli::args::{
    CanonSelectionArgs, CaseloadIoArgs, FilingSourceArgs, GithubCheckArgs, ProviderConfigArgs,
    ProviderTransportArgs,
};
use crate::cli::closure::{
    ClosureBackend, materialize_closure_snapshot, resolve_cascade_discover, resolve_cascade_intake,
    resolve_closure_validate,
};
use crate::cli::io::ensure_stage_stdin;
use crate::cli::node_args::{AuditNodeModelArgs, RejectedUnqualifiedTuningArgs};
use crate::discover::WorkspaceSnapshot;
use crate::github::detect_github_bypass_label;
use crate::provider::StructuredModelProvider;
use crate::telemetry::TelemetrySession;
use clap::{Args, ValueEnum};
use finalize::{
    AuditOutputContext, finalize_cascade_result, finalize_verdict_and_exit_code,
    finish_bypassed_github_label, finish_zero_candidate_audit, handle_validation_failure,
};
use guards::{
    can_fast_forward_completed_audit, enforce_filing_and_case_guard,
    ensure_stage_prefixed_threshold_flags, resolve_and_validate_audit_cli_params,
};
use render::AuditWatermarks;
use stages::execute_jit_configure_docket_admit_and_audit;
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

  # Publish all findings via the GitHub Checks REST API (batched 50/request) with forensics archiving:
  git diff origin/main | canon-clerk audit --diff - --publish-github-check --output-caseload caseload.json

  # Use TypeSafe Jev across docket, admit, and vet before escalating contested cases to Gemini Carbon:
  git diff origin/main | canon-clerk audit --diff - --provider typesafe --audit-model google:gemini-4-carbon";

/// Behavior when heuristic evaluation is required but configured provider API credentials are absent.
#[derive(ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OnMissingCredentials {
    /// Fail closed with exit code `2` and an actionable remediation hint (default).
    #[default]
    Fail,
    /// After deterministic `validate` passes, record a `SKIPPED` summary / Check Run and exit `0`.
    Skip,
}

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

    /// Behavior when heuristic evaluation is required and configured provider credentials are absent (`fail` default, or `skip`).
    #[arg(
        long = "on-missing-credentials",
        value_name = "fail|skip",
        value_enum,
        default_value_t = OnMissingCredentials::Fail,
        help_heading = "Provider Credentials",
        help = "Action when required provider credentials are absent ('fail' = exit 2 default, 'skip' = record SKIPPED and exit 0 after validate)"
    )]
    pub on_missing_credentials: OnMissingCredentials,

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

    #[command(flatten)]
    pub github_check: GithubCheckArgs,

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

fn try_preflight_short_circuits(
    args: &AuditArgs,
    resolved_filing: &crate::cli::github_args::ResolvedFilingSource,
    bypass_label: Option<&str>,
    incoming: &mut Option<Caseload>,
    thresholds: stages::AuditCascadeThresholds,
    output_ctx: &mut AuditOutputContext<'_>,
    telemetry: &TelemetrySession<'_>,
) -> Option<Result<Caseload, CliError>> {
    if can_fast_forward_completed_audit(args, &resolved_filing.intake, incoming.as_ref())
        && let Some(completed) = incoming.take()
    {
        telemetry.finish_pipeline();
        let wm = AuditWatermarks::pre_populated(
            thresholds.docket_threshold,
            thresholds.admit_threshold,
            thresholds.vet_threshold,
            thresholds.audit_threshold,
            completed.vetting.is_some(),
        );
        return Some(finalize_verdict_and_exit_code(
            args, completed, &wm, output_ctx,
        ));
    }

    if let Some(bypass) = detect_github_bypass_label(
        bypass_label,
        resolved_filing.fetched_pr_labels.as_deref(),
        resolved_filing.fetched_pr_actor.as_deref(),
        output_ctx.env,
    ) {
        telemetry.finish_pipeline();
        return Some(finish_bypassed_github_label(
            args,
            incoming.take().unwrap_or_default(),
            &bypass,
            output_ctx,
        ));
    }

    None
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
    ensure_stage_stdin(&args.filing, &args.canons, &args.io)?;
    let (thresholds, stored_config) = resolve_and_validate_audit_cli_params(args, env)?;
    let bypass_label = args.github_check.resolve_bypass_label(env)?;
    let mut incoming = args.io.read_caseload(stdin)?;
    let resolved_filing = args.filing.resolve_intake_options_with_env(stdin, env)?;
    let github_target = args.github_check.resolve_target(
        args.filing.github_repo.as_deref(),
        resolved_filing.fetched_pr_head_sha.as_deref(),
        env,
    )?;
    let mut output_ctx = AuditOutputContext {
        env,
        stdout,
        github_target: github_target.as_ref(),
    };

    if let Some(res) = try_preflight_short_circuits(
        args,
        &resolved_filing,
        bypass_label.as_deref(),
        &mut incoming,
        thresholds,
        &mut output_ctx,
        telemetry,
    ) {
        return res;
    }

    let intake_opts = resolved_filing.intake;

    enforce_filing_and_case_guard(&intake_opts, &args.canons, incoming.as_ref())?;

    let (caseload_after_intake, intake_disp, intake_changed) =
        resolve_cascade_intake("audit", &intake_opts, incoming, telemetry)?;

    let (mut snapshot, inline_canons) =
        materialize_closure_snapshot(&backend, &args.canons, stdin, &caseload_after_intake)?;
    let total_workspace_canons = Some(snapshot.canons.len());

    let (caseload_after_discover, discover_disp) = resolve_cascade_discover(
        &args.canons,
        &inline_canons,
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
            &mut output_ctx,
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
            &mut output_ctx,
        );
    }

    let cascade_result = execute_jit_configure_docket_admit_and_audit(
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

    finalize_cascade_result(
        args,
        cascade_result,
        (intake_disp, discover_disp, validate_disp),
        total_workspace_canons,
        thresholds,
        &mut output_ctx,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::caseload::VerdictStatus;
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
                        exception_claim_score: None,
                        vetting_status: VettingStatus::Contested,
                        contested_exhibits: Vec::new(),
                        contested_spans: Vec::new(),
                    },
                ),
                (
                    rule_b.to_string(),
                    CanonVettingAssessment {
                        vetting_summary: "Contested in vet.".to_string(),
                        vetting_score: 0.40,
                        exception_claim_score: None,
                        vetting_status: VettingStatus::Contested,
                        contested_exhibits: Vec::new(),
                        contested_spans: Vec::new(),
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
                    } else if key == "claims_exception"
                        || key.starts_with("file_exception:")
                        || key.starts_with("span_exception:")
                    {
                        0.05
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

    mod base_workspace_reference_overlay {
        use super::*;
        use crate::provider::{DecisionRequest, DecisionResponse};

        struct PromptCapturingProvider {
            decision_states: Mutex<Vec<String>>,
            audit_prompts: Mutex<Vec<String>>,
        }

        impl StructuredModelProvider for PromptCapturingProvider {
            fn generate_structured(
                &self,
                request: &GenerationRequest,
            ) -> Result<GenerationResponse, ProviderError> {
                self.audit_prompts
                    .lock()
                    .expect("lock")
                    .push(request.user_prompt.clone());
                Ok(GenerationResponse {
                    payload: json!({
                        "compliance_summary": "Settings match README.",
                        "compliance_score": 0.95,
                        "annotations": []
                    }),
                    thoughts: Vec::new(),
                    usage: TokenUsage::default(),
                    latency_ms: 10,
                    resolved_model: crate::caseload::DEFAULT_GOOGLE_GENERATOR_MODEL.to_string(),
                })
            }

            fn evaluate_decisions(
                &self,
                request: &DecisionRequest,
            ) -> Result<DecisionResponse, ProviderError> {
                self.decision_states
                    .lock()
                    .expect("lock")
                    .push(request.state.clone());
                // Force `vet` to contest (0.30) while `docket` & `admit` admit (0.95) so both `vet` and `audit` prompts are exercised.
                let score = if request.state.contains("Statutory Grounding References") {
                    0.30
                } else {
                    0.95
                };
                let scores = request
                    .questions
                    .keys()
                    .map(|id| (id.clone(), score))
                    .collect();
                Ok(DecisionResponse {
                    scores,
                    usage: TokenUsage::default(),
                    latency_ms: 10,
                    resolved_model: request.model_specifier.clone(),
                })
            }
        }

        #[test]
        fn test_base_workspace_resolves_post_patch_reference_in_vet_and_audit_prompts() {
            let ws = WorkspaceSnapshot::new()
                .with_canon_markdown(
                    "---\ntriggers:\n  - \"README.md\"\nreferences:\n  - \".github/settings.yml\"\n---\nREADME description changes MUST update `.github/settings.yml`.\n",
                    &ParseCanonOptions::with_source_ref(".canons/settings-sync.md"),
                )
                .with_file_content("README.md", "# Canon Clerk\nOld tagline.\n")
                .with_file_content(
                    ".github/settings.yml",
                    "repository:\n  description: Old tagline\n",
                );

            let pr_diff = "\
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1,2 +1,2 @@
 # Canon Clerk
-Old tagline.
+New tagline.
diff --git a/.github/settings.yml b/.github/settings.yml
--- a/.github/settings.yml
+++ b/.github/settings.yml
@@ -1,2 +1,2 @@
 repository:
-  description: Old tagline
+  description: New tagline
";
            let provider = PromptCapturingProvider {
                decision_states: Mutex::new(Vec::new()),
                audit_prompts: Mutex::new(Vec::new()),
            };
            let env = BTreeMap::from([("GEMINI_API_KEY".to_string(), "test-key".to_string())]);
            let mut stdout = Vec::new();
            let caseload = run_audit_with_snapshot_and_provider(
                &parse_audit(&[
                    "canon-clerk",
                    "audit",
                    "--diff",
                    "-",
                    "--workspace-state",
                    "base",
                ]),
                &mut Cursor::new(pr_diff.as_bytes()),
                &mut stdout,
                &ws,
                &env,
                &provider,
            )
            .expect("base-workspace audit succeeds");

            assert_eq!(
                caseload.intake.as_ref().map(|i| i.workspace_state),
                Some(crate::caseload::WorkspaceState::Base)
            );
            let vet_states = provider.decision_states.lock().expect("lock");
            let vet_prompt = vet_states
                .iter()
                .find(|s| s.contains("Statutory Grounding References"))
                .expect("vet prompt includes references section");
            assert!(
                vet_prompt.contains("description: New tagline"),
                "vet prompt must see post-patch reference content"
            );
            assert!(
                !vet_prompt.contains("description: Old tagline"),
                "vet prompt must not see stale base-branch reference content"
            );

            let audit_prompts = provider.audit_prompts.lock().expect("lock");
            assert_eq!(audit_prompts.len(), 1);
            assert!(
                audit_prompts[0].contains("description: New tagline"),
                "audit prompt must see post-patch reference content"
            );
            assert!(
                !audit_prompts[0].contains("description: Old tagline"),
                "audit prompt must not see stale base-branch reference content"
            );
        }

        #[test]
        fn test_base_workspace_conflict_fails_fast_post_validate_with_zero_model_calls() {
            let ws = WorkspaceSnapshot::new()
                .with_canon_markdown(
                    "---\ntriggers:\n  - \"README.md\"\nreferences:\n  - \".github/settings.yml\"\n---\nREADME description changes MUST update `.github/settings.yml`.\n",
                    &ParseCanonOptions::with_source_ref(".canons/settings-sync.md"),
                )
                .with_file_content("README.md", "# Canon Clerk\nOld tagline.\n")
                .with_file_content(
                    ".github/settings.yml",
                    "repository:\n  description: Diverged main description\n",
                );

            let conflicting_diff = "\
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1,2 +1,2 @@
 # Canon Clerk
-Old tagline.
+New tagline.
diff --git a/.github/settings.yml b/.github/settings.yml
--- a/.github/settings.yml
+++ b/.github/settings.yml
@@ -1,2 +1,2 @@
 repository:
-  description: Old tagline
+  description: New tagline
";
            let provider = PromptCapturingProvider {
                decision_states: Mutex::new(Vec::new()),
                audit_prompts: Mutex::new(Vec::new()),
            };
            let env = BTreeMap::from([("GEMINI_API_KEY".to_string(), "test-key".to_string())]);
            let mut stdout = Vec::new();
            let err = run_audit_with_snapshot_and_provider(
                &parse_audit(&[
                    "canon-clerk",
                    "audit",
                    "--diff",
                    "-",
                    "--workspace-state",
                    "base",
                ]),
                &mut Cursor::new(conflicting_diff.as_bytes()),
                &mut stdout,
                &ws,
                &env,
                &provider,
            )
            .expect_err("conflicting reference diff must pull the andon cord");

            assert_eq!(err.exit_code(), 2);
            assert!(err.message.contains(".github/settings.yml"));
            assert_eq!(
                provider.decision_states.lock().expect("lock").len(),
                0,
                "andon cord must fire post-validate with zero decision model calls"
            );
            assert_eq!(
                provider.audit_prompts.lock().expect("lock").len(),
                0,
                "andon cord must fire post-validate with zero audit model calls"
            );
        }
    }

    mod github_checks_publication {
        use super::*;
        use crate::caseload::{
            AnnotationLevel, CanonAdjudication, CaseloadIntake, CaseloadVerdict, CodeAnnotation,
        };
        use crate::github::{
            ENV_GITHUB_API_URL, ENV_GITHUB_REPOSITORY, ENV_GITHUB_SHA, ENV_GITHUB_TOKEN,
        };
        use std::net::{TcpListener, TcpStream};
        use std::sync::Arc;
        use std::thread;

        fn read_mock_github_request(stream: &mut TcpStream) -> (String, Value) {
            let mut buf = Vec::new();
            let mut tmp = [0u8; 4096];
            let mut header_end = None;
            let mut content_length = 0usize;

            while header_end.is_none() {
                let n = stream.read(&mut tmp).expect("read mock request");
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&tmp[..n]);
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    header_end = Some(pos + 4);
                    let hdrs = String::from_utf8_lossy(&buf[..pos]);
                    for line in hdrs.lines() {
                        if let Some((k, v)) = line.split_once(':')
                            && k.eq_ignore_ascii_case("content-length")
                        {
                            content_length = v.trim().parse::<usize>().unwrap_or(0);
                        }
                    }
                }
            }

            let start = header_end.expect("header terminator");
            while buf.len() < start + content_length {
                let n = stream.read(&mut tmp).expect("read mock body");
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&tmp[..n]);
            }

            let request_line = String::from_utf8_lossy(&buf[..start])
                .lines()
                .next()
                .unwrap_or("")
                .to_string();
            let body: Value =
                serde_json::from_slice(&buf[start..start + content_length]).expect("json body");
            (request_line, body)
        }

        fn write_mock_github_response(stream: &mut TcpStream, status: &str, body: &str) {
            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(resp.as_bytes()).expect("write response");
            stream.flush().expect("flush response");
        }

        #[test]
        fn test_publish_github_check_fails_fast_before_model_spend() {
            let ws = sample_workspace();
            let provider = FullCascadeProvider::new(json!({}), json!({}), json!({}));
            let mut env = BTreeMap::new();
            env.insert(ENV_GEMINI_API_KEY.to_string(), "test-key".to_string());

            let args = parse_audit(&[
                "canon-clerk",
                "audit",
                "src/cli/audit.rs",
                "--publish-github-check",
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
            .expect_err("missing GITHUB_TOKEN must fail fast with exit code 2");

            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("Hint:"));
            assert_eq!(provider.call_count(), 0);
        }

        fn failing_caseload_with_findings(count: usize) -> Vec<u8> {
            let annotations: Vec<CodeAnnotation> = (1..=count)
                .map(|i| CodeAnnotation {
                    path: format!("src/mod_{i}.rs"),
                    start_line: i,
                    end_line: i,
                    start_column: Some(1),
                    end_column: Some(8),
                    annotation_level: AnnotationLevel::Failure,
                    message: format!("Finding #{i}"),
                    title: None,
                })
                .collect();

            let completed = Caseload::new()
                .with_intake(CaseloadIntake {
                    target_paths: vec!["src/mod_1.rs".to_string()],
                    ..CaseloadIntake::default()
                })
                .with_verdict(CaseloadVerdict {
                    status: VerdictStatus::Fail,
                    summary: "0 passed, 1 failed across 1 active case(s)".to_string(),
                    adjudications: vec![CanonAdjudication {
                        canon_path: ".canons/demo/rule.md".to_string(),
                        compliance_summary: format!("{count} violations found."),
                        compliance_score: 0.05,
                        status: VerdictStatus::Fail,
                        annotations,
                    }],
                });
            serde_json::to_vec(&completed).expect("serialize caseload")
        }

        fn mock_github_env(addr: std::net::SocketAddr) -> BTreeMap<String, String> {
            BTreeMap::from([
                ("GITHUB_ACTIONS".to_string(), "true".to_string()),
                (ENV_GITHUB_API_URL.to_string(), format!("http://{addr}")),
                (ENV_GITHUB_TOKEN.to_string(), "ghs_test".to_string()),
                (
                    ENV_GITHUB_REPOSITORY.to_string(),
                    "PAIR-code/canon-clerk".to_string(),
                ),
                (ENV_GITHUB_SHA.to_string(), "deadbeef1234".to_string()),
            ])
        }

        #[test]
        fn test_replay_publishes_batched_annotations_and_suppresses_workflow_commands() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");
            let captured = Arc::new(Mutex::new(Vec::<(String, Value)>::new()));
            let captured_ref = Arc::clone(&captured);

            let server = thread::spawn(move || {
                for (status, body) in [
                    ("201 Created", r#"{"id":4242}"#),
                    ("200 OK", r#"{"id":4242}"#),
                ] {
                    let (mut stream, _) = listener.accept().expect("accept request");
                    let req = read_mock_github_request(&mut stream);
                    captured_ref.lock().expect("lock").push(req);
                    write_mock_github_response(&mut stream, status, body);
                }
            });

            let args = parse_audit(&[
                "canon-clerk",
                "audit",
                "--caseload",
                "-",
                "--publish-github-check",
                "--github-check-name",
                "Custom Audit Check",
            ]);
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let summary_path =
                std::env::temp_dir().join(format!("cc-check-step-summary-{unique}.md"));
            let mut env = mock_github_env(addr);
            env.insert(
                "GITHUB_STEP_SUMMARY".to_string(),
                summary_path.display().to_string(),
            );

            let provider = FullCascadeProvider::new(json!({}), json!({}), json!({}));
            let mut stdout = Vec::new();
            let err = run_audit_with_snapshot_and_provider(
                &args,
                &mut Cursor::new(failing_caseload_with_findings(65)),
                &mut stdout,
                &WorkspaceSnapshot::new(),
                &env,
                &provider,
            )
            .expect_err("failing verdict must exit 1");

            server.join().expect("server thread joined");
            assert_eq!(err.exit_code(), 1);
            assert_eq!(provider.call_count(), 0);

            let out_str = String::from_utf8(stdout).expect("utf8 stdout");
            assert!(out_str.contains("Pipeline Provenance:"));
            assert!(out_str.contains("Verdict: FAIL"));
            assert!(!out_str.contains("::error "));

            assert_step_summary_contains_fail_report(&summary_path);

            let reqs = captured.lock().expect("lock");
            assert_eq!(reqs.len(), 2);
            assert_eq!(
                reqs[0].0,
                "POST /repos/PAIR-code/canon-clerk/check-runs HTTP/1.1"
            );
            assert_eq!(reqs[0].1["name"], "Custom Audit Check");
            assert_eq!(reqs[0].1["conclusion"], "failure");
            assert_eq!(
                reqs[0].1["output"]["annotations"]
                    .as_array()
                    .expect("annotations")
                    .len(),
                50
            );
            assert!(
                reqs[0].1["output"]["text"]
                    .as_str()
                    .expect("text")
                    .contains("Finding #65")
            );
            assert_eq!(
                reqs[1].0,
                "PATCH /repos/PAIR-code/canon-clerk/check-runs/4242 HTTP/1.1"
            );
            assert_eq!(
                reqs[1].1["output"]["annotations"]
                    .as_array()
                    .expect("annotations")
                    .len(),
                15
            );
        }

        fn assert_step_summary_contains_fail_report(path: &std::path::Path) {
            let step_summary =
                fs::read_to_string(path).expect("step summary written alongside check");
            assert!(step_summary.contains("### Canon Clerk Audit Gate: FAIL"));
            assert!(step_summary.contains("Pipeline Provenance:"));
            let _ = fs::remove_file(path);
        }

        #[test]
        fn test_zero_candidate_and_validation_failure_publish_expected_conclusions() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");
            let captured = Arc::new(Mutex::new(Vec::<(String, Value)>::new()));
            let captured_ref = Arc::clone(&captured);

            let server = thread::spawn(move || {
                for id in [101u64, 102u64] {
                    let (mut stream, _) = listener.accept().expect("accept request");
                    let req = read_mock_github_request(&mut stream);
                    captured_ref.lock().expect("lock").push(req);
                    write_mock_github_response(
                        &mut stream,
                        "201 Created",
                        &format!(r#"{{"id":{id}}}"#),
                    );
                }
            });

            let mut env = BTreeMap::new();
            env.insert(ENV_GITHUB_API_URL.to_string(), format!("http://{addr}"));
            env.insert(ENV_GITHUB_TOKEN.to_string(), "ghs_test".to_string());
            env.insert(
                ENV_GITHUB_REPOSITORY.to_string(),
                "PAIR-code/canon-clerk".to_string(),
            );
            env.insert(ENV_GITHUB_SHA.to_string(), "cafebabe".to_string());

            let provider = FullCascadeProvider::new(json!({}), json!({}), json!({}));

            // 1. Zero-candidate short-circuit -> conclusion: "success", exit 0.
            let ws = sample_workspace();
            let zero_args = parse_audit(&[
                "canon-clerk",
                "audit",
                "README.md",
                "--publish-github-check",
            ]);
            let mut stdout = Vec::new();
            run_audit_with_snapshot_and_provider(
                &zero_args,
                &mut Cursor::new(b""),
                &mut stdout,
                &ws,
                &env,
                &provider,
            )
            .expect("zero-candidate audit must pass");

            // 2. Static validation failure -> conclusion: "failure" with validation diagnostics, exit 1.
            let broken_ws = WorkspaceSnapshot::new()
                .with_canon_markdown(
                    "---\nunknown_frontmatter_key: true\ntriggers:\n  - \"src/**\"\n---\n# Bad Canon\n\nRule must hold.\n",
                    &ParseCanonOptions::with_source_ref(".canons/broken.md"),
                )
                .with_file("src/cli/audit.rs");
            let val_args = parse_audit(&[
                "canon-clerk",
                "audit",
                "src/cli/audit.rs",
                "--publish-github-check",
            ]);
            let mut val_stdout = Vec::new();
            let val_err = run_audit_with_snapshot_and_provider(
                &val_args,
                &mut Cursor::new(b""),
                &mut val_stdout,
                &broken_ws,
                &env,
                &provider,
            )
            .expect_err("validation failure must exit 1");
            assert_eq!(val_err.exit_code(), 1);

            server.join().expect("server thread joined");

            let reqs = captured.lock().expect("lock");
            assert_eq!(reqs.len(), 2);
            assert_eq!(reqs[0].1["conclusion"], "success");
            assert_eq!(reqs[1].1["conclusion"], "failure");
            assert!(
                !reqs[1].1["output"]["annotations"]
                    .as_array()
                    .expect("annotations")
                    .is_empty(),
                "validation diagnostics must be attached as inline annotations"
            );
        }

        #[test]
        fn test_output_caseload_flushed_before_github_api_error_exits_2() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");

            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept request");
                let _ = read_mock_github_request(&mut stream);
                write_mock_github_response(
                    &mut stream,
                    "500 Internal Server Error",
                    r#"{"message":"Internal Server Error"}"#,
                );
            });

            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let out_path = std::env::temp_dir().join(format!("cc-gh-forensics-{unique}.json"));
            let out_str = out_path.display().to_string();

            let mut env = BTreeMap::new();
            env.insert(ENV_GITHUB_API_URL.to_string(), format!("http://{addr}"));
            env.insert(ENV_GITHUB_TOKEN.to_string(), "ghs_test".to_string());
            env.insert(
                ENV_GITHUB_REPOSITORY.to_string(),
                "PAIR-code/canon-clerk".to_string(),
            );
            env.insert(ENV_GITHUB_SHA.to_string(), "cafebabe".to_string());

            let ws = sample_workspace();
            let provider = FullCascadeProvider::new(json!({}), json!({}), json!({}));
            let args = parse_audit(&[
                "canon-clerk",
                "audit",
                "README.md",
                "--output-caseload",
                &out_str,
                "--publish-github-check",
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
            .expect_err("GitHub API 500 must exit 2");

            server.join().expect("server thread joined");

            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("Hint:"));
            assert!(err.to_string().contains(&out_str));
            assert!(
                out_path.exists(),
                "--output-caseload must be flushed to disk prior to GitHub API publication"
            );
            let _ = fs::remove_file(&out_path);
        }

        fn spawn_pr_and_check_mock_server() -> (
            std::net::SocketAddr,
            Arc<Mutex<Option<Value>>>,
            thread::JoinHandle<()>,
        ) {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");
            let captured_check_post = Arc::new(Mutex::new(None::<Value>));
            let captured_server = Arc::clone(&captured_check_post);

            let server = thread::spawn(move || {
                for idx in 0..4 {
                    let (mut stream, _) = listener.accept().expect("accept request");
                    let mut buf = Vec::new();
                    let mut tmp = [0u8; 4096];
                    let mut header_end = None;
                    let mut content_length = 0usize;
                    while header_end.is_none() {
                        let n = stream.read(&mut tmp).expect("read");
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            header_end = Some(pos + 4);
                            let headers = String::from_utf8_lossy(&buf[..pos]);
                            for line in headers.lines() {
                                if let Some((k, v)) = line.split_once(':')
                                    && k.eq_ignore_ascii_case("content-length")
                                {
                                    content_length = v.trim().parse().unwrap_or(0);
                                }
                            }
                        }
                    }
                    let start = header_end.expect("headers");
                    while buf.len() < start + content_length {
                        let n = stream.read(&mut tmp).expect("read body");
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                    }
                    let (status, ctype, body) = match idx {
                        0 => (
                            "200 OK",
                            "application/json",
                            r#"{"title":"feat: native PR","body":"body","head":{"sha":"pr-api-sha-358"}}"#,
                        ),
                        1 => (
                            "200 OK",
                            "text/plain",
                            "diff --git a/src/cli/audit.rs b/src/cli/audit.rs\n--- a/src/cli/audit.rs\n+++ b/src/cli/audit.rs\n@@ -1 +1 @@\n-old\n+new\n",
                        ),
                        2 => (
                            "200 OK",
                            "application/json",
                            r#"[{"sha":"abc1234","commit":{"message":"feat: native PR"}}]"#,
                        ),
                        _ => {
                            let parsed: Value =
                                serde_json::from_slice(&buf[start..start + content_length])
                                    .expect("check-run JSON");
                            *captured_server.lock().unwrap() = Some(parsed);
                            ("201 Created", "application/json", r#"{"id":777}"#)
                        }
                    };
                    let resp = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(resp.as_bytes()).expect("write response");
                }
            });

            (addr, captured_check_post, server)
        }

        #[test]
        fn test_github_pr_with_on_missing_credentials_skip_publishes_skipped_check_and_exits_0() {
            let (addr, captured_check_post, server) = spawn_pr_and_check_mock_server();

            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let summary_path = std::env::temp_dir().join(format!("cc-skip-summary-{unique}.md"));
            let out_path = std::env::temp_dir().join(format!("cc-skip-caseload-{unique}.json"));
            let out_str = out_path.display().to_string();

            // Notice: NO GEMINI_API_KEY, NO GITHUB_EVENT_PATH, NO GITHUB_SHA!
            let mut env = BTreeMap::new();
            env.insert(ENV_GITHUB_API_URL.to_string(), format!("http://{addr}"));
            env.insert(ENV_GITHUB_TOKEN.to_string(), "ghs_test".to_string());
            env.insert(
                "GITHUB_STEP_SUMMARY".to_string(),
                summary_path.display().to_string(),
            );

            let ws = sample_workspace();
            let provider = FullCascadeProvider::new(json!({}), json!({}), json!({}));
            let args = parse_audit(&[
                "canon-clerk",
                "audit",
                "--github-pr",
                "358",
                "--github-repo",
                "PAIR-code/canon-clerk",
                "--publish-github-check",
                "--on-missing-credentials",
                "skip",
                "--output-caseload",
                &out_str,
            ]);

            let mut stdout = Vec::new();
            let caseload = run_audit_with_snapshot_and_provider(
                &args,
                &mut Cursor::new(b""),
                &mut stdout,
                &ws,
                &env,
                &provider,
            )
            .expect("--on-missing-credentials skip must exit 0");

            server.join().expect("server joined");

            assert_eq!(provider.call_count(), 0);
            assert!(caseload.validation.is_some());
            assert!(caseload.verdict.is_none());
            assert!(out_path.exists());

            let step_md = fs::read_to_string(&summary_path).expect("step summary");
            assert!(step_md.contains("SKIPPED"));

            let check_body = captured_check_post
                .lock()
                .unwrap()
                .clone()
                .expect("Check Run POST captured");
            assert_eq!(check_body["conclusion"], "skipped");
            assert_eq!(check_body["head_sha"], "pr-api-sha-358");

            let _ = fs::remove_file(&summary_path);
            let _ = fs::remove_file(&out_path);
        }

        #[test]
        fn test_on_missing_credentials_skip_still_fails_exit_1_on_validation_error() {
            let mut ws = WorkspaceSnapshot::default();
            ws.existing_files.insert("src/lib.rs".to_string());
            ws.file_contents
                .insert("src/lib.rs".to_string(), "fn main() {}\n".to_string());
            let opts = ParseCanonOptions::with_source_ref(".canons/broken.md");
            ws = ws.with_canon_markdown(
                "---\nid: [unterminated\n---\n# Broken\nCode MUST work.\n",
                &opts,
            );

            let provider = FullCascadeProvider::new(json!({}), json!({}), json!({}));
            let empty_env = BTreeMap::new(); // No GEMINI_API_KEY

            let args = parse_audit(&[
                "canon-clerk",
                "audit",
                "src/lib.rs",
                "--on-missing-credentials",
                "skip",
            ]);
            let mut stdout = Vec::new();
            let err = run_audit_with_snapshot_and_provider(
                &args,
                &mut Cursor::new(b""),
                &mut stdout,
                &ws,
                &empty_env,
                &provider,
            )
            .expect_err("static validation failure must still exit 1 even under skip");

            assert_eq!(err.exit_code(), 1);
            assert_eq!(provider.call_count(), 0);
        }

        #[test]
        fn test_default_on_missing_credentials_fail_exits_2() {
            let ws = sample_workspace();
            let provider = FullCascadeProvider::new(json!({}), json!({}), json!({}));
            let empty_env = BTreeMap::new(); // No GEMINI_API_KEY

            let args = parse_audit(&["canon-clerk", "audit", "src/cli/audit.rs"]);
            let mut stdout = Vec::new();
            let err = run_audit_with_snapshot_and_provider(
                &args,
                &mut Cursor::new(b""),
                &mut stdout,
                &ws,
                &empty_env,
                &provider,
            )
            .expect_err("default fail-closed behavior must exit 2");

            assert_eq!(err.exit_code(), 2);
            assert_eq!(provider.call_count(), 0);
        }
    }

    mod github_bypass_label {
        use super::*;
        use crate::github::{
            DEFAULT_GITHUB_BYPASS_LABEL, ENV_GITHUB_ACTOR, ENV_GITHUB_API_URL,
            ENV_GITHUB_EVENT_PATH, ENV_GITHUB_REPOSITORY, ENV_GITHUB_TOKEN,
        };
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::Arc;
        use std::thread;

        fn spawn_mock_github_server(
            responses: Vec<(&'static str, &'static str, String)>,
        ) -> (
            std::net::SocketAddr,
            Arc<Mutex<Vec<Value>>>,
            thread::JoinHandle<()>,
        ) {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");
            let captured_posts = Arc::new(Mutex::new(Vec::<Value>::new()));
            let captured_server = Arc::clone(&captured_posts);

            let server = thread::spawn(move || {
                for (status, ctype, body) in responses {
                    let (mut stream, _) = listener.accept().expect("accept request");
                    let mut buf = Vec::new();
                    let mut tmp = [0u8; 4096];
                    let mut header_end = None;
                    let mut content_length = 0usize;
                    while header_end.is_none() {
                        let n = stream.read(&mut tmp).expect("read");
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            header_end = Some(pos + 4);
                            let headers = String::from_utf8_lossy(&buf[..pos]);
                            for line in headers.lines() {
                                if let Some((k, v)) = line.split_once(':')
                                    && k.eq_ignore_ascii_case("content-length")
                                {
                                    content_length = v.trim().parse().unwrap_or(0);
                                }
                            }
                        }
                    }
                    let start = header_end.expect("headers");
                    while buf.len() < start + content_length {
                        let n = stream.read(&mut tmp).expect("read body");
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                    }
                    if content_length > 0 {
                        let parsed: Value =
                            serde_json::from_slice(&buf[start..start + content_length])
                                .expect("POST JSON body");
                        captured_server.lock().unwrap().push(parsed);
                    }
                    let resp = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(resp.as_bytes()).expect("write response");
                }
            });

            (addr, captured_posts, server)
        }

        fn assert_bypass_check_run_post(
            captured_posts: &Arc<Mutex<Vec<Value>>>,
            expected_sha: &str,
            expected_label: &str,
            expected_actor: &str,
        ) {
            let posts = captured_posts.lock().unwrap();
            assert_eq!(posts.len(), 1);
            let check_body = &posts[0];
            assert_eq!(check_body["conclusion"], "skipped");
            assert_eq!(check_body["head_sha"], expected_sha);
            assert!(
                check_body["output"]["title"]
                    .as_str()
                    .unwrap_or("")
                    .contains(expected_label)
            );
            assert!(
                check_body["output"]["summary"]
                    .as_str()
                    .unwrap_or("")
                    .contains(expected_actor)
            );
            assert_eq!(check_body["output"]["annotations"], json!([]));
        }

        #[test]
        fn test_bypass_label_in_event_short_circuits_before_validate_and_llm_spend() {
            // Bare --github-pr fetches diff (1 GET), then publishes Check Run (1 POST).
            let (addr, captured_posts, server) = spawn_mock_github_server(vec![
                (
                    "200 OK",
                    "text/plain",
                    "diff --git a/src/cli/audit.rs b/src/cli/audit.rs\n--- a/src/cli/audit.rs\n+++ b/src/cli/audit.rs\n@@ -1 +1 @@\n-old\n+new\n"
                        .to_string(),
                ),
                (
                    "200 OK",
                    "application/json",
                    r#"[{"sha":"abc1234","commit":{"message":"hotfix: bypass test"}}]"#.to_string(),
                ),
                ("201 Created", "application/json", r#"{"id":901}"#.to_string()),
            ]);

            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let event_path = std::env::temp_dir().join(format!("cc-bypass-e2e-{unique}.json"));
            let summary_path = std::env::temp_dir().join(format!("cc-bypass-summary-{unique}.md"));
            let out_path = std::env::temp_dir().join(format!("cc-bypass-caseload-{unique}.json"));
            let out_str = out_path.display().to_string();

            fs::write(
                &event_path,
                r#"{"sender":{"login":"maintainer-bob"},"pull_request":{"number":360,"title":"hotfix: bypass test","head":{"sha":"bypass-head-sha-360"},"labels":[{"name":"Skip-Audit"}]}}"#,
            )
            .expect("write event file");

            // Workspace intentionally contains a broken canon AND no GEMINI_API_KEY:
            // label bypass must still short-circuit cleanly with exit 0 and 0 model calls.
            let broken_ws = WorkspaceSnapshot::new()
                .with_canon_markdown(
                    "---\nid: [unterminated\n---\n# Broken\nMust hold.\n",
                    &ParseCanonOptions::with_source_ref(".canons/broken.md"),
                )
                .with_file("src/cli/audit.rs");

            let mut env = BTreeMap::new();
            env.insert(ENV_GITHUB_API_URL.to_string(), format!("http://{addr}"));
            env.insert(ENV_GITHUB_TOKEN.to_string(), "ghs_test".to_string());
            env.insert(
                ENV_GITHUB_REPOSITORY.to_string(),
                "PAIR-code/canon-clerk".to_string(),
            );
            env.insert(
                ENV_GITHUB_EVENT_PATH.to_string(),
                event_path.display().to_string(),
            );
            env.insert(
                "GITHUB_STEP_SUMMARY".to_string(),
                summary_path.display().to_string(),
            );

            let provider = FullCascadeProvider::new(json!({}), json!({}), json!({}));
            let args = parse_audit(&[
                "canon-clerk",
                "audit",
                "--github-pr",
                "--publish-github-check",
                "--github-bypass-label",
                "--output-caseload",
                &out_str,
            ]);

            let mut stdout = Vec::new();
            let caseload = run_audit_with_snapshot_and_provider(
                &args,
                &mut Cursor::new(b""),
                &mut stdout,
                &broken_ws,
                &env,
                &provider,
            )
            .expect("bypass label must short-circuit with exit 0");

            server.join().expect("mock server joined");

            assert_eq!(provider.call_count(), 0);
            assert!(caseload.validation.is_none());
            assert!(caseload.verdict.is_none());
            assert!(out_path.exists());

            let rendered = String::from_utf8(stdout).expect("utf8 stdout");
            assert!(rendered.contains("SKIPPED") && rendered.contains("Skip-Audit"));
            let step_md = fs::read_to_string(&summary_path).expect("step summary");
            assert!(step_md.contains("Skip-Audit") && step_md.contains("@maintainer-bob"));
            assert_bypass_check_run_post(
                &captured_posts,
                "bypass-head-sha-360",
                "Skip-Audit",
                "@maintainer-bob",
            );

            let _ = fs::remove_file(&event_path);
            let _ = fs::remove_file(&summary_path);
            let _ = fs::remove_file(&out_path);
        }

        #[test]
        fn test_bypass_label_from_numbered_pr_api_and_non_matching_label_does_not_bypass() {
            // Explicit --github-pr 360 fetches metadata, diff, commits, then posts Check Run.
            let commits_json =
                r#"[{"sha":"api-bypass-sha","commit":{"message":"hotfix: emergency"}}]"#
                    .to_string();
            let (addr, captured_posts, server) = spawn_mock_github_server(vec![
                (
                    "200 OK",
                    "application/json",
                    r#"{"title":"hotfix","body":"urgent","head":{"sha":"api-bypass-sha"},"labels":[{"name":"emergency-bypass"}]}"#
                        .to_string(),
                ),
                (
                    "200 OK",
                    "text/plain",
                    "diff --git a/src/cli/audit.rs b/src/cli/audit.rs\n--- a/src/cli/audit.rs\n+++ b/src/cli/audit.rs\n@@ -1 +1 @@\n-old\n+new\n"
                        .to_string(),
                ),
                ("200 OK", "application/json", commits_json),
                ("201 Created", "application/json", r#"{"id":902}"#.to_string()),
            ]);

            let mut env = BTreeMap::new();
            env.insert(ENV_GITHUB_API_URL.to_string(), format!("http://{addr}"));
            env.insert(ENV_GITHUB_TOKEN.to_string(), "ghs_test".to_string());
            env.insert(ENV_GITHUB_ACTOR.to_string(), "release-bot".to_string());

            let ws = sample_workspace();
            let provider = FullCascadeProvider::new(json!({}), json!({}), json!({}));
            let args = parse_audit(&[
                "canon-clerk",
                "audit",
                "--github-pr",
                "360",
                "--github-repo",
                "PAIR-code/canon-clerk",
                "--publish-github-check",
                "--github-bypass-label",
                "emergency-bypass",
            ]);

            let mut stdout = Vec::new();
            run_audit_with_snapshot_and_provider(
                &args,
                &mut Cursor::new(b""),
                &mut stdout,
                &ws,
                &env,
                &provider,
            )
            .expect("custom bypass label via PR API must exit 0");
            server.join().expect("mock server joined");

            assert_eq!(provider.call_count(), 0);
            let posts = captured_posts.lock().unwrap();
            assert_eq!(posts.len(), 1);
            assert_eq!(posts[0]["conclusion"], "skipped");
            assert_eq!(posts[0]["head_sha"], "api-bypass-sha");
            assert!(
                posts[0]["output"]["summary"]
                    .as_str()
                    .unwrap_or("")
                    .contains("emergency-bypass")
            );
            assert!(
                posts[0]["output"]["summary"]
                    .as_str()
                    .unwrap_or("")
                    .contains("@release-bot")
            );

            // When label is absent, audit does NOT bypass (proceeds to missing-credentials exit 2).
            let mut no_match_env = BTreeMap::new();
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let no_label_event =
                std::env::temp_dir().join(format!("cc-no-bypass-event-{unique}.json"));
            fs::write(
                &no_label_event,
                r#"{"pull_request":{"number":360,"head":{"sha":"sha1"},"labels":[{"name":"type: feat"}]}}"#,
            )
            .expect("write event");
            no_match_env.insert(
                ENV_GITHUB_EVENT_PATH.to_string(),
                no_label_event.display().to_string(),
            );
            let non_bypass_args = parse_audit(&[
                "canon-clerk",
                "audit",
                "src/cli/audit.rs",
                "--github-bypass-label",
                DEFAULT_GITHUB_BYPASS_LABEL,
            ]);
            let mut out2 = Vec::new();
            let err = run_audit_with_snapshot_and_provider(
                &non_bypass_args,
                &mut Cursor::new(b""),
                &mut out2,
                &ws,
                &no_match_env,
                &provider,
            )
            .expect_err("non-matching label must not bypass");
            let _ = fs::remove_file(&no_label_event);
            assert_eq!(err.exit_code(), 2);
            assert_eq!(provider.call_count(), 0);
        }
    }
}
