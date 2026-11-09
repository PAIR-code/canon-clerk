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

//! CLI driving adapter for the `canon-clerk discover` subcommand (`docs/architecture/nodes/discover.md` §5).

use crate::caseload::{Caseload, CaseloadDiscovery};
use crate::cli::CliError;
use crate::cli::intake::{read_operand_source, resolve_target_operands, write_caseload_file};
use crate::discover::{DiscoverError, DiscoverOptions, execute_discover_on_disk};
use crate::intake::{IntakeError, IntakeOptions, execute_intake};
use clap::{Args, ValueEnum};
use std::env;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const DISCOVER_EXAMPLES: &str = "\
Examples:
  # Evaluate discovery on an in-flight unified diff stream:
  git diff origin/main | canon-clerk discover --diff -

  # Enumerate all canons across the repository corpus:
  canon-clerk discover --all-canons

  # Evaluate discovery against an upstream Caseload JSON:
  canon-clerk discover --caseload caseload-1.json --json

  # Predicate mode (-q): exit 0 if candidate canons match, 1 if zero match:
  git diff origin/main | canon-clerk discover -q --diff -";

/// Output presentation format for `canon-clerk discover`.
#[derive(ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DiscoverFormat {
    /// Human-readable discovery summary with pipeline provenance and trigger joins.
    #[default]
    Stylish,
    /// Full cumulative `Caseload` JSON envelope.
    Json,
    /// Delimiter-separated (`<canon>\t<matched_exhibits>`) one-line-per-canon output.
    Compact,
}

/// Command-line arguments for `canon-clerk discover`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
#[command(
    about = "Identify candidate canons and prune un-inspected exhibits for a Caseload filing",
    long_about = "Multi-plane relevance sieve and dual pruning gate of the Caseload DAG (Branch A). Materializes target file exhibits, verifies `exists:` preconditions, enforces monorepo scope containment, matches `triggers:` and `inspect:` planes, and populates `.discovery` on the cumulative Caseload.",
    after_help = DISCOVER_EXAMPLES
)]
pub struct DiscoverArgs {
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

    /// Discover all canons across the repository corpus, bypassing path trigger matching.
    #[arg(
        long = "all-canons",
        help_heading = "Discovery Options",
        help = "Discover all repository canons into candidate_canons, bypassing trigger matching"
    )]
    pub all_canons: bool,

    /// Explicitly restrict candidate canons to a specific file path or glob subset.
    #[arg(
        long = "canon",
        value_name = "PATH|GLOB",
        help_heading = "Discovery Options",
        help = "Explicitly restrict candidate canons to a specific subset"
    )]
    pub canon: Vec<String>,

    /// Predicate mode: suppress stdout and indicate candidate canon match presence via exit code (`0` or `1`).
    #[arg(
        short = 'q',
        long = "quiet",
        help_heading = "Discovery Options",
        help = "Predicate mode: suppress stdout and exit 1 if zero candidate canons matched"
    )]
    pub quiet: bool,

    /// Output format for matched canons and triggering exhibits (`stylish`, `json`, or `compact`).
    #[arg(
        long = "format",
        value_enum,
        default_value_t = DiscoverFormat::Stylish,
        help_heading = "Caseload & Output",
        help = "Format matched canons and triggering files"
    )]
    pub format: DiscoverFormat,

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

