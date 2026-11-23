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
use crate::cli::io::ensure_single_stdin_source_with_canons;
use crate::cli::runners::run_branch_a_validate;
use crate::cli::telemetry::resolve_verbose_flag;
use crate::telemetry::TelemetrySession;
use clap::{Args, ValueEnum};
use std::collections::BTreeMap;
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

    /// Propagated top-level `-v` / `--verbose` flag (`CANON_CLERK_VERBOSE`).
    #[arg(skip)]
    pub verbose: bool,
}

/// Executes the `validate` CLI subcommand against `workspace_root` with an explicit [`TelemetrySession`].
pub fn run_validate_with_telemetry(
    args: &ValidateArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace_root: &Path,
    telemetry: &TelemetrySession<'_>,
) -> Result<Caseload, CliError> {
    let env_map: BTreeMap<String, String> = env::vars().collect();
    run_validate_with_env_and_telemetry(args, stdin, stdout, workspace_root, &env_map, telemetry)
}

pub(crate) fn run_validate_with_env_and_telemetry(
    args: &ValidateArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace_root: &Path,
    env: &BTreeMap<String, String>,
    telemetry: &TelemetrySession<'_>,
) -> Result<Caseload, CliError> {
    ensure_single_stdin_source_with_canons(
        &args.filing,
        Some(&args.canons),
        args.io.caseload.as_deref(),
    )?;
    let verbose = resolve_verbose_flag(args.verbose, env)?;

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
    render_validate_output(args, &caseload, &validation, env, verbose, stdout)?;

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
    env: &BTreeMap<String, String>,
    verbose: bool,
    stdout: &mut dyn Write,
) -> Result<(), CliError> {
    let effective_format = if args.io.json {
        ValidateFormat::Json
    } else if let Some(explicit) = args.format {
        explicit
    } else if env.get("GITHUB_ACTIONS").map(String::as_str) == Some("true") {
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
                && let Some(summary_path) = env
                    .get("GITHUB_STEP_SUMMARY")
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
            {
                let _ = append_github_step_summary(Path::new(summary_path), validation);
            }
        }
        ValidateFormat::Stylish => {
            if !args.quiet || validation.has_errors {
                render_stylish_output(validation, args.quiet, verbose, stdout).map_err(|e| {
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
    verbose: bool,
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
        if quiet {
            if meta.error_count == 0 {
                continue;
            }
        } else {
            let has_findings =
                meta.error_count > 0 || meta.warning_count > 0 || !meta.diagnostics.is_empty();
            if !has_findings && !verbose {
                continue;
            }
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
        use std::fs;
        use std::path::PathBuf;
        use std::time::{SystemTime, UNIX_EPOCH};

        struct TempDir {
            path: PathBuf,
        }

        impl TempDir {
            fn new(label: &str) -> Self {
                let unique = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos();
                let path = env::temp_dir().join(format!(
                    "canon-clerk-validate-cli-{label}-{unique}-{}",
                    std::process::id()
                ));
                let _ = fs::remove_dir_all(&path);
                fs::create_dir_all(&path).expect("create temp dir");
                Self { path }
            }

            fn path(&self) -> &Path {
                &self.path
            }

            fn write_file(&self, rel_path: &str, content: &str) {
                let full_path = self.path.join(rel_path);
                if let Some(parent) = full_path.parent() {
                    fs::create_dir_all(parent).expect("create parent dir");
                }
                fs::write(full_path, content).expect("write fixture file");
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.path);
            }
        }

        #[test]
        fn test_all_canons_validates_repository_corpus_and_exits_0() {
            let fixture = TempDir::new("all-canons");
            fixture.write_file(
                ".canons/cli/flags-must-use-positive-polarity.canon.md",
                "---\ntriggers:\n  - \"src/cli/**/*.rs\"\ntags:\n  - cli\n---\nCLI flags MUST use positive polarity.\n",
            );
            fixture.write_file(
                ".canons/rust/modules-must-document-public-items.canon.md",
                "---\ntriggers:\n  - \"src/**/*.rs\"\ntags:\n  - rust\n---\nPublic items MUST include rustdoc comments.\n",
            );
            fixture.write_file(
                ".canons/README.md",
                "# Local Canons\n\nNon-canon docs must be ignored by validate.\n",
            );

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
                run_validate_in_workspace(&args, &mut stdin, &mut stdout, fixture.path())
                    .expect("validate --all-canons");
            let val = caseload.validation.expect("validation populated");
            assert!(!val.has_errors);
            assert_eq!(
                val.results.len(),
                2 + crate::builtin::builtin_canons().len()
            );

            let no_builtins_args = ValidateArgs {
                canons: CanonSelectionArgs {
                    all_canons: true,
                    no_builtins: true,
                    ..CanonSelectionArgs::default()
                },
                ..args
            };
            let mut no_builtins_stdout = Vec::new();
            let no_builtins_cl = run_validate_in_workspace(
                &no_builtins_args,
                &mut stdin,
                &mut no_builtins_stdout,
                fixture.path(),
            )
            .expect("validate --all-canons --no-builtins");
            let no_builtins_val = no_builtins_cl.validation.expect("validation populated");
            assert!(!no_builtins_val.has_errors);
            assert_eq!(no_builtins_val.results.len(), 2);
        }

        #[test]
        fn test_github_actions_env_emits_annotations_and_populates_step_summary() {
            let fixture = TempDir::new("github-actions");
            fixture.write_file(
                ".canons/security/tokens-must-expire.canon.md",
                "---\ntriggers:\n  - \"src/**/*.rs\"\ntags:\n  - security\n---\nTokens must expire within one hour.\n",
            );
            let summary_path = fixture.path().join("step-summary.md");
            let env_map = BTreeMap::from([
                ("GITHUB_ACTIONS".to_string(), "true".to_string()),
                (
                    "GITHUB_STEP_SUMMARY".to_string(),
                    summary_path.to_string_lossy().into_owned(),
                ),
            ]);
            let args = ValidateArgs {
                canons: CanonSelectionArgs {
                    all_canons: true,
                    ..CanonSelectionArgs::default()
                },
                max_warnings: Some(0),
                ..ValidateArgs::default()
            };

            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let err = run_validate_with_env_and_telemetry(
                &args,
                &mut stdin,
                &mut stdout,
                fixture.path(),
                &env_map,
                &TelemetrySession::noop(),
            )
            .expect_err("RFC 2119 error must exit 1");
            assert_eq!(err.exit_code(), 1);

            let rendered = String::from_utf8(stdout).expect("utf8");
            assert!(
                rendered.contains(
                    "::error file=.canons/security/tokens-must-expire.canon.md,line=7,col=8,title=RFC 2119 Violation::"
                ),
                "expected GitHub Actions workflow annotation, got: {rendered}"
            );
            let summary = fs::read_to_string(&summary_path).expect("read step summary");
            assert!(summary.contains("### Canon Clerk Validation Summary: Failed"));
            assert!(summary.contains("- **Candidate Canons Evaluated:** 1"));
            assert!(summary.contains("- **Errors:** 1"));
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

        #[test]
        fn test_stylish_clean_corpus_suppresses_pass_rows_unless_verbose() {
            use crate::caseload::{CaseloadDiscovery, ValidatedCanonMetadata, ValidationOutcome};

            let clean_canon = ".canons/cli/clean-rule.md";
            let all_clean_caseload = Caseload::new()
                .with_discovery(CaseloadDiscovery {
                    candidate_canons: vec![clean_canon.to_string()],
                    ..CaseloadDiscovery::default()
                })
                .with_validation(CaseloadValidation {
                    has_errors: false,
                    results: BTreeMap::from([(
                        clean_canon.to_string(),
                        ValidatedCanonMetadata {
                            result: ValidationOutcome::Pass,
                            warning_count: 0,
                            error_count: 0,
                            diagnostics: Vec::new(),
                        },
                    )]),
                });
            let clean_bytes = serde_json::to_vec(&all_clean_caseload).expect("serialize clean");
            let caseload_args = ValidateArgs {
                io: CaseloadIoArgs {
                    caseload: Some("-".to_string()),
                    ..CaseloadIoArgs::default()
                },
                ..ValidateArgs::default()
            };

            let mut default_clean_out = Vec::new();
            run_validate_with_env_and_telemetry(
                &caseload_args,
                &mut Cursor::new(&clean_bytes),
                &mut default_clean_out,
                Path::new("."),
                &BTreeMap::new(),
                &TelemetrySession::noop(),
            )
            .expect("clean validate exits 0");
            let default_clean_text = String::from_utf8(default_clean_out).expect("utf8");
            assert!(default_clean_text.contains("Validation Summary (PASS):"));
            assert!(default_clean_text.contains("Canons Evaluated : 1 canon(s)"));
            assert!(!default_clean_text.contains("[pass]"));

            let mut verbose_clean_out = Vec::new();
            run_validate_with_env_and_telemetry(
                &ValidateArgs {
                    verbose: true,
                    ..caseload_args
                },
                &mut Cursor::new(&clean_bytes),
                &mut verbose_clean_out,
                Path::new("."),
                &BTreeMap::new(),
                &TelemetrySession::noop(),
            )
            .expect("verbose clean validate exits 0");
            assert!(
                String::from_utf8(verbose_clean_out)
                    .expect("utf8")
                    .contains("    - .canons/cli/clean-rule.md [pass] (0 error(s), 0 warning(s))")
            );
        }

        fn mixed_validation_caseload_bytes() -> Vec<u8> {
            use crate::caseload::{CaseloadDiscovery, ValidatedCanonMetadata, ValidationOutcome};

            let clean_canon = ".canons/cli/clean-rule.md";
            let warn_canon = ".canons/cli/warn-rule.md";
            let err_canon = ".canons/cli/broken-rule.md";

            let mixed_caseload = Caseload::new()
                .with_discovery(CaseloadDiscovery {
                    candidate_canons: vec![
                        err_canon.to_string(),
                        clean_canon.to_string(),
                        warn_canon.to_string(),
                    ],
                    ..CaseloadDiscovery::default()
                })
                .with_validation(CaseloadValidation {
                    has_errors: true,
                    results: BTreeMap::from([
                        (
                            err_canon.to_string(),
                            ValidatedCanonMetadata {
                                result: ValidationOutcome::Fail,
                                warning_count: 0,
                                error_count: 1,
                                diagnostics: vec![CodeAnnotation {
                                    path: err_canon.to_string(),
                                    start_line: 3,
                                    end_line: 3,
                                    start_column: Some(1),
                                    end_column: None,
                                    annotation_level: AnnotationLevel::Failure,
                                    message: "Unknown frontmatter key.".to_string(),
                                    title: Some("frontmatter-schema".to_string()),
                                }],
                            },
                        ),
                        (
                            clean_canon.to_string(),
                            ValidatedCanonMetadata {
                                result: ValidationOutcome::Pass,
                                warning_count: 0,
                                error_count: 0,
                                diagnostics: Vec::new(),
                            },
                        ),
                        (
                            warn_canon.to_string(),
                            ValidatedCanonMetadata {
                                result: ValidationOutcome::Pass,
                                warning_count: 1,
                                error_count: 0,
                                diagnostics: vec![CodeAnnotation {
                                    path: warn_canon.to_string(),
                                    start_line: 8,
                                    end_line: 8,
                                    start_column: Some(5),
                                    end_column: None,
                                    annotation_level: AnnotationLevel::Warning,
                                    message: "Lowercase normative keyword.".to_string(),
                                    title: Some("rfc2119".to_string()),
                                }],
                            },
                        ),
                    ]),
                });
            serde_json::to_vec(&mixed_caseload).expect("serialize mixed")
        }

        #[test]
        fn test_stylish_mixed_corpus_surfaces_warnings_and_errors_while_honoring_quiet() {
            let mixed_bytes = mixed_validation_caseload_bytes();
            let caseload_args = ValidateArgs {
                io: CaseloadIoArgs {
                    caseload: Some("-".to_string()),
                    ..CaseloadIoArgs::default()
                },
                ..ValidateArgs::default()
            };

            let mut mixed_default_out = Vec::new();
            let err = run_validate_with_env_and_telemetry(
                &caseload_args,
                &mut Cursor::new(&mixed_bytes),
                &mut mixed_default_out,
                Path::new("."),
                &BTreeMap::new(),
                &TelemetrySession::noop(),
            )
            .expect_err("mixed validate with error must exit 1");
            assert_eq!(err.exit_code(), 1);
            let mixed_default_text = String::from_utf8(mixed_default_out).expect("utf8");
            assert!(mixed_default_text.contains(".canons/cli/broken-rule.md"));
            assert!(mixed_default_text.contains(".canons/cli/warn-rule.md"));
            assert!(!mixed_default_text.contains(".canons/cli/clean-rule.md"));

            let mut quiet_out = Vec::new();
            let _ = run_validate_with_env_and_telemetry(
                &ValidateArgs {
                    quiet: true,
                    verbose: true,
                    ..caseload_args
                },
                &mut Cursor::new(&mixed_bytes),
                &mut quiet_out,
                Path::new("."),
                &BTreeMap::new(),
                &TelemetrySession::noop(),
            )
            .expect_err("quiet validate with error must exit 1");
            let quiet_text = String::from_utf8(quiet_out).expect("utf8");
            assert!(quiet_text.contains(".canons/cli/broken-rule.md"));
            assert!(!quiet_text.contains(".canons/cli/warn-rule.md"));
            assert!(!quiet_text.contains(".canons/cli/clean-rule.md"));
        }

        #[test]
        fn test_validate_standalone_inline_canon_and_caseload_resumption() {
            let dir = TempDir::new("inline-canon");
            dir.write_file("src/lib.rs", "pub fn ok() {}\n");

            // 1. Standalone inline canon validation (no targets, no --all-canons)
            let valid_args = ValidateArgs {
                canons: CanonSelectionArgs {
                    canon_name: Some("avoid-eval.canon.md".to_string()),
                    canon_body: Some("Source code MUST avoid dynamic eval().".to_string()),
                    ..CanonSelectionArgs::default()
                },
                ..ValidateArgs::default()
            };
            let mut out = Vec::new();
            let caseload =
                run_validate_in_workspace(&valid_args, &mut Cursor::new(""), &mut out, dir.path())
                    .expect("standalone inline canon should pass validate");
            let disc = caseload.discovery.as_ref().expect("discovery");
            assert_eq!(disc.candidate_canons, vec!["inline:avoid-eval".to_string()]);
            assert_eq!(
                disc.inline_canons
                    .get("inline:avoid-eval")
                    .map(String::as_str),
                Some("Source code MUST avoid dynamic eval().")
            );
            assert!(!caseload.validation.as_ref().expect("validation").has_errors);

            // 2. Inline canon via stdin (`--canon-body-file -`) with negated-modal stem triggers lint warning
            let bad_stem_args = ValidateArgs {
                canons: CanonSelectionArgs {
                    canon_name: Some("code-must-not-use-eval".to_string()),
                    canon_body_file: Some("-".to_string()),
                    ..CanonSelectionArgs::default()
                },
                max_warnings: Some(0),
                ..ValidateArgs::default()
            };
            let mut bad_out = Vec::new();
            let err = run_validate_in_workspace(
                &bad_stem_args,
                &mut Cursor::new("Code MUST avoid eval()."),
                &mut bad_out,
                dir.path(),
            )
            .expect_err("negated modal stem warning should trip --max-warnings 0");
            assert_eq!(err.exit_code(), 1);
            let bad_text = String::from_utf8(bad_out).expect("utf8");
            assert!(bad_text.contains("inline:code-must-not-use-eval"));
            assert!(bad_text.contains("no-negated-file-stems"));

            // 3. `--caseload` resumption rehydrates `inline:avoid-eval` without re-passing CLI flags
            let discovery_only = Caseload::new().with_discovery(disc.clone());
            let encoded = serde_json::to_vec(&discovery_only).expect("serialize discovery");
            let resume_args = ValidateArgs {
                io: CaseloadIoArgs {
                    caseload: Some("-".to_string()),
                    ..CaseloadIoArgs::default()
                },
                ..ValidateArgs::default()
            };
            let mut resume_out = Vec::new();
            let resumed = run_validate_in_workspace(
                &resume_args,
                &mut Cursor::new(encoded),
                &mut resume_out,
                dir.path(),
            )
            .expect("resumed validate from caseload with inline_canons should succeed");
            let val = resumed.validation.expect("validation");
            assert!(!val.has_errors);
            assert!(val.results.contains_key("inline:avoid-eval"));
        }
    }
}
