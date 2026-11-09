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

//! CLI driving adapter for the `canon-clerk validate` subcommand (`docs/architecture/nodes/validate.md` §5).

use crate::caseload::{AnnotationLevel, Caseload, CaseloadValidation, CodeAnnotation};
use crate::cli::CliError;
use crate::cli::discover::{StdinSourceRefs, ensure_single_stdin_source, read_caseload_operand};
use crate::cli::intake::{read_operand_source, resolve_target_operands, write_caseload_file};
use crate::discover::{DiscoverError, DiscoverOptions, execute_discover_on_disk};
use crate::intake::{IntakeError, IntakeOptions, execute_intake};
use crate::validate::{ValidateError, ValidateOptions, execute_validate_on_disk};
use clap::{Args, ValueEnum};
use std::env;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const VALIDATE_EXAMPLES: &str = "\
Examples:
  # Validate full repository canon corpus (Codex Audit):
  canon-clerk validate --all-canons

  # Validate candidate canons triggered by an in-flight diff stream:
  git diff origin/main | canon-clerk validate --diff -

  # Validate candidates on an existing Caseload JSON:
  canon-clerk validate --caseload caseload-2.json --json

  # Fail when any warning is emitted:
  canon-clerk validate --all-canons --max-warnings 0";

/// Output presentation format for `canon-clerk validate`.
#[derive(ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ValidateFormat {
    /// Human-readable lint report with file paths, line/column coordinates, and summary counts.
    #[default]
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
    /// Target file paths, directories, globs, or `-` to read newline-delimited paths from stdin.
    #[arg(
        value_name = "TARGETS",
        help = "Target file paths, directories, globs, or '-' for stdin paths"
    )]
    pub targets: Vec<String>,

    /// Read unified diff patch from a file path or `-` for standard input.
    #[arg(
        long = "diff",
        value_name = "PATH|-",
        help_heading = "Filing Sources",
        help = "Ingest unified diff patch from file or '-' for stdin"
    )]
    pub diff: Option<String>,

    /// Prospective architectural design intent or plan description.
    #[arg(
        long = "intent",
        value_name = "TEXT",
        help_heading = "Filing Sources",
        help = "Ingest prospective architectural design intent"
    )]
    pub intent: Option<String>,

    /// Ingest all repository files as target artifacts (`scope: "all-targets"`).
    #[arg(
        long = "all-targets",
        help_heading = "Filing Sources",
        help = "Ingest all repository files as target artifacts"
    )]
    pub all_targets: bool,

    /// Pull request title or commit subject line.
    #[arg(
        long = "pr-title",
        value_name = "TEXT",
        help_heading = "Pull Request Metadata",
        help = "Ingest pull request title or commit subject text"
    )]
    pub pr_title: Option<String>,

    /// Pull request Markdown description or commit body.
    #[arg(
        long = "pr-body",
        value_name = "TEXT",
        conflicts_with = "pr_body_file",
        help_heading = "Pull Request Metadata",
        help = "Ingest pull request Markdown description text"
    )]
    pub pr_body: Option<String>,

    /// Read pull request Markdown description from a file path or `-` for stdin.
    #[arg(
        long = "pr-body-file",
        value_name = "PATH",
        conflicts_with = "pr_body",
        help_heading = "Pull Request Metadata",
        help = "Ingest pull request Markdown description from a file"
    )]
    pub pr_body_file: Option<String>,

    /// Validate all canons across the repository corpus, bypassing path trigger matching.
    #[arg(
        long = "all-canons",
        help_heading = "Validation Options",
        help = "Validate all repository canons, bypassing trigger matching"
    )]
    pub all_canons: bool,

    /// Explicitly restrict candidate canons to a specific file path or glob subset.
    #[arg(
        long = "canon",
        value_name = "PATH|GLOB",
        help_heading = "Validation Options",
        help = "Explicitly restrict candidate canons to a specific subset"
    )]
    pub canon: Vec<String>,

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

    /// Read an existing upstream `Caseload` JSON from a file path or `-` for stdin.
    #[arg(
        long = "caseload",
        value_name = "PATH|-",
        help_heading = "Caseload & Output",
        help = "Ingest existing upstream Caseload JSON from file or '-' for stdin"
    )]
    pub caseload: Option<String>,

    /// Write the cumulative `Caseload` JSON to a file path independently of stdout.
    #[arg(
        long = "output-caseload",
        value_name = "PATH",
        help_heading = "Caseload & Output",
        help = "Write cumulative Caseload JSON to disk independently of stdout"
    )]
    pub output_caseload: Option<PathBuf>,

    /// Emit the enriched `Caseload` as JSON to standard output.
    #[arg(
        long = "json",
        help_heading = "Caseload & Output",
        help = "Emit enriched Caseload JSON to stdout"
    )]
    pub json: bool,
}