/// Executes the `discover` CLI subcommand against the provided `stdin`, `stdout`, and `workspace_root`.
pub fn run_discover_in_workspace(
    args: &DiscoverArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace_root: &Path,
) -> Result<Caseload, CliError> {
    validate_stdin_sources(args)?;

    let incoming_caseload = read_incoming_caseload(args, stdin)?;
    let intake_opts = build_intake_options(args, stdin)?;

    let caseload_after_intake = if intake_opts.has_designated_source() {
        execute_intake(&intake_opts, incoming_caseload).map_err(|err| match err {
            IntakeError::MissingInputSource | IntakeError::MalformedDiff(_) => {
                CliError::usage(err.to_string())
            }
        })?
    } else if let Some(existing) = incoming_caseload {
        existing
    } else if args.all_canons {
        Caseload::new()
    } else {
        return Err(CliError::usage(
            DiscoverError::MissingInputSource.to_string(),
        ));
    };

    let discover_opts = DiscoverOptions {
        all_canons: args.all_canons,
        canon_patterns: args.canon.clone(),
    };

    let caseload = execute_discover_on_disk(&discover_opts, caseload_after_intake, workspace_root)
        .map_err(|err| match err {
            DiscoverError::MissingInputSource
            | DiscoverError::InvalidGlob(_)
            | DiscoverError::PathEscapesWorkspace { .. } => CliError::usage(err.to_string()),
            DiscoverError::Io { .. } => CliError::runtime(err.to_string()),
        })?;

    if let Some(output_path) = &args.output_caseload {
        write_caseload_file(output_path, &caseload)?;
    }

    let discovery = caseload.discovery.clone().unwrap_or_default();

    if args.quiet {
        if discovery.candidate_canons.is_empty() {
            return Err(CliError::silent(1));
        }
        return Ok(caseload);
    }

    render_discover_output(args, &caseload, &discovery, stdout)?;
    Ok(caseload)
}

/// Executes the `discover` CLI subcommand using the current working directory as `workspace_root`.
pub fn run_discover(
    args: &DiscoverArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    let cwd = env::current_dir().map_err(|err| {
        CliError::runtime(format!(
            "Failed to determine current working directory: {err}"
        ))
    })?;
    run_discover_in_workspace(args, stdin, stdout, &cwd)
}

fn read_incoming_caseload(
    args: &DiscoverArgs,
    stdin: &mut dyn Read,
) -> Result<Option<Caseload>, CliError> {
    match &args.caseload {
        Some(source) => {
            let raw = read_operand_source(source, "--caseload", stdin)?;
            let parsed: Caseload = serde_json::from_str(&raw).map_err(|err| {
                CliError::usage(format!(
                    "Failed to parse incoming Caseload JSON from `{source}`: {err}\n  \
                     Hint: Ensure `--caseload` points to a valid Canon Clerk Caseload JSON file."
                ))
            })?;
            Ok(Some(parsed))
        }
        None => Ok(None),
    }
}

fn build_intake_options(
    args: &DiscoverArgs,
    stdin: &mut dyn Read,
) -> Result<IntakeOptions, CliError> {
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

    Ok(IntakeOptions {
        pr_title: args.pr_title.clone(),
        pr_body,
        intent: args.intent.clone(),
        target_paths,
        patch_content,
        all_targets: args.all_targets,
        linked_issues: None,
    })
}

