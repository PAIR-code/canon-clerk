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

//! CLI driving adapter for the `canon-clerk validate` subcommand.

use crate::caseload::{AnnotationLevel, Caseload, CaseloadValidation, CodeAnnotation};
use crate::cli::CliError;
use crate::cli::args::{CanonSelectionArgs, CaseloadIoArgs, FilingSourceArgs};
use crate::cli::io::ensure_single_stdin_source;
use crate::cli::runners::run_branch_a_validate;
use crate::telemetry::TelemetrySession;
use clap::{Args, ValueEnum};
use std::env;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;

const VALIDATE_EXAMPLES: &str = "\
Examples:
  # Validate candidate canons matching the working tree diff:
  git diff origin/main | canon-clerk validate --diff -

  # Validate candidate canons for explicit target paths or globs:
  canon-clerk validate src/lib.rs 'src/**/*.rs'

  # Validate all repository canons (pre-commit or CI statutory lint gate):
  canon-clerk validate --all-canons

  # Enforce zero warnings in strict CI mode:
  canon-clerk validate --all-canons --max-warnings 0

  # Emit tab-separated diagnostics for IDE or editor integration:
  canon-clerk validate --all-canons --format compact

  # Emit full cumulative Caseload JSON to stdout:
  canon-clerk validate --all-canons --json";

/// Output presentation format for `canon-clerk validate`.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidateFormat {
    /// Human-readable terminal output with pass/fail summary and line diagnostics.
    Stylish,
    /// Full cumulative `Caseload` JSON envelope.
    Json,
    /// Tab-separated (`<path>\t<line>:<col>\t<level>\t<title>\t<message>`) diagnostic lines.
    Compact,
    /// GitHub Actions workflow command annotations (`::error file=...,line=...,col=...,title=...::...`).
    Github,
}

/// Command-line arguments for `canon-clerk validate`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
#[command(
    about = "Validate candidate canon Markdown AST and frontmatter schemas",
    long_about = "Statutory Validity Gate of the Caseload DAG (Branch A). Performs pre-flight static linting of candidate canon Markdown bodies and YAML frontmatter blocks, populating `.validation` on the cumulative Caseload and aborting with exit code 1 before model token spend if errors are detected.",
    after_help = VALIDATE_EXAMPLES
)]
pub struct ValidateArgs {
    #[command(flatten)]
    pub filing: FilingSourceArgs,

    #[command(flatten)]
    pub canons: CanonSelectionArgs,

    /// Maximum number of warnings allowed before exiting with non-zero status (`1`).
    #[arg(
        long = "max-warnings",
        value_name = "N",
        help_heading = "Validation Options",
        help = "Warning threshold before triggering non-zero exit code"
    )]
    pub max_warnings: Option<usize>,

    /// Quiet mode: suppress warnings and non-essential output.
    #[arg(
        short = 'q',
        long = "quiet",
        help_heading = "Validation Options",
        help = "Suppress warnings and non-essential output"
    )]
    pub quiet: bool,

    /// Output format (`stylish`, `json`, `compact`, or `github`; defaults to `github` when `GITHUB_ACTIONS=true`).
    #[arg(
        long = "format",
        value_enum,
        help_heading = "Caseload & Output",
        help = "Output formatting choice (defaults to 'github' when GITHUB_ACTIONS=true)"
    )]
    pub format: Option<ValidateFormat>,

    #[command(flatten)]
    pub io: CaseloadIoArgs,
}

/// Executes the `validate` CLI subcommand against `workspace_root` with an explicit [`TelemetrySession`].
pub fn run_validate_with_telemetry(
    args: &ValidateArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace_root: &Path,
    telemetry: &TelemetrySession<'_>,
) -> Result<Caseload, CliError> {
    ensure_single_stdin_source(&args.filing, args.io.caseload.as_deref())?;

    let incoming_caseload = args.io.read_caseload(stdin)?;
    let caseload = run_branch_a_validate(
        &args.filing,
        &args.canons,
        incoming_caseload,
        workspace_root,
        stdin,
        args.max_warnings,
        telemetry,
    )?;
    telemetry.finish_pipeline();

    args.io.write_output_caseload(&caseload)?;

    let validation = caseload.validation.clone().unwrap_or_default();
    render_validate_output(args, &caseload, &validation, stdout)?;

    if validation.has_errors {
        return Err(CliError::silent(1));
    }

    Ok(caseload)
}