/// Executes the `validate` CLI subcommand against the provided `stdin`, `stdout`, and `workspace_root`.
pub fn run_validate_in_workspace(
    args: &ValidateArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace_root: &Path,
) -> Result<Caseload, CliError> {
    ensure_single_stdin_source(&StdinSourceRefs {
        targets: &args.targets,
        diff: args.diff.as_deref(),
        pr_body_file: args.pr_body_file.as_deref(),
        caseload: args.caseload.as_deref(),
    })?;

    let incoming_caseload = read_caseload_operand(args.caseload.as_deref(), stdin)?;
    let intake_opts = build_intake_options(args, stdin)?;

    let caseload_after_intake = if intake_opts.has_designated_source() {
        execute_intake(&intake_opts, incoming_caseload).map_err(|err| match err {
            IntakeError::MissingInputSource | IntakeError::MalformedDiff(_) => {
                CliError::usage(err.to_string())
            }
        })?
    } else if let Some(existing) = incoming_caseload {
        existing
    } else if args.all_canons || !args.canon.is_empty() {
        Caseload::new()
    } else {
        return Err(CliError::usage(ValidateError::MissingDiscovery.to_string()));
    };

    let caseload_after_discover =
        if caseload_after_intake.discovery.is_some() && !args.all_canons && args.canon.is_empty() {
            caseload_after_intake
        } else {
            let discover_opts = DiscoverOptions {
                all_canons: args.all_canons
                    || (caseload_after_intake.intake.is_none() && !args.canon.is_empty()),
                canon_patterns: args.canon.clone(),
            };
            execute_discover_on_disk(&discover_opts, caseload_after_intake, workspace_root)
                .map_err(map_discover_error)?
        };

    let validate_opts = ValidateOptions {
        max_warnings: args.max_warnings,
    };

    let caseload =
        execute_validate_on_disk(&validate_opts, caseload_after_discover, workspace_root).map_err(
            |err| match err {
                ValidateError::MissingDiscovery => CliError::usage(err.to_string()),
                ValidateError::Discover(d) => map_discover_error(d),
            },
        )?;

    if let Some(output_path) = &args.output_caseload {
        write_caseload_file(output_path, &caseload)?;
    }

    let validation = caseload.validation.clone().unwrap_or_default();
    render_validate_output(args, &caseload, &validation, stdout)?;

    if validation.has_errors {
        return Err(CliError::silent(1));
    }

    Ok(caseload)
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

fn map_discover_error(err: DiscoverError) -> CliError {
    match err {
        DiscoverError::MissingInputSource => {
            CliError::usage(ValidateError::MissingDiscovery.to_string())
        }
        DiscoverError::InvalidGlob(_) | DiscoverError::PathEscapesWorkspace { .. } => {
            CliError::usage(err.to_string())
        }
        DiscoverError::Io { .. } => CliError::runtime(err.to_string()),
    }
}

fn build_intake_options(
    args: &ValidateArgs,
    stdin: &mut dyn Read,
) -> Result<IntakeOptions, CliError> {
    let patch_content = args
        .diff
        .as_deref()
        .map(|s| read_operand_source(s, "--diff", stdin))
        .transpose()?;
    let pr_body = if let Some(body) = &args.pr_body {
        Some(body.clone())
    } else if let Some(path) = &args.pr_body_file {
        Some(read_operand_source(path, "--pr-body-file", stdin)?)
    } else {
        None
    };
    Ok(IntakeOptions {
        pr_title: args.pr_title.clone(),
        pr_body,
        intent: args.intent.clone(),
        target_paths: resolve_target_operands(&args.targets, stdin)?,
        patch_content,
        all_targets: args.all_targets,
        linked_issues: None,
    })
}

fn render_validate_output(
    args: &ValidateArgs,
    caseload: &Caseload,
    validation: &CaseloadValidation,
    stdout: &mut dyn Write,
) -> Result<(), CliError> {
    let effective_format = if args.json {
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
                diff: Some("-".to_string()),
                caseload: Some("-".to_string()),
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
                all_canons: true,
                max_warnings: Some(0),
                json: true,
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
