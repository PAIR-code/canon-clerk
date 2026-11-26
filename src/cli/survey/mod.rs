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

//! CLI driving adapter for the `canon-clerk survey` subcommand (`docs/architecture/nodes/survey.md` §5).

mod render;

use crate::caseload::Caseload;
use crate::cli::CliError;
use crate::cli::args::CaseloadIoArgs;
use crate::cli::provenance::StageDisposition;
use crate::policy::PolicyError;
use crate::survey::{
    SurveyCeilingKind, SurveyError, SurveyOptions, execute_survey_on_disk, resolve_survey_ceiling,
};
use crate::telemetry::{PipelineStage, TelemetrySession};
use clap::{Args, ValueEnum};
use render::render_survey_output;
use std::collections::BTreeMap;
use std::env;
use std::io::{Read, Write};
use std::num::NonZeroUsize;
use std::path::Path;

const SURVEY_EXAMPLES: &str = "\
Examples:
  # Survey repository-identity files using <scope>/.canons/policy-chains.yaml and built-in defaults:
  canon-clerk survey

  # Override per-file and aggregate character ceilings:
  canon-clerk survey --survey-max-file-chars 2000 --survey-max-total-chars 8000

  # Override policy chains and defaults with explicit workspace-relative globs:
  canon-clerk survey --survey-context 'README.md' --survey-context 'docs/architecture.md'

  # Emit an explicit empty survey:
  canon-clerk survey --no-survey-context --json

  # Cache survey output as a reusable Caseload envelope:
  canon-clerk survey --output-caseload survey-caseload.json --json

  # Fast-forward a pre-populated survey Caseload:
  canon-clerk survey --caseload survey-caseload.json --json";

/// Output presentation format for `canon-clerk survey`.
#[derive(ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SurveyFormat {
    /// Human-readable repository survey summary with Pipeline Funnel Provenance receipt.
    #[default]
    Stylish,
    /// Full cumulative `Caseload` JSON envelope.
    Json,
    /// Tab-separated one-line-per-file output (`file\t<path>\t<chars>\t<orig>\t<state>`).
    Compact,
}

/// Command-line arguments for `canon-clerk survey`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
#[command(
    about = "Catalog bounded repository-identity context files onto .survey",
    long_about = "Repository Identity Cataloger of the Caseload DAG (Branch C). Deterministically discovers and bounds top-level documentation, manifests, and nestable <scope>/.canons/policy-chains.yaml `survey-context-chain:` files into `.survey` with zero tokens and zero credentials.",
    after_help = SURVEY_EXAMPLES
)]
pub struct SurveyArgs {
    /// Repeatable workspace-relative glob overriding `survey-context-chain:` and built-in defaults.
    #[arg(
        long = "survey-context",
        value_name = "GLOB",
        conflicts_with = "no_survey_context",
        help_heading = "Survey Policy & Ceilings",
        help = "Override survey-context-chain and defaults with explicit workspace-relative glob(s)"
    )]
    pub survey_context: Vec<String>,

    /// Explicitly disable repository-identity context collection (produces an empty `.survey`).
    #[arg(
        long = "no-survey-context",
        conflicts_with = "survey_context",
        help_heading = "Survey Policy & Ceilings",
        help = "Disable repository-identity context collection (emit empty .survey)"
    )]
    pub no_survey_context: bool,

    /// Maximum materialized characters per surveyed file (fallback env: `CANON_CLERK_SURVEY_MAX_FILE_CHARS`; default: 4000).
    #[arg(
        long = "survey-max-file-chars",
        value_name = "COUNT",
        help_heading = "Survey Policy & Ceilings",
        help = "Maximum materialized characters per file (default: 4000; fallback env: CANON_CLERK_SURVEY_MAX_FILE_CHARS)"
    )]
    pub survey_max_file_chars: Option<NonZeroUsize>,

    /// Maximum aggregate materialized characters across all surveyed files (fallback env: `CANON_CLERK_SURVEY_MAX_TOTAL_CHARS`; default: 12000).
    #[arg(
        long = "survey-max-total-chars",
        value_name = "COUNT",
        help_heading = "Survey Policy & Ceilings",
        help = "Maximum aggregate materialized characters across all files (default: 12000; fallback env: CANON_CLERK_SURVEY_MAX_TOTAL_CHARS)"
    )]
    pub survey_max_total_chars: Option<NonZeroUsize>,

    /// Output format (`stylish`, `json`, or `compact`).
    #[arg(
        long = "format",
        value_enum,
        default_value_t = SurveyFormat::Stylish,
        help_heading = "Caseload & Output",
        help = "Format repository survey output"
    )]
    pub format: SurveyFormat,

    #[command(flatten)]
    pub io: CaseloadIoArgs,
}