/// Executes the `validate` CLI subcommand against the provided `stdin`, `stdout`, and `workspace_root`.
pub fn run_validate_in_workspace(
    args: &ValidateArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace_root: &Path,
) -> Result<Caseload, CliError> {
    run_validate_with_telemetry(
        args,
        stdin,
        stdout,
        workspace_root,
        &TelemetrySession::noop(),
    )
}

/// Executes the `validate` CLI subcommand using the current working directory as `workspace_root`.
pub fn run_validate(
    args: &ValidateArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    let cwd = env::current_dir().map_err(|err| {
        CliError::runtime(format!(
            "Failed to determine current working directory: {err}"
        ))
    })?;
    run_validate_in_workspace(args, stdin, stdout, &cwd)
}

fn render_validate_output(
    args: &ValidateArgs,
    caseload: &Caseload,
    validation: &CaseloadValidation,
    stdout: &mut dyn Write,
) -> Result<(), CliError> {
    let effective_format = if args.io.json {
        ValidateFormat::Json
    } else if let Some(explicit) = args.format {
        explicit
    } else if env::var("GITHUB_ACTIONS").ok().as_deref() == Some("true") {
        ValidateFormat::Github
    } else {
        ValidateFormat::Stylish
    };

    match effective_format {
        ValidateFormat::Json => {
            let serialized = serde_json::to_string_pretty(caseload).map_err(|e| {
                CliError::runtime(format!("Failed to serialize Caseload JSON: {e}"))
            })?;
            writeln!(stdout, "{serialized}")
                .map_err(|e| CliError::runtime(format!("Failed to write JSON to stdout: {e}")))?;
        }
        ValidateFormat::Compact | ValidateFormat::Github => {
            render_compact_or_github(validation, args.quiet, effective_format, stdout)
                .map_err(|e| CliError::runtime(format!("Failed to write output to stdout: {e}")))?;
            if effective_format == ValidateFormat::Github
                && let Ok(summary_path) = env::var("GITHUB_STEP_SUMMARY")
                && !summary_path.trim().is_empty()
            {
                let _ = append_github_step_summary(Path::new(&summary_path), validation);
            }
        }
        ValidateFormat::Stylish => {
            if !args.quiet || validation.has_errors {
                render_stylish_output(validation, args.quiet, stdout).map_err(|e| {
                    CliError::runtime(format!("Failed to write summary to stdout: {e}"))
                })?;
            }
        }
    }

    Ok(())
}

fn render_compact_or_github(
    validation: &CaseloadValidation,
    quiet: bool,
    format: ValidateFormat,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    for meta in validation.results.values() {
        for diag in &meta.diagnostics {
            if quiet && diag.annotation_level == AnnotationLevel::Warning {
                continue;
            }
            if format == ValidateFormat::Github {
                writeln!(stdout, "{}", format_github_annotation(diag))?;
            } else {
                let col = diag.start_column.unwrap_or(1);
                let title = diag.title.as_deref().unwrap_or("canon-lint");
                writeln!(
                    stdout,
                    "{}\t{}:{col}\t{}\t{title}\t{}",
                    diag.path,
                    diag.start_line,
                    diag.annotation_level.as_str(),
                    diag.message
                )?;
            }
        }
    }
    Ok(())
}

fn format_github_annotation(diag: &CodeAnnotation) -> String {
    let command = match diag.annotation_level {
        AnnotationLevel::Failure => "error",
        AnnotationLevel::Warning => "warning",
        AnnotationLevel::Notice => "notice",
    };
    let col = diag.start_column.unwrap_or(1);
    let title = diag.title.as_deref().unwrap_or("Canon Validation");
    format!(
        "::{command} file={},line={},col={col},title={title}::{}",
        diag.path, diag.start_line, diag.message
    )
}

fn append_github_step_summary(
    summary_path: &Path,
    validation: &CaseloadValidation,
) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(summary_path)?;
    let (total_errors, total_warnings) = tally_counts(validation);
    let status = if validation.has_errors {
        "Failed"
    } else {
        "Passed"
    };
    writeln!(file, "### Canon Clerk Validation Summary: {status}")?;
    writeln!(
        file,
        "- **Candidate Canons Evaluated:** {}",
        validation.results.len()
    )?;
    writeln!(file, "- **Errors:** {total_errors}")?;
    writeln!(file, "- **Warnings:** {total_warnings}")?;
    Ok(())
}

