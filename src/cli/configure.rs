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

//! CLI driving adapter for the `canon-clerk configure` subcommand (`docs/architecture/nodes/configure.md` §5).

use crate::caseload::{Caseload, CaseloadConfig};
use crate::cli::CliError;
use crate::cli::discover::read_caseload_operand;
use crate::cli::intake::write_caseload_file;
use crate::configure::{
    ConfigureOptions, ENV_GEMINI_API_KEY, execute_configure, load_host_config_file,
    resolve_default_config_path,
};
use clap::{Args, ValueEnum};
use std::collections::BTreeMap;
use std::env;
use std::io::{Read, Write};
use std::path::PathBuf;

const CONFIGURE_EXAMPLES: &str = "\
Examples:
  # Normalize and print active configuration as JSON:
  canon-clerk configure --json

  # Override model tiers and reasoning budget from CLI flags:
  canon-clerk configure --screener-model flash-lite --auditor-model pro --reasoning-budget 4096

  # Enrich an upstream Caseload JSON with normalized provider configuration:
  canon-clerk configure --caseload caseload-3.json --json";

/// Output presentation format for `canon-clerk configure`.
#[derive(ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConfigureFormat {
    /// Human-readable configuration summary with redacted credential status.
    #[default]
    Stylish,
    /// Full cumulative `Caseload` JSON envelope.
    Json,
    /// Tab-separated (`<key>\t<value>`) one-line-per-setting output.
    Compact,
}

/// Command-line arguments for `canon-clerk configure`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
#[command(
    about = "Resolve and normalize provider credentials, model specifiers, and reasoning budgets",
    long_about = "Operational Environment Normalizer of the Caseload DAG (Branch B). Verifies runtime provider credentials (GEMINI_API_KEY), normalizes screener and auditor model specifiers into canonical provider:model identifiers, and populates `.config` on the cumulative Caseload without persisting secret material.",
    after_help = CONFIGURE_EXAMPLES
)]
pub struct ConfigureArgs {
    /// Custom screener model specifier or shorthand alias (`flash-lite`, `flash`, `pro`; env: `CANON_CLERK_SCREENER_MODEL`).
    #[arg(
        long = "screener-model",
        value_name = "MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Screener model specifier or alias (fallback env: CANON_CLERK_SCREENER_MODEL)"
    )]
    pub screener_model: Option<String>,

    /// Custom auditor model specifier or shorthand alias (`pro`, `flash`, `flash-lite`; env: `CANON_CLERK_AUDITOR_MODEL`).
    #[arg(
        long = "auditor-model",
        value_name = "MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Auditor model specifier or alias (fallback env: CANON_CLERK_AUDITOR_MODEL)"
    )]
    pub auditor_model: Option<String>,

    /// Maximum reasoning budget in tokens for extended thinking during `audit` trials (env: `CANON_CLERK_REASONING_BUDGET`).
    #[arg(
        long = "reasoning-budget",
        value_name = "TOKENS",
        help_heading = "Model & Reasoning Options",
        help = "Maximum reasoning budget tokens (fallback env: CANON_CLERK_REASONING_BUDGET)"
    )]
    pub reasoning_budget: Option<u32>,

    /// Google Gemini API key override (fallback env: `GEMINI_API_KEY`; never persisted to Caseload JSON).
    #[arg(
        long = "gemini-api-key",
        value_name = "KEY",
        help_heading = "Provider Credentials",
        help = "Google Gemini API key (fallback env: GEMINI_API_KEY)"
    )]
    pub gemini_api_key: Option<String>,

    /// Output format (`stylish`, `json`, or `compact`).
    #[arg(
        long = "format",
        value_enum,
        default_value_t = ConfigureFormat::Stylish,
        help_heading = "Caseload & Output",
        help = "Format resolved configuration output"
    )]
    pub format: ConfigureFormat,

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

/// Executes the `configure` CLI subcommand against an explicit environment variable map.
pub fn run_configure_with_env(
    args: &ConfigureArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    env: &BTreeMap<String, String>,
) -> Result<Caseload, CliError> {
    let incoming_caseload = read_caseload_operand(args.caseload.as_deref(), stdin)?;

    let cwd = env::current_dir().ok();
    let stored_config = match resolve_default_config_path(env) {
        Some(cfg_path) => load_host_config_file(&cfg_path, cwd.as_deref())
            .map_err(|err| CliError::usage(err.to_string()))?,
        None => None,
    };

    let options = ConfigureOptions {
        screener_model: args.screener_model.clone(),
        auditor_model: args.auditor_model.clone(),
        reasoning_budget: args.reasoning_budget,
        gemini_api_key: args.gemini_api_key.clone(),
        env: env.clone(),
        stored_config,
    };

    let caseload = execute_configure(&options, incoming_caseload)
        .map_err(|err| CliError::usage(err.to_string()))?;

    if let Some(output_path) = &args.output_caseload {
        write_caseload_file(output_path, &caseload)?;
    }

    let config = caseload.config.clone().unwrap_or_default();
    render_configure_output(args, &caseload, &config, stdout)?;

    Ok(caseload)
}