impl SurveyArgs {
    /// Returns `true` when any explicit `--survey-*` or `--no-survey-context` override flag was supplied.
    pub fn has_explicit_overrides(&self) -> bool {
        !self.survey_context.is_empty()
            || self.no_survey_context
            || self.survey_max_file_chars.is_some()
            || self.survey_max_total_chars.is_some()
    }
}

/// Maps a domain [`SurveyError`] to the normative CLI exit code (`2` for usage/policy/glob errors, `1` for I/O errors).
pub fn map_survey_error(err: SurveyError) -> CliError {
    match err {
        SurveyError::ConflictingContextFlags
        | SurveyError::InvalidCeiling { .. }
        | SurveyError::InvalidContextGlob { .. }
        | SurveyError::PathEscapesWorkspace { .. }
        | SurveyError::Policy(PolicyError::InvalidPolicyFile { .. }) => {
            CliError::usage(err.to_string())
        }
        SurveyError::Io { .. } | SurveyError::Policy(PolicyError::Io { .. }) => {
            CliError::runtime(err.to_string())
        }
    }
}

/// Executes the `survey` CLI subcommand against an explicit workspace root, env map, and [`TelemetrySession`].
///
/// # Errors
///
/// Returns [`CliError`] with exit code `2` on conflicting `--survey-context` / `--no-survey-context` flags,
/// malformed `--caseload` input, invalid or zero character ceilings, malformed `<scope>/.canons/policy-chains.yaml`
/// policy files, or invalid/escaping `--survey-context` globs; returns exit code `1` on local workspace,
/// `--output-caseload`, or `stdout` I/O failures.
pub fn run_survey_with_env_and_telemetry(
    args: &SurveyArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace_root: &Path,
    env: &BTreeMap<String, String>,
    telemetry: &TelemetrySession<'_>,
) -> Result<Caseload, CliError> {
    if args.no_survey_context && !args.survey_context.is_empty() {
        return Err(map_survey_error(SurveyError::ConflictingContextFlags));
    }

    let incoming_caseload = args.io.read_caseload(stdin)?;

    let (caseload, disp) = if incoming_caseload
        .as_ref()
        .is_some_and(|existing| existing.survey.is_some())
        && !args.has_explicit_overrides()
    {
        (
            incoming_caseload.unwrap_or_default(),
            StageDisposition::PrePopulated,
        )
    } else {
        let max_file_chars = resolve_survey_ceiling(
            SurveyCeilingKind::MaxFileChars,
            args.survey_max_file_chars,
            env,
        )
        .map_err(map_survey_error)?;
        let max_total_chars = resolve_survey_ceiling(
            SurveyCeilingKind::MaxTotalChars,
            args.survey_max_total_chars,
            env,
        )
        .map_err(map_survey_error)?;

        let options = SurveyOptions {
            context_globs: args.survey_context.clone(),
            no_survey_context: args.no_survey_context,
            max_file_chars,
            max_total_chars,
        };

        let updated = telemetry.run_stage(PipelineStage::Survey, |_scope| {
            execute_survey_on_disk(&options, incoming_caseload, workspace_root)
                .map_err(map_survey_error)
        })?;
        (updated, StageDisposition::Executed)
    };

    telemetry.finish_pipeline();
    args.io.write_output_caseload(&caseload)?;

    let survey = caseload.survey.clone().unwrap_or_default();
    render_survey_output(args, &caseload, &survey, disp, stdout)?;

    Ok(caseload)
}

/// Executes the `survey` CLI subcommand against an explicit workspace root and [`TelemetrySession`] using process env.
pub fn run_survey_with_telemetry(
    args: &SurveyArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace_root: &Path,
    telemetry: &TelemetrySession<'_>,
) -> Result<Caseload, CliError> {
    let env: BTreeMap<String, String> = env::vars().collect();
    run_survey_with_env_and_telemetry(args, stdin, stdout, workspace_root, &env, telemetry)
}

/// Executes the `survey` CLI subcommand against an explicit workspace root without telemetry.
pub fn run_survey_in_workspace(
    args: &SurveyArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace_root: &Path,
) -> Result<Caseload, CliError> {
    run_survey_with_telemetry(
        args,
        stdin,
        stdout,
        workspace_root,
        &TelemetrySession::noop(),
    )
}

