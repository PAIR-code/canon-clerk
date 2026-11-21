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
                    ..CanonSelectionArgs::default()
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
        use std::fs;
        use std::path::PathBuf;
        use std::time::{SystemTime, UNIX_EPOCH};

        struct TempDiscoverWorkspace {
            path: PathBuf,
        }

        impl TempDiscoverWorkspace {
            fn new(label: &str) -> Self {
                let unique = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos();
                let path = std::env::temp_dir().join(format!(
                    "canon-clerk-discover-ut-{label}-{unique}-{}",
                    std::process::id()
                ));
                let _ = fs::remove_dir_all(&path);
                fs::create_dir_all(path.join(".canons")).expect("create .canons");
                fs::create_dir_all(path.join("src")).expect("create src");
                fs::write(
                    path.join(".canons/rust-modules-must-compile.canon.md"),
                    "---\ntriggers:\n  - \"src/**/*.rs\"\n---\nRust modules MUST compile cleanly.\n",
                )
                .expect("write rust fixture canon");
                fs::write(
                    path.join(".canons/readme-must-be-concise.canon.md"),
                    "---\ntriggers:\n  - \"README.md\"\n---\nREADME files MUST be concise.\n",
                )
                .expect("write readme fixture canon");
                fs::write(
                    path.join(".canons/README.md"),
                    "# Non-Canon Documentation\n\nThis file must be ignored by discover.\n",
                )
                .expect("write non-canon .canons/README.md");
                fs::write(path.join("src/lib.rs"), "pub fn fixture() {}\n")
                    .expect("write src/lib.rs");
                Self { path }
            }
        }

        impl Drop for TempDiscoverWorkspace {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.path);
            }
        }

        #[test]
        fn test_all_canons_discovers_workspace_and_builtin_corpus() {
            let ws = TempDiscoverWorkspace::new("all-canons");
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
            let caseload = run_discover_in_workspace(&args, &mut stdin, &mut stdout, &ws.path)
                .expect("discover --all-canons");
            let disc = caseload.discovery.expect("discovery populated");
            assert_eq!(disc.mode.as_str(), "all-canons");
            assert!(
                disc.candidate_canons
                    .contains(&".canons/rust-modules-must-compile.canon.md".to_string())
            );
            assert!(
                !disc
                    .candidate_canons
                    .contains(&".canons/README.md".to_string()),
                "non-canon .canons/README.md must be excluded from discovery"
            );
            assert!(
                disc.candidate_canons
                    .iter()
                    .any(|c| c.starts_with("builtin:"))
            );
        }

        #[test]
        fn test_predicate_quiet_mode_exits_1_on_zero_matches() {
            let ws = TempDiscoverWorkspace::new("quiet-miss");
            let args = DiscoverArgs {
                filing: FilingSourceArgs {
                    targets: vec!["src/lib.rs".to_string()],
                    ..FilingSourceArgs::default()
                },
                canons: CanonSelectionArgs {
                    canon: vec![".canons/readme-must-be-concise.canon.md".to_string()],
                    ..CanonSelectionArgs::default()
                },
                quiet: true,
                ..DiscoverArgs::default()
            };
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let err = run_discover_in_workspace(&args, &mut stdin, &mut stdout, &ws.path)
                .expect_err("zero matches in -q must fail");
            assert_eq!(err.exit_code(), 1);
            assert!(err.is_silent());
            assert!(stdout.is_empty());
        }

        #[test]
        fn test_compact_format_emits_tab_separated_lines() {
            let ws = TempDiscoverWorkspace::new("compact");
            let args = DiscoverArgs {
                filing: FilingSourceArgs {
                    targets: vec!["src/lib.rs".to_string()],
                    ..FilingSourceArgs::default()
                },
                canons: CanonSelectionArgs {
                    no_builtins: true,
                    ..CanonSelectionArgs::default()
                },
                format: DiscoverFormat::Compact,
                ..DiscoverArgs::default()
            };
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let caseload = run_discover_in_workspace(&args, &mut stdin, &mut stdout, &ws.path)
                .expect("discover compact");
            let disc = caseload.discovery.expect("discovery populated");
            assert_eq!(
                disc.candidate_canons,
                vec![".canons/rust-modules-must-compile.canon.md".to_string()]
            );
            let rendered = String::from_utf8(stdout).expect("utf8");
            assert!(rendered.contains('\t'));
        }

        #[test]
        fn test_policy_config_on_disk_exit_code_2_on_invalid_and_nested_warning_ignored() {
            let ws = TempDiscoverWorkspace::new("policy-config-cli");
            // Create a valid root config and an ignored nested config inside `.canons/security/`
            std::fs::write(
                ws.path.join(".canons/config.yaml"),
                "canon-policy-chain:\n  - deny: \"builtin:**\"\n  - deny: \"readme-must-be-concise\"\n",
            )
            .expect("write root config");
            std::fs::create_dir_all(ws.path.join(".canons/security")).expect("mkdir security");
            std::fs::write(
                ws.path.join(".canons/security/config.yaml"),
                "this is invalid yaml: [unclosed",
            )
            .expect("write nested config");

            let args = DiscoverArgs {
                canons: CanonSelectionArgs {
                    all_canons: true,
                    ..CanonSelectionArgs::default()
                },
                format: DiscoverFormat::Json,
                ..DiscoverArgs::default()
            };
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let caseload = run_discover_in_workspace(&args, &mut stdin, &mut stdout, &ws.path)
                .expect("valid root config with ignored nested config must succeed");
            let disc = caseload.discovery.expect("discovery");
            assert_eq!(disc.config_files, vec![".canons/config.yaml".to_string()]);
            assert_eq!(
                disc.candidate_canons,
                vec![".canons/rust-modules-must-compile.canon.md".to_string()]
            );

            // Now write a forbidden credential key in root `.canons/config.yaml` -> exit code 2
            std::fs::write(
                ws.path.join(".canons/config.yaml"),
                "providers:\n  gemini:\n    apiKey: secret\n",
            )
            .expect("write invalid root config");
            let mut stdin2 = Cursor::new(b"");
            let mut stdout2 = Vec::new();
            let err = run_discover_in_workspace(&args, &mut stdin2, &mut stdout2, &ws.path)
                .expect_err("credential key in .canons/config.yaml must fail fast");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("config.json"));
        }

        #[test]
        fn test_validate_all_canons_respects_unconditional_policy_config() {
            let ws = TempDiscoverWorkspace::new("policy-validate-all-canons");
            std::fs::write(
                ws.path.join(".canons/config.yaml"),
                "canon-policy-chain:\n  - deny: \"builtin:**\"\n  - deny: \"readme-must-be-concise\"\n",
            )
            .expect("write config");

            let args = crate::cli::ValidateArgs {
                canons: CanonSelectionArgs {
                    all_canons: true,
                    ..CanonSelectionArgs::default()
                },
                ..crate::cli::ValidateArgs::default()
            };
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let caseload =
                crate::cli::run_validate_in_workspace(&args, &mut stdin, &mut stdout, &ws.path)
                    .expect("validate --all-canons");
            let disc = caseload.discovery.expect("discovery");
            assert_eq!(disc.config_files, vec![".canons/config.yaml".to_string()]);
            assert_eq!(
                disc.candidate_canons,
                vec![".canons/rust-modules-must-compile.canon.md".to_string()]
            );
            let val = caseload.validation.expect("validation");
            assert_eq!(val.results.len(), 1);
            assert!(
                val.results
                    .contains_key(".canons/rust-modules-must-compile.canon.md")
            );
        }
    }
}
