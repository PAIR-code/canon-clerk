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

use clap::{CommandFactory, Parser};
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Canon Clerk CLI command-line argument parser.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[command(
    name = "canon-clerk",
    author = "PAIR-code",
    version = env!("CARGO_PKG_VERSION"),
    about = "Automated review gate enforcing declarative engineering rule packs",
    long_about = "Canon Clerk is an automated review gate that enforces declarative engineering rule packs (canons) and repository-specific invariants.",
    propagate_version = true
)]
pub struct Cli {
    /// Primary positional operational targets (file paths, directories, or globs)
    #[arg(
        value_name = "TARGETS",
        help = "Target file paths, directories, or globs to evaluate"
    )]
    pub targets: Vec<PathBuf>,

    /// Diagnostic verbose flag with positive polarity
    #[arg(
        short = 'v',
        long = "verbose",
        help_heading = "Logging & Diagnostics",
        help = "Enable verbose diagnostic output"
    )]
    pub verbose: bool,
}

impl Cli {
    /// Renders the standardized help screen as a String.
    pub fn render_help() -> String {
        let mut cmd = Self::command();
        cmd.render_help().to_string()
    }
}

/// Initializes structured logging directed exclusively to stderr.
pub fn init_logging(verbose: bool) {
    let env_filter = if verbose {
        tracing_subscriber::EnvFilter::new("debug")
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .compact(),
        )
        .try_init()
        .ok();
}

/// Executes the baseline CLI application logic.
pub fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    init_logging(cli.verbose);

    tracing::debug!("Initialized Canon Clerk CLI runtime");

    if cli.targets.is_empty() {
        Cli::command().print_help()?;
        println!();
        return Ok(());
    }

    tracing::info!("Received {} target operand(s)", cli.targets.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse_empty() {
        let cli = Cli::try_parse_from(["canon-clerk"]).expect("parsing empty args should succeed");
        assert!(cli.targets.is_empty());
        assert!(!cli.verbose);
    }

    #[test]
    fn test_cli_parse_targets_positional() {
        let cli = Cli::try_parse_from(["canon-clerk", "src/lib.rs", "SPEC.md"])
            .expect("parsing positionals should succeed");
        assert_eq!(cli.targets.len(), 2);
        assert_eq!(cli.targets[0], PathBuf::from("src/lib.rs"));
        assert_eq!(cli.targets[1], PathBuf::from("SPEC.md"));
        assert!(!cli.verbose);
    }

    #[test]
    fn test_cli_parse_verbose_short_and_long() {
        let cli_short = Cli::try_parse_from(["canon-clerk", "-v"]).expect("-v should succeed");
        assert!(cli_short.verbose);

        let cli_long =
            Cli::try_parse_from(["canon-clerk", "--verbose"]).expect("--verbose should succeed");
        assert!(cli_long.verbose);
    }

    #[test]
    fn test_cli_parse_invalid_flag_yields_usage_error() {
        let err = Cli::try_parse_from(["canon-clerk", "--unrecognized-option"])
            .expect_err("unrecognized flag must fail");
        assert!(err.use_stderr());
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn test_cli_help_screen_groups_options_by_concern() {
        let help = Cli::render_help();
        assert!(
            help.contains("Logging & Diagnostics:"),
            "help screen must contain 'Logging & Diagnostics' group heading"
        );
        assert!(
            help.contains("-v, --verbose"),
            "help screen must document verbose flag"
        );
        assert!(
            help.contains("TARGETS"),
            "help screen must document positional TARGETS"
        );
    }

    #[test]
    fn test_cli_version_matches_cargo_pkg() {
        let cmd = Cli::command();
        assert_eq!(cmd.get_version(), Some(env!("CARGO_PKG_VERSION")));
    }
}
