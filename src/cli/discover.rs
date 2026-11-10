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

//! CLI driving adapter for the `canon-clerk discover` subcommand.

use crate::caseload::{Caseload, CaseloadDiscovery};
use crate::cli::CliError;
use crate::cli::args::{CanonSelectionArgs, CaseloadIoArgs, FilingSourceArgs};
use crate::cli::io::ensure_single_stdin_source;
use crate::cli::runners::run_branch_a_discover;
use crate::telemetry::TelemetrySession;
use clap::{Args, ValueEnum};
use std::env;
use std::io::{Read, Write};
use std::path::Path;

const DISCOVER_EXAMPLES: &str = "\
Examples:
  # Discover candidate canons matching working tree diff:
  git diff origin/main | canon-clerk discover --diff -

  # Discover candidate canons for explicit target paths:
  canon-clerk discover src/lib.rs 'src/**/*.rs'

  # Discover all canons across the repository corpus:
  canon-clerk discover --all-canons

  # Predicate check (silent; exits 0 if matches exist, 1 if none):
  canon-clerk discover --diff - -q

  # Emit discovery Caseload JSON to standard output:
  canon-clerk discover --all-targets --json";

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
    #[command(flatten)]
    pub filing: FilingSourceArgs,

    #[command(flatten)]
    pub canons: CanonSelectionArgs,

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

    #[command(flatten)]
    pub io: CaseloadIoArgs,
}

/// Executes the `discover` CLI subcommand against `workspace_root` with an explicit [`TelemetrySession`].
pub fn run_discover_with_telemetry(
    args: &DiscoverArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace_root: &Path,
    telemetry: &TelemetrySession<'_>,
) -> Result<Caseload, CliError> {
    ensure_single_stdin_source(&args.filing, args.io.caseload.as_deref())?;

    let incoming_caseload = args.io.read_caseload(stdin)?;
    let caseload = run_branch_a_discover(
        &args.filing,
        &args.canons,
        incoming_caseload,
        workspace_root,
        stdin,
        telemetry,
    )?;
    telemetry.finish_pipeline();

    args.io.write_output_caseload(&caseload)?;

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

/// Executes the `discover` CLI subcommand against the provided `stdin`, `stdout`, and `workspace_root`.
pub fn run_discover_in_workspace(
    args: &DiscoverArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    workspace_root: &Path,
) -> Result<Caseload, CliError> {
    run_discover_with_telemetry(
        args,
        stdin,
        stdout,
        workspace_root,
        &TelemetrySession::noop(),
    )
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

fn render_discover_output(
    args: &DiscoverArgs,
    caseload: &Caseload,
    discovery: &CaseloadDiscovery,
    stdout: &mut dyn Write,
) -> Result<(), CliError> {
    let effective_format = if args.io.json {
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
                filing: FilingSourceArgs {
                    diff: Some("-".to_string()),
                    ..FilingSourceArgs::default()
                },
                io: CaseloadIoArgs {
                    caseload: Some("-".to_string()),
                    ..CaseloadIoArgs::default()
                },
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
                canons: CanonSelectionArgs {
                    all_canons: true,
                    canon: vec![".canons/[unclosed/*.md".to_string()],
                },
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
                canons: CanonSelectionArgs {
                    all_canons: true,
                    ..CanonSelectionArgs::default()
                },
                io: CaseloadIoArgs {
                    json: true,
                    ..CaseloadIoArgs::default()
                },
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
                filing: FilingSourceArgs {
                    targets: vec!["src/lib.rs".to_string()],
                    ..FilingSourceArgs::default()
                },
                canons: CanonSelectionArgs {
                    canon: vec![
                        ".canons/readme-authoring/readme-must-lead-with-what-and-why.md"
                            .to_string(),
                    ],
                    ..CanonSelectionArgs::default()
                },
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
                filing: FilingSourceArgs {
                    targets: vec!["src/cli/mod.rs".to_string()],
                    ..FilingSourceArgs::default()
                },
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