fn validate_stdin_sources(args: &DiscoverArgs) -> Result<(), CliError> {
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

fn render_discover_output(
    args: &DiscoverArgs,
    caseload: &Caseload,
    discovery: &CaseloadDiscovery,
    stdout: &mut dyn Write,
) -> Result<(), CliError> {
    let effective_format = if args.json {
        DiscoverFormat::Json
    } else {
        args.format
    };

    match effective_format {
        DiscoverFormat::Json => {
            let serialized = serde_json::to_string_pretty(caseload).map_err(|err| {
                CliError::runtime(format!("Failed to serialize Caseload JSON: {err}"))
            })?;
            writeln!(stdout, "{serialized}").map_err(|err| {
                CliError::runtime(format!("Failed to write JSON to stdout: {err}"))
            })?;
        }
        DiscoverFormat::Compact => {
            for canon_ref in &discovery.candidate_canons {
                let joined = discovery
                    .triggers_join
                    .get(canon_ref)
                    .map(|items| items.join(","))
                    .unwrap_or_default();
                writeln!(stdout, "{canon_ref}\t{joined}").map_err(|err| {
                    CliError::runtime(format!("Failed to write compact output to stdout: {err}"))
                })?;
            }
        }
        DiscoverFormat::Stylish => {
            render_stylish_summary(discovery, stdout).map_err(|err| {
                CliError::runtime(format!("Failed to write summary to stdout: {err}"))
            })?;
        }
    }

    Ok(())
}

fn render_stylish_summary(
    discovery: &CaseloadDiscovery,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    writeln!(stdout, "Discovery Summary:")?;
    writeln!(stdout, "  Mode             : {}", discovery.mode.as_str())?;
    writeln!(
        stdout,
        "  Target Files     : {} file(s)",
        discovery.target_files.len()
    )?;
    writeln!(
        stdout,
        "  Candidate Canons : {} matched",
        discovery.candidate_canons.len()
    )?;

    for canon_ref in &discovery.candidate_canons {
        let joined = discovery
            .triggers_join
            .get(canon_ref)
            .filter(|items| !items.is_empty())
            .map(|items| format!(" ({})", items.join(", ")))
            .unwrap_or_default();
        writeln!(stdout, "    - {canon_ref}{joined}")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    mod source_and_stdin_guards {
        use super::*;

        #[test]
        fn test_naked_discover_fails_with_exit_code_2() {
            let args = DiscoverArgs::default();
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let err = run_discover(&args, &mut stdin, &mut stdout).expect_err("naked must fail");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("Hint:"));
        }

        #[test]
        fn test_conflicting_stdin_operands_rejected_with_exit_code_2() {
            let args = DiscoverArgs {
                diff: Some("-".to_string()),
                caseload: Some("-".to_string()),
                ..DiscoverArgs::default()
            };
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let err =
                run_discover(&args, &mut stdin, &mut stdout).expect_err("multiple stdin must fail");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("Hint:"));
        }

        #[test]
        fn test_invalid_glob_syntax_exits_with_code_2() {
            let args = DiscoverArgs {
                all_canons: true,
                canon: vec![".canons/[unclosed/*.md".to_string()],
                ..DiscoverArgs::default()
            };
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let err =
                run_discover(&args, &mut stdin, &mut stdout).expect_err("invalid glob must fail");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("Hint:"));
        }
    }

    mod execution_and_formats {
        use super::*;

        #[test]
        fn test_all_canons_discovers_repository_corpus() {
            let args = DiscoverArgs {
                all_canons: true,
                json: true,
                ..DiscoverArgs::default()
            };
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let caseload =
                run_discover(&args, &mut stdin, &mut stdout).expect("discover --all-canons");
            let disc = caseload.discovery.expect("discovery populated");
            assert_eq!(disc.mode.as_str(), "all-canons");
            assert!(!disc.candidate_canons.is_empty());
        }

        #[test]
        fn test_predicate_quiet_mode_exits_1_on_zero_matches() {
            let args = DiscoverArgs {
                targets: vec!["src/lib.rs".to_string()],
                canon: vec![
                    ".canons/readme-authoring/readme-must-lead-with-what-and-why.md".to_string(),
                ],
                quiet: true,
                ..DiscoverArgs::default()
            };
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let err = run_discover(&args, &mut stdin, &mut stdout)
                .expect_err("zero matches in -q must fail");
            assert_eq!(err.exit_code(), 1);
            assert!(err.is_silent());
            assert!(stdout.is_empty());
        }

        #[test]
        fn test_compact_format_emits_tab_separated_lines() {
            let args = DiscoverArgs {
                targets: vec!["src/cli/mod.rs".to_string()],
                format: DiscoverFormat::Compact,
                ..DiscoverArgs::default()
            };
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let caseload = run_discover(&args, &mut stdin, &mut stdout).expect("discover compact");
            let disc = caseload.discovery.expect("discovery populated");
            assert!(!disc.candidate_canons.is_empty());
            let rendered = String::from_utf8(stdout).expect("utf8");
            assert!(rendered.contains('\t'));
        }
    }
}