/// Executes the `configure` CLI subcommand using the process environment.
pub fn run_configure(
    args: &ConfigureArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    let env: BTreeMap<String, String> = env::vars().collect();
    run_configure_with_env(args, stdin, stdout, &env)
}

fn render_configure_output(
    args: &ConfigureArgs,
    caseload: &Caseload,
    config: &CaseloadConfig,
    stdout: &mut dyn Write,
) -> Result<(), CliError> {
    let effective_format = if args.json {
        ConfigureFormat::Json
    } else {
        args.format
    };

    match effective_format {
        ConfigureFormat::Json => {
            let serialized = serde_json::to_string_pretty(caseload).map_err(|err| {
                CliError::runtime(format!("Failed to serialize Caseload JSON: {err}"))
            })?;
            writeln!(stdout, "{serialized}")
                .map_err(|err| CliError::runtime(format!("Failed to write to stdout: {err}")))?;
        }
        ConfigureFormat::Compact => {
            render_compact_output(config, stdout)
                .map_err(|err| CliError::runtime(format!("Failed to write to stdout: {err}")))?;
        }
        ConfigureFormat::Stylish => {
            render_stylish_output(config, stdout)
                .map_err(|err| CliError::runtime(format!("Failed to write to stdout: {err}")))?;
        }
    }

    Ok(())
}

fn render_compact_output(config: &CaseloadConfig, stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(stdout, "screener_model\t{}", config.screener_model)?;
    writeln!(stdout, "auditor_model\t{}", config.auditor_model)?;
    if let Some(budget) = config.reasoning_budget {
        writeln!(stdout, "reasoning_budget\t{budget}")?;
    }
    writeln!(stdout, "credentials\t{ENV_GEMINI_API_KEY} (verified)")?;
    Ok(())
}

