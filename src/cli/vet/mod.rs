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

//! CLI driving adapter for the `canon-clerk vet` subcommand (`docs/architecture/nodes/vet.md` §5).

pub mod guards;
mod render;
pub(crate) mod stages;

use crate::caseload::{Caseload, CaseloadDocket, CaseloadEvidence, CaseloadVetting};
use crate::cli::CliError;
use crate::cli::admit::guards::{resolve_admit_concurrency, resolve_admit_threshold};
use crate::cli::args::{CanonSelectionArgs, CaseloadIoArgs, FilingSourceArgs, ProviderConfigArgs};
use crate::cli::closure::{
    ClosureBackend, materialize_closure_snapshot, resolve_cascade_discover, resolve_cascade_intake,
    resolve_closure_validate,
};
use crate::cli::docket::guards::{
    resolve_docket_batch_size, resolve_docket_concurrency, resolve_docket_threshold,
};
use crate::cli::io::ensure_single_stdin_source;
use crate::cli::provenance::StageDisposition;
use crate::cli::telemetry::resolve_verbose_flag;
use crate::discover::WorkspaceSnapshot;
use crate::provider::StructuredModelProvider;
use crate::telemetry::TelemetrySession;
use clap::{Args, ValueEnum};
use guards::{
    can_fast_forward_completed_vet, enforce_filing_and_case_guard,
    ensure_stage_prefixed_threshold_flags, resolve_vet_concurrency, resolve_vet_threshold,
};
use render::{VetWatermarks, render_vet_output};
use stages::{VetCascadeThresholds, execute_jit_configure_docket_admit_and_vet};
use std::collections::BTreeMap;
use std::env;
use std::io::{Read, Write};

const VET_EXAMPLES: &str = "\
Examples:
  # Fast pre-flight compliance vetting on a unified diff using TypeSafe Jev:
  git diff origin/main | canon-clerk vet --diff - --screener-model jev

  # Vet explicit target paths or newline-delimited stdin paths:
  canon-clerk vet src/cli src/vet --screener-model jev
  git diff origin/main --name-only | canon-clerk vet - --screener-model jev

  # Escalate to `canon-clerk audit` only when contested cases require remediation & line spans:
  git diff origin/main | canon-clerk vet --diff - --screener-model jev --output-caseload caseload.json \\
    || canon-clerk audit --caseload caseload.json

  # Emit tab-separated compact records strictly for contested cases:
  git diff origin/main | canon-clerk vet --diff - --format compact";

/// Output presentation format for `canon-clerk vet`.
#[derive(ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VetFormat {
    /// Human-readable 7-stage Pipeline Funnel Provenance receipt, Vetting Gate summary, and contested cases.
    #[default]
    Stylish,
    /// Full cumulative `Caseload` JSON envelope.
    Json,
    /// Tab-separated (`<canon>\tcontested\t<score>\t<summary>`) diagnostic lines for contested cases.
    Compact,
}

/// Command-line arguments for `canon-clerk vet`.
#[derive(Args, Debug, Clone, Default, PartialEq)]
#[command(
    about = "Prima facie compliance vetting: clear uncontested compliant cases prior to judicial audit",
    long_about = "Prima Facie Compliance Vetting stage of the Caseload DAG (Cascade Spine). Executes the guarded [intake, discover, validate, configure, docket, admit, vet] closure to clear high-confidence compliant cases (`vetted`) on the papers while flagging disputed cases (`contested`, exit 1) for substantive judicial `audit`.",
    after_help = VET_EXAMPLES
)]
pub struct VetArgs {
    #[command(flatten)]
    pub filing: FilingSourceArgs,

    #[command(flatten)]
    pub canons: CanonSelectionArgs,

