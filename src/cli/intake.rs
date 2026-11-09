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

//! CLI driving adapter for the `canon-clerk intake` subcommand.

use crate::caseload::{Caseload, CaseloadIntake};
use crate::cli::CliError;
use crate::intake::{IntakeError, IntakeOptions, execute_intake};
use clap::Args;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const INTAKE_EXAMPLES: &str = "\
Examples:
  # Ingest direct target file paths or globs:
  canon-clerk intake src/lib.rs 'src/**/*.rs'

  # Ingest a unified diff patch from standard input:
  git diff origin/main | canon-clerk intake --diff -

  # Ingest newline-delimited target paths from standard input:
  git diff origin/main --name-only | canon-clerk intake -

  # Ingest prospective design intent and target scope:
  canon-clerk intake --intent \"Implement round-robin auth rotation\" src/auth

  # Ingest all repository files and archive the cumulative Caseload to disk:
  canon-clerk intake --all-targets --output-caseload caseload.json --json";

/// Command-line arguments for `canon-clerk intake`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
#[command(
    about = "Ingest diffs, target paths/globs, design intent, or PR metadata into a Caseload filing",
    long_about = "Universal front door of the Caseload DAG (Branch A). Catalogs tendered exhibits (unified diffs, PR metadata, design intent) and target discovery directives into the `.intake` namespace on the cumulative Caseload.",
    after_help = INTAKE_EXAMPLES
)]
pub struct IntakeArgs {
    /// Target file paths, directories, globs, or `-` to read newline-delimited paths from stdin.
    #[arg(
        value_name = "TARGETS",
        help = "Target file paths, directories, globs, or '-' for newline-delimited stdin paths"
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
        help = "Ingest prospective architectural design intent or plan description"
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

/// Executes the `intake` CLI subcommand against the provided `stdin` and `stdout` streams.
pub fn run_intake(
    args: &IntakeArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    validate_stdin_sources(args)?;

    let incoming_caseload = match &args.caseload {
        Some(source) => {
            let raw = read_operand_source(source, "--caseload", stdin)?;
            let parsed: Caseload = serde_json::from_str(&raw).map_err(|err| {
                CliError::usage(format!(
                    "Failed to parse incoming Caseload JSON from `{source}`: {err}\n  \
                     Hint: Ensure `--caseload` points to a valid Canon Clerk Caseload JSON file."
                ))
            })?;
            Some(parsed)
        }
        None => None,
    };

    let patch_content = match &args.diff {
        Some(source) => Some(read_operand_source(source, "--diff", stdin)?),
        None => None,
    };

    let pr_body = if let Some(body) = &args.pr_body {
        Some(body.clone())
    } else if let Some(path) = &args.pr_body_file {
        Some(read_operand_source(path, "--pr-body-file", stdin)?)
    } else {
        None
    };

    let target_paths = resolve_target_operands(&args.targets, stdin)?;

    let options = IntakeOptions {
        pr_title: args.pr_title.clone(),
        pr_body,
        intent: args.intent.clone(),
        target_paths,
        patch_content,
        all_targets: args.all_targets,
        linked_issues: None,
    };

    let caseload = execute_intake(&options, incoming_caseload).map_err(|err| match err {
        IntakeError::MissingInputSource | IntakeError::MalformedDiff(_) => {
            CliError::usage(err.to_string())
        }
    })?;

    if let Some(output_path) = &args.output_caseload {
        write_caseload_file(output_path, &caseload)?;
    }

    if args.json {
        let serialized = serde_json::to_string_pretty(&caseload).map_err(|err| {
            CliError::runtime(format!("Failed to serialize Caseload JSON: {err}"))
        })?;
        writeln!(stdout, "{serialized}")
            .map_err(|err| CliError::runtime(format!("Failed to write JSON to stdout: {err}")))?;
    } else {
        let intake = caseload.intake.clone().unwrap_or_default();
        render_human_summary(&intake, stdout).map_err(|err| {
            CliError::runtime(format!("Failed to write summary to stdout: {err}"))
        })?;
    }

    Ok(caseload)
}

/// Enforces the single-stdin guard: at most one operand may consume standard input (`-`).
fn validate_stdin_sources(args: &IntakeArgs) -> Result<(), CliError> {
    let mut stdin_consumers = Vec::new();

    let positional_stdin_count = args.targets.iter().filter(|t| t.as_str() == "-").count();
    if positional_stdin_count > 1 {
        return Err(CliError::usage(
            "Standard input (`-`) was specified multiple times in positional `TARGETS`.\n  \
             Hint: Pass `-` at most once to read newline-delimited target paths from stdin."
                .to_string(),
        ));
    }
    if positional_stdin_count == 1 {
        stdin_consumers.push("positional `TARGETS` (`-`)");
    }
    if args.diff.as_deref() == Some("-") {
        stdin_consumers.push("`--diff -`");
    }
    if args.pr_body_file.as_deref() == Some("-") {
        stdin_consumers.push("`--pr-body-file -`");
    }
    if args.caseload.as_deref() == Some("-") {
        stdin_consumers.push("`--caseload -`");
    }

    if stdin_consumers.len() > 1 {
        return Err(CliError::usage(format!(
            "Conflicting standard input (`-`) sources specified: {}.\n  \
             Hint: Standard input can only be consumed by a single source per invocation; pass file paths for the remaining options.",
            stdin_consumers.join(", ")
        )));
    }

    Ok(())
}

pub(crate) fn read_operand_source(
    source: &str,
    flag_name: &str,
    stdin: &mut dyn Read,
) -> Result<String, CliError> {
    if source == "-" {
        let mut buf = String::new();
        stdin.read_to_string(&mut buf).map_err(|err| {
            CliError::usage(format!(
                "Failed to read standard input for `{flag_name} -`: {err}\n  \
                 Hint: Pipe valid UTF-8 text into standard input or provide a file path."
            ))
        })?;
        Ok(buf)
    } else {
        fs::read_to_string(source).map_err(|err| {
            CliError::usage(format!(
                "Failed to read file `{source}` for `{flag_name}`: {err}\n  \
                 Hint: Verify that `{source}` exists and is readable, or pass `-` to read from standard input."
            ))
        })
    }
}

pub(crate) fn resolve_target_operands(
    targets: &[String],
    stdin: &mut dyn Read,
) -> Result<Option<Vec<String>>, CliError> {
    if targets.is_empty() {
        return Ok(None);
    }

    let mut resolved = Vec::new();
    for target in targets {
        if target == "-" {
            let raw = read_operand_source("-", "TARGETS", stdin)?;
            for line in raw.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    resolved.push(trimmed.to_string());
                }
            }
        } else {
            resolved.push(target.clone());
        }
    }

    Ok(Some(resolved))
}