fn render_stylish_output(config: &CaseloadConfig, stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(stdout, "Configuration Summary (READY):")?;
    writeln!(stdout, "  Screener Model   : {}", config.screener_model)?;
    writeln!(stdout, "  Auditor Model    : {}", config.auditor_model)?;
    match config.reasoning_budget {
        Some(budget) => writeln!(stdout, "  Reasoning Budget : {budget} tokens")?,
        None => writeln!(stdout, "  Reasoning Budget : default (unbounded)")?,
    }
    writeln!(
        stdout,
        "  Credentials      : {ENV_GEMINI_API_KEY} (verified)"
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{DEFAULT_AUDITOR_MODEL, DEFAULT_SCREENER_MODEL};
    use crate::cli::{Cli, Commands};
    use clap::Parser;
    use std::io::Cursor;

    fn test_env() -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        env.insert(
            ENV_GEMINI_API_KEY.to_string(),
            "synthetic-test-api-key-value".to_string(),
        );
        env
    }

    mod cli_argument_parsing_and_help {
        use super::*;

        #[test]
        fn test_parse_configure_subcommand_flags() {
            let cli = Cli::try_parse_from([
                "canon-clerk",
                "configure",
                "--screener-model",
                "flash-lite",
                "--auditor-model",
                "pro",
                "--reasoning-budget",
                "4096",
                "--json",
            ])
            .expect("parse configure subcommand");

            match cli.command {
                Some(Commands::Configure(args)) => {
                    assert_eq!(args.screener_model.as_deref(), Some("flash-lite"));
                    assert_eq!(args.auditor_model.as_deref(), Some("pro"));
                    assert_eq!(args.reasoning_budget, Some(4096));
                    assert!(args.json);
                }
                _ => panic!("expected Configure subcommand"),
            }
        }
    }

    mod execution_and_output_formats {
        use super::*;

        #[test]
        fn test_json_output_emits_normalized_caseload_without_key_leakage() {
            let secret = "synthetic-top-secret-never-echo-999";
            let args = ConfigureArgs {
                screener_model: Some("flash-lite".to_string()),
                auditor_model: Some("pro".to_string()),
                reasoning_budget: Some(2048),
                gemini_api_key: Some(secret.to_string()),
                json: true,
                ..ConfigureArgs::default()
            };
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let env = BTreeMap::new();

            let caseload = run_configure_with_env(&args, &mut stdin, &mut stdout, &env)
                .expect("run_configure_with_env");
            let out_str = String::from_utf8(stdout).expect("utf8 stdout");

            assert!(!out_str.contains(secret));
            let parsed: Caseload = serde_json::from_str(&out_str).expect("valid caseload json");
            assert_eq!(parsed, caseload);
            assert_eq!(
                parsed.config,
                Some(CaseloadConfig {
                    screener_model: DEFAULT_SCREENER_MODEL.to_string(),
                    auditor_model: DEFAULT_AUDITOR_MODEL.to_string(),
                    reasoning_budget: Some(2048),
                })
            );
        }

        #[test]
        fn test_stylish_and_compact_formats_never_echo_api_key() {
            let secret = "synthetic-secret-key-material-should-stay-hidden";
            let mut env = BTreeMap::new();
            env.insert(ENV_GEMINI_API_KEY.to_string(), secret.to_string());

            let stylish_args = ConfigureArgs {
                format: ConfigureFormat::Stylish,
                ..ConfigureArgs::default()
            };
            let mut stdin = Cursor::new(b"");
            let mut stylish_out = Vec::new();
            run_configure_with_env(&stylish_args, &mut stdin, &mut stylish_out, &env)
                .expect("stylish configure");
            let stylish_str = String::from_utf8(stylish_out).expect("utf8");
            assert!(stylish_str.contains("Configuration Summary (READY):"));
            assert!(stylish_str.contains("GEMINI_API_KEY (verified)"));
            assert!(!stylish_str.contains(secret));

            let compact_args = ConfigureArgs {
                format: ConfigureFormat::Compact,
                reasoning_budget: Some(1024),
                ..ConfigureArgs::default()
            };
            let mut compact_out = Vec::new();
            run_configure_with_env(&compact_args, &mut stdin, &mut compact_out, &env)
                .expect("compact configure");
            let compact_str = String::from_utf8(compact_out).expect("utf8");
            assert!(compact_str.contains("screener_model\tgoogle:gemini-flash-lite-latest"));
            assert!(compact_str.contains("auditor_model\tgoogle:gemini-pro-latest"));
            assert!(compact_str.contains("reasoning_budget\t1024"));
            assert!(!compact_str.contains(secret));
        }
    }

    mod error_handling_and_fast_forwarding {
        use super::*;

        #[test]
        fn test_missing_api_key_exits_with_code_2_and_hint() {
            let args = ConfigureArgs::default();
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let empty_env = BTreeMap::new();

            let err = run_configure_with_env(&args, &mut stdin, &mut stdout, &empty_env)
                .expect_err("missing key must fail");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("Hint:"));
        }

        #[test]
        fn test_caseload_stdin_fast_forwards_when_config_already_present() {
            let upstream = Caseload::new().with_config(CaseloadConfig {
                screener_model: "google:gemini-2.5-flash-lite".to_string(),
                auditor_model: "google:gemini-2.5-pro".to_string(),
                reasoning_budget: Some(4096),
            });
            let upstream_json = serde_json::to_string(&upstream).expect("serialize upstream");

            let args = ConfigureArgs {
                caseload: Some("-".to_string()),
                json: true,
                ..ConfigureArgs::default()
            };
            let mut stdin = Cursor::new(upstream_json.into_bytes());
            let mut stdout = Vec::new();
            let empty_env = BTreeMap::new();

            let result = run_configure_with_env(&args, &mut stdin, &mut stdout, &empty_env)
                .expect("fast-forward on pre-populated caseload.config");
            assert_eq!(result, upstream);
        }

        #[test]
        fn test_enriches_upstream_branch_a_caseload_from_stdin() {
            let raw = r#"{"version":"1.0","intake":{"scope":"targeted","target_paths":["src/lib.rs"],"diffs":{}}}"#;
            let args = ConfigureArgs {
                caseload: Some("-".to_string()),
                json: true,
                ..ConfigureArgs::default()
            };
            let mut stdin = Cursor::new(raw.as_bytes());
            let mut stdout = Vec::new();

            let result = run_configure_with_env(&args, &mut stdin, &mut stdout, &test_env())
                .expect("enrich branch A caseload");
            assert!(result.intake.is_some());
            assert_eq!(result.config, Some(CaseloadConfig::default()));
        }
    }
}