    /// Prima facie compliance score threshold in `[0.0, 1.0]` (default: `0.80`; env: `CANON_CLERK_VET_THRESHOLD`).
    #[arg(
        long = "vet-threshold",
        value_name = "FLOAT",
        allow_hyphen_values = true,
        help_heading = "Vet Options",
        help = "Prima facie compliance threshold in [0.0, 1.0] (default: 0.80; fallback env: CANON_CLERK_VET_THRESHOLD)"
    )]
    pub vet_threshold: Option<f64>,

    /// Optional maximum concurrent in-flight per-case vetting calls (default: `4`; env: `CANON_CLERK_VET_CONCURRENCY`).
    #[arg(
        long = "vet-concurrency",
        value_name = "COUNT",
        help_heading = "Vet Options",
        help = "Maximum concurrent in-flight per-case vetting calls (default: 4; fallback env: CANON_CLERK_VET_CONCURRENCY)"
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

    /// Upstream maximum concurrent in-flight per-case `admit` screener calls (default: `4`; env: `CANON_CLERK_ADMIT_CONCURRENCY`).
    #[arg(
        long = "admit-concurrency",
        visible_alias = "max-concurrent-calls",
        value_name = "COUNT",
        help_heading = "Admit Options",
        help = "Maximum concurrent upstream admit screener calls (default: 4; fallback env: CANON_CLERK_ADMIT_CONCURRENCY)"
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

    /// Upstream maximum candidate canons per `docket` screener call (default: `25`; env: `CANON_CLERK_DOCKET_BATCH_SIZE`).
    #[arg(
        long = "docket-batch-size",
        visible_alias = "max-canons-per-call",
        value_name = "COUNT",
        help_heading = "Docket Options",
        help = "Maximum candidate canons per upstream docket screener call (default: 25; fallback env: CANON_CLERK_DOCKET_BATCH_SIZE)"
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

    /// Hidden capture rejecting ambiguous unqualified `--threshold`.
    #[arg(long = "threshold", hide = true, allow_hyphen_values = true)]
    pub rejected_threshold: Option<String>,

    /// Hidden capture rejecting out-of-closure `--audit-threshold`.
    #[arg(long = "audit-threshold", hide = true, allow_hyphen_values = true)]
    pub rejected_audit_threshold: Option<String>,

    /// Hidden capture rejecting out-of-closure `--audit-concurrency`.
    #[arg(long = "audit-concurrency", hide = true, allow_hyphen_values = true)]
    pub rejected_audit_concurrency: Option<String>,

    /// Hidden capture rejecting out-of-closure `--apprise-threshold`.
    #[arg(long = "apprise-threshold", hide = true, allow_hyphen_values = true)]
    pub rejected_apprise_threshold: Option<String>,

    /// Hidden capture rejecting out-of-closure `--apprise-batch-size`.
    #[arg(long = "apprise-batch-size", hide = true, allow_hyphen_values = true)]
    pub rejected_apprise_batch_size: Option<String>,

    /// Hidden capture rejecting out-of-closure `--apprise-concurrency`.
    #[arg(long = "apprise-concurrency", hide = true, allow_hyphen_values = true)]
    pub rejected_apprise_concurrency: Option<String>,

    #[command(flatten)]
    pub provider: ProviderConfigArgs,

    /// Output format (`stylish`, `json`, or `compact`).
    #[arg(
        long = "format",
        value_enum,
        default_value_t = VetFormat::Stylish,
        help_heading = "Caseload & Output",
        help = "Format prima facie vetting output"
    )]
    pub format: VetFormat,

    #[command(flatten)]
    pub io: CaseloadIoArgs,

    /// Propagated top-level `-v` / `--verbose` flag (`CANON_CLERK_VERBOSE`).
    #[arg(skip)]
    pub verbose: bool,
}

impl Eq for VetArgs {}

