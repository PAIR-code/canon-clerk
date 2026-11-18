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

use crate::caseload::{
    Caseload, CaseloadConfig, DEFAULT_AUDIT_THINKING_LEVEL, HeuristicNode, ResolvedNodeConfig,
};
use crate::cli::CliError;
use crate::cli::args::{CaseloadIoArgs, ProviderConfigArgs};
use crate::cli::node_args::AllNodeModelArgs;
use crate::cli::runners::{resolve_branch_b_options, run_branch_b_configure};
use crate::configure::{is_discriminative_model, verified_credential_env_vars};
use crate::telemetry::TelemetrySession;
use clap::{Args, ValueEnum};
use std::collections::BTreeMap;
use std::env;
use std::io::{Read, Write};

const CONFIGURE_EXAMPLES: &str = "\
Examples:
  # Resolve default Google provider node bindings and verify GEMINI_API_KEY:
  canon-clerk configure

  # Configure TypeSafe AI across all discriminative screening/vetting nodes:
  canon-clerk configure --provider typesafe

  # Pair TypeSafe Jev screening/vetting with Google Gemini 4 Carbon on audit:
  canon-clerk configure --provider typesafe --audit-model google:gemini-4-carbon

  # Override individual node models via canonical <provider>:<model> specifiers:
  canon-clerk configure --docket-model google:gemini-4-hydrogen --audit-model google:gemini-4-carbon

  # Configure maximum reasoning budget tokens for Generator-bound nodes:
  canon-clerk configure --reasoning-budget 4096

  # Emit tab-separated configuration key-value lines:
  canon-clerk configure --format compact

  # Ingest upstream Caseload JSON and emit enriched envelope to stdout:
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
    about = "Resolve and normalize provider credentials, per-node model bindings, and reasoning budgets",
    long_about = "Operational Environment Normalizer of the Caseload DAG (Branch B). Resolves the active default provider, cascades canonical <provider>:<model> bindings across heuristic nodes (apprise, docket, admit, vet, audit), verifies referenced runtime provider credentials (GEMINI_API_KEY, TYPESAFE_API_KEY), and populates `.config` on the cumulative Caseload without persisting secret material.",
    after_help = CONFIGURE_EXAMPLES
)]
pub struct ConfigureArgs {
    #[command(flatten)]
    pub provider: ProviderConfigArgs,

    #[command(flatten)]
    pub models: AllNodeModelArgs,

    /// Hidden capture rejecting ambiguous unqualified `--model` (`scheduling.md` §4).
    #[arg(long = "model", hide = true, allow_hyphen_values = true)]
    pub rejected_model: Option<String>,

    /// Output format (`stylish`, `json`, or `compact`).
    #[arg(
        long = "format",
        value_enum,
        default_value_t = ConfigureFormat::Stylish,
        help_heading = "Caseload & Output",
        help = "Format resolved configuration output"
    )]
    pub format: ConfigureFormat,

    #[command(flatten)]
    pub io: CaseloadIoArgs,
}

/// Executes the `configure` CLI subcommand against an explicit environment map and [`TelemetrySession`].
pub fn run_configure_with_telemetry(
    args: &ConfigureArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    env: &BTreeMap<String, String>,
    telemetry: &TelemetrySession<'_>,
) -> Result<Caseload, CliError> {
    if args.rejected_model.is_some() {
        return Err(CliError::usage(
            "Ambiguous unqualified flag `--model` is disallowed on `canon-clerk configure`.\n  \
             Hint: Pass `--provider <slug>` or per-node `--apprise-model`, `--docket-model`, `--admit-model`, `--vet-model`, or `--audit-model`.",
        ));
    }
    let incoming_caseload = args.io.read_caseload(stdin)?;
    let options = resolve_branch_b_options(&args.provider, args.models.to_node_map(), env)?;
    let caseload = run_branch_b_configure(&options, incoming_caseload, telemetry)?;
    telemetry.finish_pipeline();

    args.io.write_output_caseload(&caseload)?;

    let config = caseload.config.clone().unwrap_or_default();
    render_configure_output(args, &caseload, &config, stdout)?;

    Ok(caseload)
}