pub(crate) fn write_caseload_file(path: &Path, caseload: &Caseload) -> Result<(), CliError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|err| {
            CliError::usage(format!(
                "Failed to create parent directory `{}` for `--output-caseload`: {err}\n  \
                 Hint: Check directory write permissions for `{}`.",
                parent.display(),
                path.display()
            ))
        })?;
    }

    let json = serde_json::to_string_pretty(caseload)
        .map_err(|err| CliError::runtime(format!("Failed to serialize Caseload JSON: {err}")))?;
    fs::write(path, format!("{json}\n")).map_err(|err| {
        CliError::usage(format!(
            "Failed to write Caseload JSON to `{}`: {err}\n  \
             Hint: Verify that the output path is writable.",
            path.display()
        ))
    })
}

fn render_human_summary(intake: &CaseloadIntake, stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(stdout, "Intake Filing Summary:")?;
    writeln!(stdout, "  Scope           : {}", intake.scope.as_str())?;

    if let Some(title) = &intake.pr_title {
        writeln!(stdout, "  PR Title        : {title}")?;
    }
    if let Some(body) = &intake.pr_body {
        writeln!(stdout, "  PR Body         : {} byte(s)", body.len())?;
    }
    if let Some(intent) = &intake.intent {
        writeln!(stdout, "  Design Intent   : {intent}")?;
    }

    writeln!(
        stdout,
        "  Target Paths    : {} directive(s)",
        intake.target_paths.len()
    )?;
    for path in &intake.target_paths {
        writeln!(stdout, "    - {path}")?;
    }

    let total_added: usize = intake.diffs.values().map(|a| a.lines_added).sum();
    let total_deleted: usize = intake.diffs.values().map(|a| a.lines_deleted).sum();
    writeln!(
        stdout,
        "  Diff Exhibits   : {} file(s) (+{} / -{} lines)",
        intake.diffs.len(),
        total_added,
        total_deleted
    )?;
    for artifact in intake.diffs.values() {
        if let Some(prev) = &artifact.previous_path {
            writeln!(
                stdout,
                "    - [{}] {} (from {}, +{} / -{})",
                artifact.status.as_str(),
                artifact.path,
                prev,
                artifact.lines_added,
                artifact.lines_deleted
            )?;
        } else {
            writeln!(
                stdout,
                "    - [{}] {} (+{} / -{})",
                artifact.status.as_str(),
                artifact.path,
                artifact.lines_added,
                artifact.lines_deleted
            )?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Commands, run};
    use clap::{CommandFactory, Parser};
    use std::io::Cursor;

    mod help_and_flag_parsing {
        use super::*;

        #[test]
        fn test_intake_help_includes_examples_and_concern_groups() {
            let mut root_cmd = Cli::command();
            root_cmd.build();
            let intake_cmd = root_cmd
                .find_subcommand_mut("intake")
                .expect("intake subcommand registered");
            let help = intake_cmd.render_help().to_string();

            assert!(help.contains("Filing Sources:"));
            assert!(help.contains("Pull Request Metadata:"));
            assert!(help.contains("Caseload & Output:"));
            assert!(help.contains("Logging & Diagnostics:"));
            assert!(help.contains("Examples:"));
            assert!(help.contains("canon-clerk intake --diff -"));
        }

        #[test]
        fn test_pr_body_and_pr_body_file_conflict_exits_2() {
            let err = Cli::try_parse_from([
                "canon-clerk",
                "intake",
                "--pr-body",
                "inline text",
                "--pr-body-file",
                "body.md",
                "src/lib.rs",
            ])
            .expect_err("conflicting pr-body flags must fail");
            assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
        }
    }

    mod source_and_stdin_guards {
        use super::*;

        #[test]
        fn test_naked_invocation_exits_code_2_with_remediation() {
            let cli =
                Cli::try_parse_from(["canon-clerk", "intake"]).expect("parse naked intake args");
            let err = run(cli).expect_err("naked intake invocation must fail");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("No filing input source provided"));
            assert!(err.to_string().contains("Hint:"));
        }

        #[test]
        fn test_multiple_stdin_operands_rejected_with_exit_code_2() {
            let args = IntakeArgs {
                targets: vec!["-".to_string()],
                diff: Some("-".to_string()),
                ..IntakeArgs::default()
            };
            let mut stdin = Cursor::new("src/lib.rs\n");
            let mut stdout = Vec::new();

            let err = run_intake(&args, &mut stdin, &mut stdout)
                .expect_err("conflicting stdin sources must fail");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("Conflicting standard input"));
            assert!(err.to_string().contains("Hint:"));
        }

        #[test]
        fn test_duplicate_positional_stdin_rejected_with_exit_code_2() {
            let args = IntakeArgs {
                targets: vec!["-".to_string(), "-".to_string()],
                ..IntakeArgs::default()
            };
            let mut stdin = Cursor::new("src/lib.rs\n");
            let mut stdout = Vec::new();

            let err = run_intake(&args, &mut stdin, &mut stdout)
                .expect_err("duplicate positional '-' must fail");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("specified multiple times"));
        }
    }

    mod stream_and_file_execution {
        use super::*;

        #[test]
        fn test_empty_diff_stream_succeeds_with_empty_filing() {
            let cli = Cli::try_parse_from(["canon-clerk", "intake", "--diff", "-", "--json"])
                .expect("parse intake --diff - --json");
            let Some(Commands::Intake(args)) = cli.command else {
                panic!("expected intake command");
            };

            let mut stdin = Cursor::new(Vec::<u8>::new());
            let mut stdout = Vec::new();

            let caseload =
                run_intake(&args, &mut stdin, &mut stdout).expect("empty diff stream must succeed");
            let intake = caseload.intake.expect("intake present");
            assert!(intake.diffs.is_empty());
            assert!(intake.target_paths.is_empty());

            let parsed_stdout: Caseload =
                serde_json::from_slice(&stdout).expect("valid Caseload JSON on stdout");
            assert_eq!(parsed_stdout.intake.unwrap().diffs.len(), 0);
        }

        #[test]
        fn test_stdin_target_paths_and_human_summary() {
            let args = IntakeArgs {
                targets: vec!["-".to_string()],
                pr_title: Some("feat(intake): add stream support".to_string()),
                intent: Some("Catalog piped paths".to_string()),
                ..IntakeArgs::default()
            };
            let mut stdin = Cursor::new("src/main.rs\n./src/lib.rs\n\n");
            let mut stdout = Vec::new();

            let caseload =
                run_intake(&args, &mut stdin, &mut stdout).expect("stdin target paths succeed");
            let intake = caseload.intake.expect("intake present");
            assert_eq!(intake.target_paths, vec!["src/lib.rs", "src/main.rs"]);

            let rendered = String::from_utf8(stdout).expect("utf8 stdout");
            assert!(rendered.contains("Intake Filing Summary:"));
            assert!(rendered.contains("PR Title        : feat(intake): add stream support"));
            assert!(rendered.contains("Design Intent   : Catalog piped paths"));
            assert!(rendered.contains("- src/lib.rs"));
            assert!(rendered.contains("- src/main.rs"));
        }

        #[test]
        fn test_output_caseload_writes_file_independently_of_stdout() {
            let temp_dir = std::env::temp_dir()
                .join(format!("canon-clerk-intake-test-{}", std::process::id()));
            let _ = fs::remove_dir_all(&temp_dir);
            fs::create_dir_all(&temp_dir).expect("create temp dir");

            let patch_path = temp_dir.join("change.patch");
            let output_caseload_path = temp_dir.join("out").join("caseload.json");
            let upstream_caseload_path = temp_dir.join("upstream.json");

            let patch = "\
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1 +1,2 @@
 pub mod cli;
+pub mod intake;
";
            fs::write(&patch_path, patch).expect("write patch file");
            fs::write(
                &upstream_caseload_path,
                r#"{"version":"1.0","config":{"screener_model":"gemini-flash-lite-latest"}}"#,
            )
            .expect("write upstream caseload");

            let args = IntakeArgs {
                diff: Some(patch_path.to_string_lossy().into_owned()),
                caseload: Some(upstream_caseload_path.to_string_lossy().into_owned()),
                output_caseload: Some(output_caseload_path.clone()),
                json: false,
                ..IntakeArgs::default()
            };

            let mut stdin = Cursor::new(Vec::<u8>::new());
            let mut stdout = Vec::new();
            run_intake(&args, &mut stdin, &mut stdout).expect("run_intake succeeds");

            // Default stdout receives human summary while --output-caseload writes JSON
            let stdout_str = String::from_utf8(stdout).expect("utf8");
            assert!(stdout_str.contains("Intake Filing Summary:"));

            let disk_json =
                fs::read_to_string(&output_caseload_path).expect("read output caseload file");
            let saved: Caseload =
                serde_json::from_str(&disk_json).expect("parse saved caseload json");
            assert!(saved.config.is_some());
            assert_eq!(saved.intake.unwrap().diffs["src/lib.rs"].lines_added, 1);

            let _ = fs::remove_dir_all(&temp_dir);
        }
    }
}