/// Executes `canon-clerk vet` hermetically with an explicit [`TelemetrySession`].
pub fn run_vet_with_telemetry(
    args: &VetArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace: &WorkspaceSnapshot,
    env: &BTreeMap<String, String>,
    provider: &dyn StructuredModelProvider,
    telemetry: &TelemetrySession<'_>,
) -> Result<Caseload, CliError> {
    run_vet_core(
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

/// Executes `canon-clerk vet` hermetically against an in-memory [`WorkspaceSnapshot`],
/// explicit environment map, and hexagonal [`StructuredModelProvider`].
pub fn run_vet_with_snapshot_and_provider(
    args: &VetArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace: &WorkspaceSnapshot,
    env: &BTreeMap<String, String>,
    provider: &dyn StructuredModelProvider,
) -> Result<Caseload, CliError> {
    run_vet_with_telemetry(
        args,
        stdin,
        stdout,
        workspace,
        env,
        provider,
        &TelemetrySession::noop(),
    )
}

/// Executes `canon-clerk vet` against the live working directory with an explicit [`TelemetrySession`].
pub fn run_vet_live_with_telemetry(
    args: &VetArgs,
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
    run_vet_core(
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

/// Executes `canon-clerk vet` using the current working directory, process environment,
/// and live model provider.
pub fn run_vet(
    args: &VetArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    run_vet_live_with_telemetry(args, stdin, stdout, &TelemetrySession::noop())
}

fn run_vet_core(
    args: &VetArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    env: &BTreeMap<String, String>,
    backend: ClosureBackend<'_>,
    telemetry: &TelemetrySession<'_>,
) -> Result<Caseload, CliError> {
    ensure_stage_prefixed_threshold_flags(args)?;
    args.provider.validate_thinking_flags()?;
    ensure_single_stdin_source(&args.filing, args.io.caseload.as_deref())?;
    let _ = resolve_verbose_flag(args.verbose, env)?;

    let thresholds = VetCascadeThresholds {
        docket_threshold: resolve_docket_threshold(args.docket_threshold, env)?,
        admit_threshold: resolve_admit_threshold(args.admit_threshold, env)?,
        vet_threshold: resolve_vet_threshold(args.vet_threshold, env)?,
    };
    let _validated_docket_batch = resolve_docket_batch_size(args.docket_batch_size, env, None)?;
    let _validated_docket_conc = resolve_docket_concurrency(args.docket_concurrency, env, None)?;
    let _validated_admit_conc = resolve_admit_concurrency(args.admit_concurrency, env, None)?;
    let _validated_vet_conc = resolve_vet_concurrency(args.vet_concurrency, env, None)?;

    let incoming = args.io.read_caseload(stdin)?;
    let intake_opts = args.filing.resolve_intake_options(stdin)?;

    if can_fast_forward_completed_vet(args, &intake_opts, incoming.as_ref())
        && let Some(completed) = incoming
    {
        telemetry.finish_pipeline();
        let wm = VetWatermarks::pre_populated(
            thresholds.docket_threshold,
            thresholds.admit_threshold,
            thresholds.vet_threshold,
        );
        return finalize_vetting_and_exit_code(args, completed, &wm, env, stdout);
    }

    enforce_filing_and_case_guard(&intake_opts, &args.canons, incoming.as_ref())?;

    let (caseload_after_intake, intake_disp, intake_changed) =
        resolve_cascade_intake("vet", &intake_opts, incoming, telemetry)?;

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
        return finish_zero_candidate_vet(
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

    let outcome = execute_jit_configure_docket_admit_and_vet(
        args,
        env,
        thresholds,
        caseload_after_validate,
        &mut snapshot,
        &backend,
        telemetry,
    )?;
    telemetry.finish_pipeline();

    let wm = VetWatermarks {
        intake: intake_disp,
        discover: discover_disp,
        validate: validate_disp,
        configure: outcome.configure_disp,
        docket: outcome.docket_disp,
        admit: outcome.admit_disp,
        vet: outcome.vet_disp,
        total_workspace_canons,
        docket_threshold: thresholds.docket_threshold,
        admit_threshold: thresholds.admit_threshold,
        vet_threshold: thresholds.vet_threshold,
    };
    finalize_vetting_and_exit_code(args, outcome.caseload, &wm, env, stdout)
}

fn finalize_vetting_and_exit_code(
    args: &VetArgs,
    caseload: Caseload,
    wm: &VetWatermarks,
    env: &BTreeMap<String, String>,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    args.io.write_output_caseload(&caseload)?;
    render_vet_output(args, &caseload, wm, env, stdout)?;
    if caseload
        .vetting
        .as_ref()
        .is_some_and(|v| !v.is_all_vetted())
    {
        return Err(CliError::silent(1));
    }
    Ok(caseload)
}

fn finish_zero_candidate_vet(
    args: &VetArgs,
    caseload: Caseload,
    intake_disp: StageDisposition,
    total_workspace_canons: Option<usize>,
    thresholds: VetCascadeThresholds,
    env: &BTreeMap<String, String>,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    let enriched = caseload
        .with_docket(CaseloadDocket::default())
        .with_evidence(CaseloadEvidence::default())
        .with_vetting(CaseloadVetting::default());
    let wm = VetWatermarks {
        intake: intake_disp,
        discover: StageDisposition::ZeroCandidateShortCircuit,
        validate: StageDisposition::SkippedZeroCandidates,
        configure: StageDisposition::SkippedZeroCandidates,
        docket: StageDisposition::SkippedZeroCandidates,
        admit: StageDisposition::SkippedZeroCandidates,
        vet: StageDisposition::SkippedZeroCandidates,
        total_workspace_canons,
        docket_threshold: thresholds.docket_threshold,
        admit_threshold: thresholds.admit_threshold,
        vet_threshold: thresholds.vet_threshold,
    };
    finalize_vetting_and_exit_code(args, enriched, &wm, env, stdout)
}

#[allow(
    clippy::too_many_arguments,
    reason = "Internal CLI helper formatting validation failure provenance watermarks across the 7-stage vet closure"
)]
fn handle_validation_failure(
    args: &VetArgs,
    caseload: &Caseload,
    intake_disp: StageDisposition,
    discover_disp: StageDisposition,
    total_workspace_canons: Option<usize>,
    thresholds: VetCascadeThresholds,
    env: &BTreeMap<String, String>,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    args.io.write_output_caseload(caseload)?;
    let wm = VetWatermarks {
        intake: intake_disp,
        discover: discover_disp,
        validate: StageDisposition::ValidationFailed,
        configure: StageDisposition::SkippedValidationFailed,
        docket: StageDisposition::SkippedValidationFailed,
        admit: StageDisposition::SkippedValidationFailed,
        vet: StageDisposition::SkippedValidationFailed,
        total_workspace_canons,
        docket_threshold: thresholds.docket_threshold,
        admit_threshold: thresholds.admit_threshold,
        vet_threshold: thresholds.vet_threshold,
    };
    render_vet_output(args, caseload, &wm, env, stdout)?;
    Err(CliError::silent(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::configure::{DEFAULT_JEV_MODEL, ENV_TYPESAFE_API_KEY};
    use crate::provider::{
        DecisionRequest, DecisionResponse, GenerationRequest, GenerationResponse, ProviderError,
        TokenUsage,
    };
    use crate::telemetry::{
        PipelineStage, RecordingTelemetrySink, SteppingTelemetryClock, TelemetryEvent,
    };
    use clap::CommandFactory;
    use std::io::Cursor;

    struct HermeticJevProvider {
        compliance_score: f64,
    }

    impl StructuredModelProvider for HermeticJevProvider {
        fn generate_structured(
            &self,
            request: &GenerationRequest,
        ) -> Result<GenerationResponse, ProviderError> {
            Err(ProviderError::UnsupportedProvider {
                provider: "google".to_string(),
                model_specifier: request.model_specifier.clone(),
            })
        }

        fn evaluate_decisions(
            &self,
            request: &DecisionRequest,
        ) -> Result<DecisionResponse, ProviderError> {
            let mut scores = BTreeMap::new();
            for key in request.questions.keys() {
                let s = if key == "compliant" {
                    self.compliance_score
                } else {
                    0.95
                };
                scores.insert(key.clone(), s);
            }
            Ok(DecisionResponse {
                scores,
                resolved_model: "jev-1.14.0".to_string(),
                latency_ms: 80,
                usage: TokenUsage {
                    prompt_tokens: 150,
                    completion_tokens: 0,
                    reasoning_tokens: 0,
                    total_tokens: 150,
                },
            })
        }
    }

    fn sample_ws() -> WorkspaceSnapshot {
        WorkspaceSnapshot::new()
            .with_file_content("src/cli/app.rs", "pub fn main() {}\n")
            .with_canon_markdown(
                "---\ntriggers:\n  - \"src/cli/**\"\ntags:\n  - cli\n---\nCLI flags MUST use kebab-case.\n",
                &ParseCanonOptions::with_source_ref(".canons/cli/kebab.md"),
            )
    }

    fn typesafe_only_env() -> BTreeMap<String, String> {
        BTreeMap::from([(
            ENV_TYPESAFE_API_KEY.to_string(),
            "test-typesafe-key".to_string(),
        )])
    }

    mod cli_ergonomics_and_guards {
        use super::*;

        #[test]
        fn test_help_screen_documents_grouped_options_and_examples() {
            let mut cmd = crate::cli::Cli::command();
            let sub = cmd.find_subcommand_mut("vet").expect("vet subcommand");
            let help = sub.render_help().to_string();
            assert!(help.contains("Vet Options:"));
            assert!(help.contains("--vet-threshold <FLOAT>"));
            assert!(help.contains("CANON_CLERK_VET_THRESHOLD"));
            assert!(help.contains("Examples:"));
        }

        #[test]
        fn test_rejects_naked_invocation_and_out_of_closure_flags_with_exit_2() {
            let ws = sample_ws();
            let provider = HermeticJevProvider {
                compliance_score: 0.95,
            };
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();

            let naked = run_vet_with_snapshot_and_provider(
                &VetArgs::default(),
                &mut stdin,
                &mut stdout,
                &ws,
                &typesafe_only_env(),
                &provider,
            )
            .expect_err("naked vet must fail exit 2");
            assert_eq!(naked.exit_code(), 2);
            assert!(naked.to_string().contains("Hint:"));

            let bad_flag = VetArgs {
                rejected_audit_threshold: Some("0.5".to_string()),
                ..VetArgs::default()
            };
            let err_flag = run_vet_with_snapshot_and_provider(
                &bad_flag,
                &mut stdin,
                &mut stdout,
                &ws,
                &typesafe_only_env(),
                &provider,
            )
            .expect_err("audit-threshold on vet must fail exit 2");
            assert_eq!(err_flag.exit_code(), 2);
            assert!(err_flag.to_string().contains("--vet-threshold"));
        }
    }

    mod vetting_gate_and_typesafe_only_closure {
        use super::*;

        #[test]
        fn test_jev_only_credentials_exit_0_when_vetted_and_exit_1_when_contested() {
            let ws = sample_ws();
            let diff = "--- a/src/cli/app.rs\n+++ b/src/cli/app.rs\n@@ -1,1 +1,2 @@\n pub fn main() {}\n+// ok\n";
            let args = VetArgs {
                filing: FilingSourceArgs {
                    diff: Some("-".to_string()),
                    ..FilingSourceArgs::default()
                },
                provider: ProviderConfigArgs {
                    screener_model: Some(DEFAULT_JEV_MODEL.to_string()),
                    ..ProviderConfigArgs::default()
                },
                ..VetArgs::default()
            };

            let sink = RecordingTelemetrySink::new();
            let clock = SteppingTelemetryClock::new(1_767_225_600_000, 10);
            let session = TelemetrySession::new(&sink, &clock);

            let mut stdin = Cursor::new(diff.as_bytes());
            let mut stdout = Vec::new();
            let vetted_caseload = run_vet_with_telemetry(
                &args,
                &mut stdin,
                &mut stdout,
                &ws,
                &typesafe_only_env(),
                &HermeticJevProvider {
                    compliance_score: 0.95,
                },
                &session,
            )
            .expect("all vetted must exit 0 with TYPESAFE_API_KEY only");
            assert!(vetted_caseload.vetting.expect("vetting").is_all_vetted());
            let rendered = String::from_utf8(stdout).expect("utf8");
            assert!(rendered.contains("✓ vet       : 1 case(s) vetted, 0 contested"));
            assert!(rendered.contains("Vetting: PASS"));
            assert!(
                !rendered.contains("[vetted]"),
                "default stylish output must omit `[vetted]` detail blocks: {rendered}"
            );

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
                    PipelineStage::Vet,
                ]
            );

            // All-vetted `compact` exits 0 with empty stdout
            let mut compact_pass_out = Vec::new();
            run_vet_with_snapshot_and_provider(
                &VetArgs {
                    format: VetFormat::Compact,
                    ..args.clone()
                },
                &mut Cursor::new(diff.as_bytes()),
                &mut compact_pass_out,
                &ws,
                &typesafe_only_env(),
                &HermeticJevProvider {
                    compliance_score: 0.95,
                },
            )
            .expect("all vetted compact must exit 0");
            assert!(
                compact_pass_out.is_empty(),
                "all-vetted `compact` must emit empty stdout"
            );

            // Contested case exits 1 while rendering CONTESTED summary + compact TSV
            let mut stdin_fail = Cursor::new(diff.as_bytes());
            let mut stdout_fail = Vec::new();
            let contested_args = VetArgs {
                format: VetFormat::Compact,
                ..args
            };
            let err = run_vet_with_snapshot_and_provider(
                &contested_args,
                &mut stdin_fail,
                &mut stdout_fail,
                &ws,
                &typesafe_only_env(),
                &HermeticJevProvider {
                    compliance_score: 0.42,
                },
            )
            .expect_err("contested case must exit 1");
            assert_eq!(err.exit_code(), 1);
            assert!(err.is_silent());
            let tsv = String::from_utf8(stdout_fail).expect("utf8 tsv");
            assert!(tsv.starts_with(".canons/cli/kebab.md\tcontested\t0.42\t"));
        }

        #[test]
        fn test_mixed_vet_suppresses_vetted_rows_by_default_and_expands_under_verbose() {
            use crate::caseload::{CanonVettingAssessment, CaseloadIntake, VettingStatus};

            let ws = sample_ws();
            let provider = HermeticJevProvider {
                compliance_score: 0.95,
            };
            let vetted_canon = ".canons/cli/help-groups.md";
            let contested_canon = ".canons/cli/kebab.md";
            let caseload = Caseload::new()
                .with_intake(CaseloadIntake {
                    target_paths: vec!["src/cli/app.rs".to_string()],
                    ..CaseloadIntake::default()
                })
                .with_vetting(CaseloadVetting::from_cases(BTreeMap::from([
                    (
                        vetted_canon.to_string(),
                        CanonVettingAssessment {
                            vetting_summary: "Help headings grouped cleanly.".to_string(),
                            vetting_score: 0.94,
                            vetting_status: VettingStatus::Vetted,
                            contested_exhibits: Vec::new(),
                        },
                    ),
                    (
                        contested_canon.to_string(),
                        CanonVettingAssessment {
                            vetting_summary: "CamelCase flag suspected.".to_string(),
                            vetting_score: 0.38,
                            vetting_status: VettingStatus::Contested,
                            contested_exhibits: Vec::new(),
                        },
                    ),
                ])));
            let serialized = serde_json::to_vec(&caseload).expect("serialize mixed vet caseload");

            // 1. Default stylish prints strictly `[contested]` detail blocks
            let base_args = VetArgs {
                io: CaseloadIoArgs {
                    caseload: Some("-".to_string()),
                    ..CaseloadIoArgs::default()
                },
                ..VetArgs::default()
            };
            let mut default_out = Vec::new();
            let err = run_vet_with_snapshot_and_provider(
                &base_args,
                &mut Cursor::new(&serialized),
                &mut default_out,
                &ws,
                &BTreeMap::new(),
                &provider,
            )
            .expect_err("mixed vet must exit 1");
            assert_eq!(err.exit_code(), 1);
            let default_text = String::from_utf8(default_out).expect("utf8");
            assert!(
                default_text
                    .contains("Vetting: CONTESTED (1 vetted, 1 contested at threshold 0.80)")
            );
            assert!(default_text.contains("  - .canons/cli/kebab.md [contested] (score: 0.38)"));
            assert!(!default_text.contains("[vetted]"));
            assert!(!default_text.contains(vetted_canon));

            // 2. `-v` (`verbose: true`) expands `[vetted]` detail blocks in stylish mode
            let mut verbose_out = Vec::new();
            let _ = run_vet_with_snapshot_and_provider(
                &VetArgs {
                    verbose: true,
                    ..base_args.clone()
                },
                &mut Cursor::new(&serialized),
                &mut verbose_out,
                &ws,
                &BTreeMap::new(),
                &provider,
            )
            .expect_err("mixed vet must exit 1");
            let verbose_text = String::from_utf8(verbose_out).expect("utf8");
            assert!(verbose_text.contains("  - .canons/cli/help-groups.md [vetted] (score: 0.94)"));
            assert!(verbose_text.contains("  - .canons/cli/kebab.md [contested] (score: 0.38)"));

            // 3. `compact` emits strictly `contested` rows (even when `-v` is active)
            let mut compact_out = Vec::new();
            let _ = run_vet_with_snapshot_and_provider(
                &VetArgs {
                    format: VetFormat::Compact,
                    verbose: true,
                    ..base_args
                },
                &mut Cursor::new(&serialized),
                &mut compact_out,
                &ws,
                &BTreeMap::new(),
                &provider,
            )
            .expect_err("mixed vet must exit 1");
            let compact_text = String::from_utf8(compact_out).expect("utf8");
            assert_eq!(
                compact_text,
                ".canons/cli/kebab.md\tcontested\t0.38\tCamelCase flag suspected.\n"
            );
        }
    }
}