/// Executes the `configure` CLI subcommand against an explicit environment variable map.
pub fn run_configure_with_env(
    args: &ConfigureArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    env: &BTreeMap<String, String>,
) -> Result<Caseload, CliError> {
    run_configure_with_telemetry(args, stdin, stdout, env, &TelemetrySession::noop())
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
    let effective_format = if args.io.json {
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

fn verified_credentials_label(config: &CaseloadConfig) -> String {
    let keys = verified_credential_env_vars(config);
    format!("{} (verified)", keys.join(", "))
}

fn render_compact_output(config: &CaseloadConfig, stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(stdout, "provider\t{}", config.provider)?;
    for (node, node_cfg) in &config.nodes {
        writeln!(stdout, "{}_model\t{}", node.as_str(), node_cfg.model)?;
        if let Some(level) = node_cfg.thinking_level {
            writeln!(stdout, "{}_thinking_level\t{level}", node.as_str())?;
        }
        if let Some(budget) = node_cfg.reasoning_budget {
            writeln!(stdout, "{}_reasoning_budget\t{budget}", node.as_str())?;
        }
    }
    writeln!(
        stdout,
        "credentials\t{}",
        verified_credential_env_vars(config).join(",")
    )?;
    Ok(())
}

fn format_stylish_thinking_suffix(node: HeuristicNode, node_cfg: &ResolvedNodeConfig) -> String {
    if is_discriminative_model(&node_cfg.model) {
        return String::new();
    }
    match (node_cfg.thinking_level, node_cfg.reasoning_budget) {
        (Some(level), _) => format!(" (thinking: {level})"),
        (None, Some(budget)) => format!(" (reasoning budget: {budget} tokens)"),
        (None, None) if node == HeuristicNode::Audit => {
            format!(" (thinking: {DEFAULT_AUDIT_THINKING_LEVEL} default)")
        }
        (None, None) => String::new(),
    }
}

fn render_stylish_output(config: &CaseloadConfig, stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(stdout, "Configuration Summary (READY):")?;
    writeln!(stdout, "  Provider         : {}", config.provider)?;
    for node in HeuristicNode::ALL {
        let label = format!("{} model", node.as_str());
        match config.node(node) {
            Some(node_cfg) => {
                let suffix = format_stylish_thinking_suffix(node, node_cfg);
                writeln!(stdout, "  {label:<16} : {}{suffix}", node_cfg.model)?;
            }
            None => {
                writeln!(
                    stdout,
                    "  {label:<16} : (unprovisioned — pass --audit-model <provider:model>)"
                )?;
            }
        }
    }
    writeln!(
        stdout,
        "  Credentials      : {}",
        verified_credentials_label(config)
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{
        DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, DEFAULT_GOOGLE_GENERATOR_MODEL,
        DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL, TYPESAFE_PROVIDER,
    };
    use crate::cli::{Cli, Commands};
    use crate::configure::{ENV_GEMINI_API_KEY, ENV_TYPESAFE_API_KEY};
    use clap::Parser;
    use std::io::Cursor;

    fn test_env() -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        env.insert(
            ENV_GEMINI_API_KEY.to_string(),
            "synthetic-test-api-key-value".to_string(),
        );
        env.insert(
            "XDG_CONFIG_HOME".to_string(),
            "/nonexistent-canon-clerk-xdg-dir".to_string(),
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
                "--provider",
                "typesafe",
                "--docket-model",
                "google:gemini-2.5-flash-lite",
                "--audit-model",
                "google:gemini-2.5-pro",
                "--reasoning-budget",
                "4096",
                "--json",
            ])
            .expect("parse configure subcommand");

            match cli.command {
                Some(Commands::Configure(args)) => {
                    assert_eq!(args.provider.provider.as_deref(), Some("typesafe"));
                    assert_eq!(
                        args.models.docket_model.as_deref(),
                        Some("google:gemini-2.5-flash-lite")
                    );
                    assert_eq!(
                        args.models.audit_model.as_deref(),
                        Some("google:gemini-2.5-pro")
                    );
                    assert_eq!(args.provider.reasoning_budget, Some(4096));
                    assert!(args.io.json);
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
                provider: ProviderConfigArgs {
                    reasoning_budget: Some(2048),
                    gemini_api_key: Some(secret.to_string()),
                    ..ProviderConfigArgs::default()
                },
                io: CaseloadIoArgs {
                    json: true,
                    ..CaseloadIoArgs::default()
                },
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
                Some(CaseloadConfig::default().with_node(
                    HeuristicNode::Audit,
                    ResolvedNodeConfig::with_thinking(
                        DEFAULT_GOOGLE_GENERATOR_MODEL,
                        None,
                        Some(2048)
                    ),
                ))
            );
        }

        #[test]
        fn test_stylish_and_compact_formats_never_echo_api_key() {
            let secret = "synthetic-secret-key-material-should-stay-hidden";
            let mut env = BTreeMap::new();
            env.insert(ENV_GEMINI_API_KEY.to_string(), secret.to_string());
            env.insert(
                "XDG_CONFIG_HOME".to_string(),
                "/nonexistent-canon-clerk-xdg-dir".to_string(),
            );

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
            assert!(stylish_str.contains("Provider         : google"));
            assert!(stylish_str.contains("google:gemini-4-carbon (thinking: low default)"));
            assert!(stylish_str.contains("GEMINI_API_KEY (verified)"));
            assert!(!stylish_str.contains(secret));

            let compact_args = ConfigureArgs {
                provider: ProviderConfigArgs {
                    reasoning_budget: Some(1024),
                    ..ProviderConfigArgs::default()
                },
                format: ConfigureFormat::Compact,
                ..ConfigureArgs::default()
            };
            let mut compact_out = Vec::new();
            run_configure_with_env(&compact_args, &mut stdin, &mut compact_out, &env)
                .expect("compact configure");
            let compact_str = String::from_utf8(compact_out).expect("utf8");
            assert!(compact_str.contains("provider\tgoogle"));
            assert!(compact_str.contains("docket_model\tgoogle:gemini-decision-1"));
            assert!(compact_str.contains("audit_model\tgoogle:gemini-4-carbon"));
            assert!(compact_str.contains("audit_reasoning_budget\t1024"));
            assert!(!compact_str.contains("docket_reasoning_budget"));
            assert!(!compact_str.contains(secret));
        }

        #[test]
        fn test_typesafe_standalone_and_hybrid_audit_pin_configure_cleanly() {
            let ts_secret = "synthetic-typesafe-cli-secret-never-echo";
            let mut ts_only_env = BTreeMap::new();
            ts_only_env.insert(ENV_TYPESAFE_API_KEY.to_string(), ts_secret.to_string());
            ts_only_env.insert(
                "XDG_CONFIG_HOME".to_string(),
                "/nonexistent-canon-clerk-xdg-dir".to_string(),
            );

            // 1. Standalone `--provider typesafe` succeeds with ONLY `TYPESAFE_API_KEY`
            let ts_args = ConfigureArgs {
                provider: ProviderConfigArgs {
                    provider: Some(TYPESAFE_PROVIDER.to_string()),
                    ..ProviderConfigArgs::default()
                },
                format: ConfigureFormat::Stylish,
                ..ConfigureArgs::default()
            };
            let mut stylish_out = Vec::new();
            let ts_caseload = run_configure_with_env(
                &ts_args,
                &mut Cursor::new(b""),
                &mut stylish_out,
                &ts_only_env,
            )
            .expect("standalone --provider typesafe must succeed without GEMINI_API_KEY");
            let rendered = String::from_utf8(stylish_out).expect("utf8");
            assert!(rendered.contains("Provider         : typesafe"));
            assert!(rendered.contains(DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL));
            assert!(rendered.contains("(unprovisioned — pass --audit-model <provider:model>)"));
            assert!(rendered.contains("Credentials      : TYPESAFE_API_KEY (verified)"));
            assert!(!rendered.contains(ts_secret));
            let ts_cfg = ts_caseload.config.expect("config");
            assert_eq!(ts_cfg.nodes.len(), 4);
            assert!(!ts_cfg.nodes.contains_key(&HeuristicNode::Audit));

            // 2. Hybrid `--provider typesafe --audit-model google:gemini-4-carbon` verifies both keys
            let hybrid_args = ConfigureArgs {
                provider: ProviderConfigArgs {
                    provider: Some(TYPESAFE_PROVIDER.to_string()),
                    typesafe_api_key: Some(ts_secret.to_string()),
                    ..ProviderConfigArgs::default()
                },
                models: AllNodeModelArgs {
                    audit_model: Some(DEFAULT_GOOGLE_GENERATOR_MODEL.to_string()),
                    ..AllNodeModelArgs::default()
                },
                format: ConfigureFormat::Compact,
                ..ConfigureArgs::default()
            };
            let mut compact_out = Vec::new();
            run_configure_with_env(
                &hybrid_args,
                &mut Cursor::new(b""),
                &mut compact_out,
                &test_env(),
            )
            .expect("hybrid typesafe + google audit");
            let compact_str = String::from_utf8(compact_out).expect("utf8");
            assert!(compact_str.contains("provider\ttypesafe"));
            assert!(compact_str.contains("vet_model\ttypesafe:jev-latest"));
            assert!(compact_str.contains("audit_model\tgoogle:gemini-4-carbon"));
            assert!(compact_str.contains("credentials\tGEMINI_API_KEY,TYPESAFE_API_KEY"));
            assert!(!compact_str.contains(ts_secret));
        }
    }

    mod error_handling_and_fast_forwarding {
        use super::*;

        #[test]
        fn test_missing_api_key_exits_with_code_2_and_hint() {
            let args = ConfigureArgs::default();
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let mut empty_env = BTreeMap::new();
            empty_env.insert(
                "XDG_CONFIG_HOME".to_string(),
                "/nonexistent-canon-clerk-xdg-dir".to_string(),
            );

            let err = run_configure_with_env(&args, &mut stdin, &mut stdout, &empty_env)
                .expect_err("missing key must fail");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains(ENV_GEMINI_API_KEY));
            assert!(err.to_string().contains("Hint:"));

            // Ambient TYPESAFE_API_KEY alone does NOT silently switch provider away from google
            let mut ts_only_env = empty_env;
            ts_only_env.insert(
                ENV_TYPESAFE_API_KEY.to_string(),
                "ts-only-secret".to_string(),
            );
            let ts_err = run_configure_with_env(&args, &mut stdin, &mut stdout, &ts_only_env)
                .expect_err("default google configure without GEMINI_API_KEY must exit 2");
            assert_eq!(ts_err.exit_code(), 2);
            assert!(ts_err.to_string().contains(ENV_GEMINI_API_KEY));
            assert!(ts_err.to_string().contains("Hint:"));
        }

        #[test]
        fn test_retired_flags_bare_specifiers_and_discriminative_audit_exit_with_code_2_and_hint() {
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let mut both_env = test_env();
            both_env.insert(
                ENV_TYPESAFE_API_KEY.to_string(),
                "synthetic-ts-key".to_string(),
            );

            for retired_flag in ["--screener-model", "--auditor-model", "--model"] {
                let parsed = Cli::try_parse_from([
                    "canon-clerk",
                    "configure",
                    retired_flag,
                    "google:gemini-4-carbon",
                ])
                .expect("hidden capture parses flag");
                let Some(Commands::Configure(cfg_args)) = parsed.command else {
                    panic!("expected Configure");
                };
                let err = run_configure_with_env(&cfg_args, &mut stdin, &mut stdout, &both_env)
                    .expect_err("retired/ambiguous flag must exit 2");
                assert_eq!(err.exit_code(), 2);
                assert!(err.to_string().contains("Hint:"));
            }

            for bad_spec in [
                "gemini-4-hydrogen",
                "jev-latest",
                "gemini-decision-1",
                "google:hydrogen",
                "typesafe:jev",
                "google:gemini-decision",
            ] {
                let spec_args = ConfigureArgs {
                    models: AllNodeModelArgs {
                        docket_model: Some(bad_spec.to_string()),
                        ..AllNodeModelArgs::default()
                    },
                    ..ConfigureArgs::default()
                };
                let err = run_configure_with_env(&spec_args, &mut stdin, &mut stdout, &both_env)
                    .expect_err(&format!("invalid specifier `{bad_spec}` must exit 2"));
                assert_eq!(err.exit_code(), 2);
                assert!(err.to_string().contains("Invalid model specifier"));
                assert!(err.to_string().contains("Hint:"));
            }

            for disc_audit in [
                DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL,
                DEFAULT_GOOGLE_DISCRIMINATOR_MODEL,
            ] {
                let args = ConfigureArgs {
                    models: AllNodeModelArgs {
                        audit_model: Some(disc_audit.to_string()),
                        ..AllNodeModelArgs::default()
                    },
                    ..ConfigureArgs::default()
                };
                let err = run_configure_with_env(&args, &mut stdin, &mut stdout, &both_env)
                    .expect_err("discriminative audit model must exit 2");
                assert_eq!(err.exit_code(), 2);
                assert!(err.to_string().contains("cannot be configured on `audit`"));
                assert!(err.to_string().contains("Hint:"));
            }
        }

        #[test]
        fn test_caseload_stdin_fast_forwards_and_enriches_upstream_branch_a_caseload() {
            let upstream = Caseload::new().with_config(
                CaseloadConfig::default()
                    .with_node_model(HeuristicNode::Docket, "google:gemini-2.5-flash-lite")
                    .with_node_model(HeuristicNode::Audit, "google:gemini-2.5-pro"),
            );
            let upstream_json = serde_json::to_string(&upstream).expect("serialize upstream");

            let args = ConfigureArgs {
                io: CaseloadIoArgs {
                    caseload: Some("-".to_string()),
                    json: true,
                    ..CaseloadIoArgs::default()
                },
                ..ConfigureArgs::default()
            };
            let result = run_configure_with_env(
                &args,
                &mut Cursor::new(upstream_json.into_bytes()),
                &mut Vec::new(),
                &BTreeMap::new(),
            )
            .expect("fast-forward on pre-populated caseload.config");
            assert_eq!(result, upstream);

            let raw = r#"{"version":"1.0","intake":{"scope":"targeted","target_paths":["src/lib.rs"],"diffs":{}}}"#;
            let enriched = run_configure_with_env(
                &args,
                &mut Cursor::new(raw.as_bytes()),
                &mut Vec::new(),
                &test_env(),
            )
            .expect("enrich branch A caseload");
            assert!(enriched.intake.is_some());
            assert_eq!(enriched.config, Some(CaseloadConfig::default()));
        }

        #[test]
        fn test_retired_env_var_and_legacy_flat_config_json_keys_exit_2() {
            use std::fs;
            use std::os::unix::fs::PermissionsExt;
            use std::time::{SystemTime, UNIX_EPOCH};

            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let xdg_dir = std::env::temp_dir().join(format!("canon-clerk-cfg-retired-{unique}"));
            let cfg_dir = xdg_dir.join("canon-clerk");
            fs::create_dir_all(&cfg_dir).expect("create xdg dir");
            let cfg_file = cfg_dir.join("config.json");

            let mut retired_env = BTreeMap::from([
                ("XDG_CONFIG_HOME".to_string(), xdg_dir.display().to_string()),
                (ENV_GEMINI_API_KEY.to_string(), "test-key".to_string()),
                (
                    "CANON_CLERK_SCREENER_MODEL".to_string(),
                    "google:gemini-decision-1".to_string(),
                ),
            ]);
            let env_err = run_configure_with_env(
                &ConfigureArgs::default(),
                &mut Cursor::new(b""),
                &mut Vec::new(),
                &retired_env,
            )
            .expect_err("retired env var must fail exit 2");
            assert_eq!(env_err.exit_code(), 2);
            assert!(env_err.to_string().contains("Hint:"));

            retired_env.remove("CANON_CLERK_SCREENER_MODEL");
            fs::write(
                &cfg_file,
                r#"{"screenerModel": "google:gemini-decision-1"}"#,
            )
            .expect("write legacy config");
            fs::set_permissions(&cfg_file, fs::Permissions::from_mode(0o600)).expect("chmod 0600");
            let legacy_err = run_configure_with_env(
                &ConfigureArgs::default(),
                &mut Cursor::new(b""),
                &mut Vec::new(),
                &retired_env,
            )
            .expect_err("legacy flat config.json key must fail exit 2");
            assert_eq!(legacy_err.exit_code(), 2);
            assert!(legacy_err.to_string().contains("Hint:"));

            let _ = fs::remove_dir_all(&xdg_dir);
        }

        #[test]
        fn test_structured_host_config_json_resolves_cleanly() {
            use crate::caseload::ThinkingLevel;
            use std::fs;
            use std::os::unix::fs::PermissionsExt;
            use std::time::{SystemTime, UNIX_EPOCH};

            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let xdg_dir = std::env::temp_dir().join(format!("canon-clerk-cfg-struct-{unique}"));
            let cfg_dir = xdg_dir.join("canon-clerk");
            fs::create_dir_all(&cfg_dir).expect("create xdg dir");
            let cfg_file = cfg_dir.join("config.json");
            let xdg_env =
                BTreeMap::from([("XDG_CONFIG_HOME".to_string(), xdg_dir.display().to_string())]);
            fs::write(
                &cfg_file,
                r#"{
                    "providers": {
                        "google": { "apiKey": "cfg-google-key" },
                        "typesafe": { "apiKey": "cfg-typesafe-key" }
                    },
                    "defaults": {
                        "provider": "typesafe",
                        "thinkingLevel": "high"
                    },
                    "nodes": {
                        "audit": { "model": "google:gemini-4-carbon" }
                    }
                }"#,
            )
            .expect("write structured config");
            fs::set_permissions(&cfg_file, fs::Permissions::from_mode(0o600)).expect("chmod 0600");

            let resolved = run_configure_with_env(
                &ConfigureArgs::default(),
                &mut Cursor::new(b""),
                &mut Vec::new(),
                &xdg_env,
            )
            .expect("resolve structured config.json");
            let resolved_cfg = resolved.config.expect("config");
            assert_eq!(resolved_cfg.provider, "typesafe");
            assert_eq!(
                resolved_cfg.model_for(HeuristicNode::Docket),
                Some("typesafe:jev-latest")
            );
            assert_eq!(
                resolved_cfg
                    .node(HeuristicNode::Docket)
                    .and_then(|n| n.thinking_level),
                None
            );
            assert_eq!(
                resolved_cfg.model_for(HeuristicNode::Audit),
                Some("google:gemini-4-carbon")
            );
            assert_eq!(
                resolved_cfg
                    .node(HeuristicNode::Audit)
                    .and_then(|n| n.thinking_level),
                Some(ThinkingLevel::High)
            );

            let _ = fs::remove_dir_all(&xdg_dir);
        }
    }
}