fn render_stylish_output(
    validation: &CaseloadValidation,
    quiet: bool,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    let (total_errors, total_warnings) = tally_counts(validation);
    let status = if validation.has_errors {
        "FAIL"
    } else {
        "PASS"
    };

    writeln!(stdout, "Validation Summary ({status}):")?;
    writeln!(
        stdout,
        "  Canons Evaluated : {} canon(s)",
        validation.results.len()
    )?;
    writeln!(stdout, "  Errors           : {total_errors}")?;
    if !quiet {
        writeln!(stdout, "  Warnings         : {total_warnings}")?;
    }

    for (canon_ref, meta) in &validation.results {
        if quiet && meta.error_count == 0 {
            continue;
        }
        writeln!(
            stdout,
            "    - {canon_ref} [{}] ({} error(s), {} warning(s))",
            meta.result.as_str(),
            meta.error_count,
            meta.warning_count
        )?;
        for diag in &meta.diagnostics {
            if quiet && diag.annotation_level == AnnotationLevel::Warning {
                continue;
            }
            let col = diag.start_column.unwrap_or(1);
            let title = diag.title.as_deref().unwrap_or("canon-lint");
            writeln!(
                stdout,
                "        {}:{}:{col} [{}] ({title}): {}",
                diag.path,
                diag.start_line,
                diag.annotation_level.as_str(),
                diag.message
            )?;
        }
    }

    Ok(())
}

fn tally_counts(validation: &CaseloadValidation) -> (usize, usize) {
    let mut errors = 0usize;
    let mut warnings = 0usize;
    for meta in validation.results.values() {
        errors += meta.error_count;
        warnings += meta.warning_count;
    }
    (errors, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    mod source_and_stdin_guards {
        use super::*;

        #[test]
        fn test_naked_validate_fails_with_exit_code_2() {
            let args = ValidateArgs::default();
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let err = run_validate(&args, &mut stdin, &mut stdout).expect_err("naked must fail");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("Hint:"));
        }

        #[test]
        fn test_conflicting_stdin_operands_rejected_with_exit_code_2() {
            let args = ValidateArgs {
                filing: FilingSourceArgs {
                    diff: Some("-".to_string()),
                    ..FilingSourceArgs::default()
                },
                io: CaseloadIoArgs {
                    caseload: Some("-".to_string()),
                    ..CaseloadIoArgs::default()
                },
                ..ValidateArgs::default()
            };
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let err =
                run_validate(&args, &mut stdin, &mut stdout).expect_err("multiple stdin must fail");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("Hint:"));
        }
    }

    mod execution_and_formats {
        use super::*;

        #[test]
        fn test_all_canons_validates_repository_corpus_and_exits_0() {
            let args = ValidateArgs {
                canons: CanonSelectionArgs {
                    all_canons: true,
                    ..CanonSelectionArgs::default()
                },
                max_warnings: Some(0),
                io: CaseloadIoArgs {
                    json: true,
                    ..CaseloadIoArgs::default()
                },
                ..ValidateArgs::default()
            };
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let caseload =
                run_validate(&args, &mut stdin, &mut stdout).expect("validate --all-canons");
            let val = caseload.validation.expect("validation populated");
            assert!(!val.has_errors);
            assert!(!val.results.is_empty());
        }

        #[test]
        fn test_github_annotation_formatting_matches_workflow_command_spec() {
            let diag = CodeAnnotation {
                path: ".canons/security/rule.md".to_string(),
                start_line: 14,
                end_line: 14,
                start_column: Some(5),
                end_column: Some(8),
                annotation_level: AnnotationLevel::Failure,
                message: "Normative keyword 'must' must be uppercase 'MUST'.".to_string(),
                title: Some("RFC 2119 Violation".to_string()),
            };
            assert_eq!(
                format_github_annotation(&diag),
                "::error file=.canons/security/rule.md,line=14,col=5,title=RFC 2119 Violation::Normative keyword 'must' must be uppercase 'MUST'."
            );
        }
    }
}