/// Executes the `survey` CLI subcommand against the current working directory.
///
/// # Errors
///
/// Returns [`CliError`] with exit code `1` if the current working directory cannot be determined or
/// local workspace/output I/O fails, or exit code `2` on invalid flags, ceilings, policy files, or caseload input.
pub fn run_survey(
    args: &SurveyArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    let cwd = env::current_dir().map_err(|err| {
        CliError::runtime(format!(
            "Failed to determine current working directory: {err}"
        ))
    })?;
    run_survey_in_workspace(args, stdin, stdout, &cwd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{CaseloadIntake, SurveyOmissionReason};
    use crate::cli::{Cli, Commands};
    use crate::survey::{ENV_SURVEY_MAX_FILE_CHARS, ENV_SURVEY_MAX_TOTAL_CHARS};
    use crate::telemetry::{RecordingTelemetrySink, SteppingTelemetryClock, TelemetryEvent};
    use clap::Parser;
    use std::fs;
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_workspace(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("canon_clerk_cli_survey_{label}_{nanos}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn test_cli_parse_survey_flags_and_reject_out_of_closure_flags() {
        let cli = Cli::try_parse_from([
            "canon-clerk",
            "survey",
            "--survey-context",
            "README.md",
            "--survey-max-file-chars",
            "2000",
            "--survey-max-total-chars",
            "6000",
            "--format",
            "compact",
        ])
        .expect("parse survey subcommand");

        match cli.command {
            Some(Commands::Survey(args)) => {
                assert_eq!(args.survey_context, vec!["README.md"]);
                assert_eq!(args.survey_max_file_chars, NonZeroUsize::new(2000));
                assert_eq!(args.survey_max_total_chars, NonZeroUsize::new(6000));
                assert_eq!(args.format, SurveyFormat::Compact);
            }
            _ => panic!("expected Survey subcommand"),
        }

        // Zero ceiling is rejected by Clap at the CLI boundary
        let zero_err =
            Cli::try_parse_from(["canon-clerk", "survey", "--survey-max-file-chars", "0"])
                .expect_err("zero --survey-max-file-chars must fail");
        assert_eq!(zero_err.kind(), clap::error::ErrorKind::ValueValidation);

        // Mutually exclusive --survey-context and --no-survey-context
        let conflict = Cli::try_parse_from([
            "canon-clerk",
            "survey",
            "--survey-context",
            "README.md",
            "--no-survey-context",
        ])
        .expect_err("conflicting survey flags must fail");
        assert_eq!(conflict.kind(), clap::error::ErrorKind::ArgumentConflict);

        // Out-of-closure commands must reject --survey-* and --no-survey-context
        for sub in [
            "intake",
            "discover",
            "validate",
            "configure",
            "probe",
            "apprise",
            "docket",
            "admit",
            "vet",
            "audit",
        ] {
            for argv in [
                vec!["canon-clerk", sub, "--survey-context", "README.md"],
                vec!["canon-clerk", sub, "--no-survey-context"],
                vec!["canon-clerk", sub, "--survey-max-file-chars", "1000"],
                vec!["canon-clerk", sub, "--survey-max-total-chars", "2000"],
            ] {
                let err = Cli::try_parse_from(&argv)
                    .expect_err("out-of-closure subcommand must reject survey flags");
                assert_eq!(
                    err.kind(),
                    clap::error::ErrorKind::UnknownArgument,
                    "{argv:?} must fail with UnknownArgument"
                );
            }
        }
    }

    #[test]
    fn test_survey_cli_emits_telemetry_and_fast_forwards_caseload() {
        let root = make_temp_workspace("replay");
        fs::write(root.join("README.md"), "# Canon Clerk\n").unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();

        let args = SurveyArgs {
            io: CaseloadIoArgs {
                json: true,
                ..CaseloadIoArgs::default()
            },
            ..SurveyArgs::default()
        };
        let sink = RecordingTelemetrySink::new();
        let clock = SteppingTelemetryClock::new(1_700_000_000_000, 5);
        let session = TelemetrySession::new(&sink, &clock);

        let mut stdin = Cursor::new(b"");
        let mut stdout = Vec::new();
        let first = run_survey_with_env_and_telemetry(
            &args,
            &mut stdin,
            &mut stdout,
            &root,
            &BTreeMap::new(),
            &session,
        )
        .expect("initial survey");
        let events = sink.events();
        assert_eq!(events.len(), 3);
        assert!(matches!(
            events[0],
            TelemetryEvent::StageStart {
                stage: PipelineStage::Survey,
                ..
            }
        ));
        assert!(matches!(
            events[1],
            TelemetryEvent::StageFinish {
                stage: PipelineStage::Survey,
                usage: None,
                ..
            }
        ));
        assert!(matches!(
            events[2],
            TelemetryEvent::PipelineFinish {
                total_tokens: 0,
                ..
            }
        ));

        let pre_populated = first.with_intake(CaseloadIntake {
            intent: Some("plan".to_string()),
            ..CaseloadIntake::default()
        });
        let raw_json = serde_json::to_string(&pre_populated).expect("serialize");
        let replay_args = SurveyArgs {
            io: CaseloadIoArgs {
                caseload: Some("-".to_string()),
                ..CaseloadIoArgs::default()
            },
            ..SurveyArgs::default()
        };
        let replay_sink = RecordingTelemetrySink::new();
        let replay_session = TelemetrySession::new(&replay_sink, &clock);
        let mut replay_stdin = Cursor::new(raw_json.as_bytes());
        let mut replay_stdout = Vec::new();
        let replayed = run_survey_with_env_and_telemetry(
            &replay_args,
            &mut replay_stdin,
            &mut replay_stdout,
            &root,
            &BTreeMap::new(),
            &replay_session,
        )
        .expect("fast-forward replay");
        assert_eq!(replayed, pre_populated);
        assert_eq!(replay_sink.events().len(), 1);
        let rendered = String::from_utf8(replay_stdout).expect("utf8");
        assert!(rendered.contains("↷ survey    : 2 context files"));
        assert!(rendered.contains("(fast-forward: pre-populated)"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_survey_cli_override_reexecutes_prepopulated_caseload() {
        let root = make_temp_workspace("override_replay");
        fs::write(root.join("README.md"), "# Canon Clerk\n").unwrap();

        let initial = execute_survey_on_disk(&SurveyOptions::default(), None, &root)
            .expect("initial survey")
            .with_intake(CaseloadIntake {
                intent: Some("plan".to_string()),
                ..CaseloadIntake::default()
            });
        let raw_json = serde_json::to_string(&initial).expect("serialize");

        let override_args = SurveyArgs {
            survey_max_file_chars: NonZeroUsize::new(4),
            io: CaseloadIoArgs {
                caseload: Some("-".to_string()),
                ..CaseloadIoArgs::default()
            },
            ..SurveyArgs::default()
        };
        let mut ov_stdin = Cursor::new(raw_json.as_bytes());
        let mut ov_stdout = Vec::new();
        let overridden = run_survey_with_env_and_telemetry(
            &override_args,
            &mut ov_stdin,
            &mut ov_stdout,
            &root,
            &BTreeMap::new(),
            &TelemetrySession::noop(),
        )
        .expect("override replay");
        let survey = overridden.survey.expect("survey");
        assert_eq!(survey.max_file_chars, 4);
        assert_eq!(survey.files["README.md"].chars, 4);
        assert!(survey.files["README.md"].truncated);
        assert_eq!(overridden.intake, initial.intake);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_survey_cli_env_ceilings_and_compact_format() {
        let root = make_temp_workspace("env_compact");
        fs::write(root.join("Cargo.toml"), "1234567890").unwrap();
        fs::write(root.join("README.md"), "abcdefghij").unwrap();

        let mut env = BTreeMap::new();
        env.insert(ENV_SURVEY_MAX_FILE_CHARS.to_string(), "5".to_string());
        env.insert(ENV_SURVEY_MAX_TOTAL_CHARS.to_string(), "5".to_string());

        let args = SurveyArgs {
            format: SurveyFormat::Compact,
            ..SurveyArgs::default()
        };
        let mut stdin = Cursor::new(b"");
        let mut stdout = Vec::new();
        let caseload = run_survey_with_env_and_telemetry(
            &args,
            &mut stdin,
            &mut stdout,
            &root,
            &env,
            &TelemetrySession::noop(),
        )
        .expect("env ceiling survey");

        let survey = caseload.survey.expect("survey");
        assert_eq!(
            survey.omitted.get("README.md"),
            Some(&SurveyOmissionReason::BudgetExceeded)
        );
        let out = String::from_utf8(stdout).expect("utf8");
        assert!(out.contains("file\tCargo.toml\t5\t10\ttruncated"));
        assert!(out.contains("omitted\tREADME.md\tbudget_exceeded"));

        // Invalid zero env ceiling fails with exit code 2
        let mut bad_env = BTreeMap::new();
        bad_env.insert(ENV_SURVEY_MAX_FILE_CHARS.to_string(), "0".to_string());
        let err = run_survey_with_env_and_telemetry(
            &args,
            &mut Cursor::new(b""),
            &mut Vec::new(),
            &root,
            &bad_env,
            &TelemetrySession::noop(),
        )
        .expect_err("zero env ceiling must fail");
        assert_eq!(err.exit_code(), 2);

        let _ = fs::remove_dir_all(&root);
    }
}
