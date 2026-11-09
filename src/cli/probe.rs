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

//! CLI driving adapter for the `canon-clerk probe` subcommand (`docs/architecture/nodes/probe.md` §5).

use crate::caseload::{Caseload, CaseloadProbe, ProbeEndpointResult, ProbeStatus};
use crate::cli::CliError;
use crate::cli::discover::read_caseload_operand;
use crate::cli::intake::write_caseload_file;
use crate::configure::{
    ConfigureOptions, execute_configure, load_host_config_file, resolve_default_config_path,
    resolve_gemini_api_key,
};
use crate::probe::{
    DEFAULT_PROBE_TIMEOUT_MS, ENV_PROBE_TIMEOUT_MS, EndpointProber, GeminiEndpointProber,
    ProbeOptions, execute_probe,
};
use clap::{Args, ValueEnum};
use std::collections::BTreeMap;
use std::env;
use std::io::{Read, Write};
use std::path::PathBuf;

const PROBE_EXAMPLES: &str = "\
Examples:
  # Verify connectivity and latency to configured screener and auditor endpoints:
  canon-clerk probe

  # Emit structured Caseload JSON with .config and .probe records:
  canon-clerk probe --json

  # Override timeout and model tiers for diagnostic verification:
  canon-clerk probe --timeout 3000 --screener-model flash-lite --auditor-model pro";

/// Output presentation format for `canon-clerk probe`.
#[derive(ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProbeFormat {
    /// Human-readable endpoint reachability and latency report.
    #[default]
    Stylish,
    /// Full cumulative `Caseload` JSON envelope.
    Json,
    /// Tab-separated (`<tier>\t<status>\t<latency_ms>\t<model>`) one-line-per-endpoint output.
    Compact,
}

/// Command-line arguments for `canon-clerk probe`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
#[command(
    about = "Verify live connectivity, credentials, and roundtrip latency for configured model endpoints",
    long_about = "Diagnostic Network Health Check of the Caseload DAG (Branch B leaf). Executes the [configure, probe] closure by resolving provider credentials and concurrently probing both `.config.screener_model` and `.config.auditor_model` via minimal POST :generateContent requests, recording non-throwing diagnostics on `.probe`.",
    after_help = PROBE_EXAMPLES
)]
pub struct ProbeArgs {
    /// Per-endpoint request timeout in milliseconds (default: `5000`; env: `CANON_CLERK_PROBE_TIMEOUT_MS`).
    #[arg(
        long = "timeout",
        value_name = "MS",
        help_heading = "Probe Options",
        help = "Per-endpoint request timeout in milliseconds (default: 5000; fallback env: CANON_CLERK_PROBE_TIMEOUT_MS)"
    )]
    pub timeout: Option<u64>,

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
        default_value_t = ProbeFormat::Stylish,
        help_heading = "Caseload & Output",
        help = "Format endpoint probe diagnostic output"
    )]
    pub format: ProbeFormat,

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

/// Executes the `probe` CLI subcommand against an explicit environment map and hexagonal [`EndpointProber`].
pub fn run_probe_with_env_and_prober(
    args: &ProbeArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    env: &BTreeMap<String, String>,
    prober: &dyn EndpointProber,
) -> Result<Caseload, CliError> {
    let incoming_caseload = read_caseload_operand(args.caseload.as_deref(), stdin)?;
    let timeout_ms = resolve_timeout_ms(args.timeout, env)?;

    let cwd = env::current_dir().ok();
    let stored_config = match resolve_default_config_path(env) {
        Some(cfg_path) => load_host_config_file(&cfg_path, cwd.as_deref())
            .map_err(|err| CliError::usage(err.to_string()))?,
        None => None,
    };

    let configure_options = ConfigureOptions {
        screener_model: args.screener_model.clone(),
        auditor_model: args.auditor_model.clone(),
        reasoning_budget: args.reasoning_budget,
        gemini_api_key: args.gemini_api_key.clone(),
        env: env.clone(),
        stored_config,
    };

    let caseload = if let Some(existing) = incoming_caseload.as_ref()
        && existing.probe.is_some()
        && !configure_options.has_explicit_overrides()
    {
        existing.clone()
    } else {
        let configured = execute_configure(&configure_options, incoming_caseload)
            .map_err(|err| CliError::usage(err.to_string()))?;
        let api_key = resolve_gemini_api_key(&configure_options)
            .map_err(|err| CliError::usage(err.to_string()))?;
        let probe_options = ProbeOptions {
            api_key,
            timeout_ms,
        };
        execute_probe(&probe_options, prober, configured)
            .map_err(|err| CliError::usage(err.to_string()))?
    };

    if let Some(output_path) = &args.output_caseload {
        write_caseload_file(output_path, &caseload)?;
    }

    if let Some(probe) = &caseload.probe {
        render_probe_output(args, &caseload, probe, stdout)?;
        if !probe.is_healthy() {
            return Err(CliError::usage(format_unhealthy_probe_error(probe)));
        }
    }

    Ok(caseload)
}

/// Executes the `probe` CLI subcommand using the process environment and live [`GeminiEndpointProber`].
pub fn run_probe(
    args: &ProbeArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    let env: BTreeMap<String, String> = env::vars().collect();
    let prober = GeminiEndpointProber::new();
    run_probe_with_env_and_prober(args, stdin, stdout, &env, &prober)
}

fn resolve_timeout_ms(
    flag_timeout: Option<u64>,
    env: &BTreeMap<String, String>,
) -> Result<u64, CliError> {
    let resolved = if let Some(ms) = flag_timeout {
        ms
    } else if let Some(raw_env) = env
        .get(ENV_PROBE_TIMEOUT_MS)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        raw_env.parse::<u64>().map_err(|_| {
            CliError::usage(format!(
                "Invalid probe timeout `{raw_env}` in `{ENV_PROBE_TIMEOUT_MS}`: expected a positive integer in milliseconds.\n  \
                 Hint: Pass `--timeout <MS>` or set `{ENV_PROBE_TIMEOUT_MS}=<MS>` (default: `{DEFAULT_PROBE_TIMEOUT_MS}`)."
            ))
        })?
    } else {
        DEFAULT_PROBE_TIMEOUT_MS
    };

    if resolved == 0 {
        return Err(CliError::usage(format!(
            "Invalid probe timeout `0ms`: timeout must be greater than 0ms.\n  \
             Hint: Pass a positive millisecond timeout via `--timeout <MS>` (default: `{DEFAULT_PROBE_TIMEOUT_MS}`)."
        )));
    }

    Ok(resolved)
}

fn render_probe_output(
    args: &ProbeArgs,
    caseload: &Caseload,
    probe: &CaseloadProbe,
    stdout: &mut dyn Write,
) -> Result<(), CliError> {
    let effective_format = if args.json {
        ProbeFormat::Json
    } else {
        args.format
    };

    match effective_format {
        ProbeFormat::Json => {
            let serialized = serde_json::to_string_pretty(caseload).map_err(|err| {
                CliError::runtime(format!("Failed to serialize Caseload JSON: {err}"))
            })?;
            writeln!(stdout, "{serialized}")
                .map_err(|err| CliError::runtime(format!("Failed to write to stdout: {err}")))?;
        }
        ProbeFormat::Compact => {
            render_compact_output(probe, stdout)
                .map_err(|err| CliError::runtime(format!("Failed to write to stdout: {err}")))?;
        }
        ProbeFormat::Stylish => {
            render_stylish_output(probe, stdout)
                .map_err(|err| CliError::runtime(format!("Failed to write to stdout: {err}")))?;
        }
    }

    Ok(())
}

fn render_compact_output(probe: &CaseloadProbe, stdout: &mut dyn Write) -> std::io::Result<()> {
    write_compact_endpoint_line("screener", &probe.screener, stdout)?;
    write_compact_endpoint_line("auditor", &probe.auditor, stdout)?;
    Ok(())
}

fn write_compact_endpoint_line(
    tier: &str,
    ep: &ProbeEndpointResult,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    match &ep.error {
        Some(err) => writeln!(
            stdout,
            "{tier}\t{}\t{}ms\t{}\t{err}",
            ep.status.as_str(),
            ep.latency_ms,
            ep.model
        ),
        None => writeln!(
            stdout,
            "{tier}\t{}\t{}ms\t{}",
            ep.status.as_str(),
            ep.latency_ms,
            ep.model
        ),
    }
}

fn render_stylish_output(probe: &CaseloadProbe, stdout: &mut dyn Write) -> std::io::Result<()> {
    let header = if probe.is_healthy() {
        "Endpoint Health Check (HEALTHY):"
    } else {
        "Endpoint Health Check (FAILED):"
    };
    writeln!(stdout, "{header}")?;
    write_stylish_endpoint_line("Screener", &probe.screener, stdout)?;
    write_stylish_endpoint_line("Auditor ", &probe.auditor, stdout)?;
    Ok(())
}

fn write_stylish_endpoint_line(
    label: &str,
    ep: &ProbeEndpointResult,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    match ep.status {
        ProbeStatus::Ok => writeln!(
            stdout,
            "  ✓ {label} : ok ({}ms, model: {})",
            ep.latency_ms, ep.model
        ),
        ProbeStatus::Error => {
            let reason = ep.error.as_deref().unwrap_or("unreachable");
            writeln!(
                stdout,
                "  ✗ {label} : error ({}ms, model: {}) — {reason}",
                ep.latency_ms, ep.model
            )
        }
    }
}

fn format_unhealthy_probe_error(probe: &CaseloadProbe) -> String {
    let mut details = Vec::new();
    if probe.screener.status == ProbeStatus::Error {
        let msg = probe
            .screener
            .error
            .as_deref()
            .unwrap_or("Screener endpoint probe failed.");
        details.push(format!("screener ({}): {msg}", probe.screener.model));
    }
    if probe.auditor.status == ProbeStatus::Error {
        let msg = probe
            .auditor
            .error
            .as_deref()
            .unwrap_or("Auditor endpoint probe failed.");
        details.push(format!("auditor ({}): {msg}", probe.auditor.model));
    }
    format!(
        "One or more model endpoint probes failed:\n  - {}",
        details.join("\n  - ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{
        CaseloadConfig, CaseloadIntake, DEFAULT_AUDITOR_MODEL, DEFAULT_SCREENER_MODEL,
    };
    use crate::cli::{Cli, Commands};
    use crate::configure::ENV_GEMINI_API_KEY;
    use clap::Parser;
    use std::io::Cursor;

    struct MockProber {
        screener: ProbeEndpointResult,
        auditor: ProbeEndpointResult,
    }

    impl MockProber {
        fn healthy() -> Self {
            Self {
                screener: ProbeEndpointResult {
                    status: ProbeStatus::Ok,
                    latency_ms: 88,
                    model: "gemini-2.5-flash-lite".to_string(),
                    error: None,
                },
                auditor: ProbeEndpointResult {
                    status: ProbeStatus::Ok,
                    latency_ms: 194,
                    model: "gemini-2.5-pro".to_string(),
                    error: None,
                },
            }
        }
    }

    impl EndpointProber for MockProber {
        fn probe_endpoint(
            &self,
            model_specifier: &str,
            _api_key: &str,
            _timeout_ms: u64,
        ) -> ProbeEndpointResult {
            if model_specifier.contains("flash") {
                self.screener.clone()
            } else {
                self.auditor.clone()
            }
        }
    }

    fn test_env() -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        env.insert(
            ENV_GEMINI_API_KEY.to_string(),
            "synthetic-probe-api-key".to_string(),
        );
        env
    }

    mod cli_argument_parsing {
        use super::*;

        #[test]
        fn test_parse_probe_subcommand_flags() {
            let cli = Cli::try_parse_from([
                "canon-clerk",
                "probe",
                "--timeout",
                "3500",
                "--screener-model",
                "flash-lite",
                "--auditor-model",
                "pro",
                "--json",
            ])
            .expect("parse probe subcommand");

            match cli.command {
                Some(Commands::Probe(args)) => {
                    assert_eq!(args.timeout, Some(3500));
                    assert_eq!(args.screener_model.as_deref(), Some("flash-lite"));
                    assert_eq!(args.auditor_model.as_deref(), Some("pro"));
                    assert!(args.json);
                }
                _ => panic!("expected Probe subcommand"),
            }
        }
    }

    mod telescoping_and_output_formats {
        use super::*;

        #[test]
        fn test_telescopes_configure_and_probe_into_json_output_without_key_leakage() {
            let secret = "synthetic-secret-probe-key-never-echo-777";
            let args = ProbeArgs {
                gemini_api_key: Some(secret.to_string()),
                json: true,
                ..ProbeArgs::default()
            };
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();
            let prober = MockProber::healthy();

            let caseload = run_probe_with_env_and_prober(
                &args,
                &mut stdin,
                &mut stdout,
                &BTreeMap::new(),
                &prober,
            )
            .expect("healthy probe should exit 0");

            let out_str = String::from_utf8(stdout).expect("utf8 stdout");
            assert!(!out_str.contains(secret));
            let parsed: Caseload = serde_json::from_str(&out_str).expect("valid caseload json");
            assert_eq!(parsed, caseload);
            assert!(parsed.config.is_some());
            let prb = parsed.probe.expect("probe present");
            assert!(prb.is_healthy());
            assert_eq!(prb.screener.latency_ms, 88);
            assert_eq!(prb.auditor.latency_ms, 194);
        }

        #[test]
        fn test_stylish_and_compact_formats_render_healthy_endpoints() {
            let prober = MockProber::healthy();
            let env = test_env();

            let stylish_args = ProbeArgs {
                format: ProbeFormat::Stylish,
                ..ProbeArgs::default()
            };
            let mut stdin = Cursor::new(b"");
            let mut stylish_out = Vec::new();
            run_probe_with_env_and_prober(
                &stylish_args,
                &mut stdin,
                &mut stylish_out,
                &env,
                &prober,
            )
            .expect("stylish probe");
            let stylish_str = String::from_utf8(stylish_out).expect("utf8");
            assert!(stylish_str.contains("Endpoint Health Check (HEALTHY):"));
            assert!(stylish_str.contains("88ms"));
            assert!(stylish_str.contains("194ms"));

            let compact_args = ProbeArgs {
                format: ProbeFormat::Compact,
                ..ProbeArgs::default()
            };
            let mut compact_out = Vec::new();
            run_probe_with_env_and_prober(
                &compact_args,
                &mut stdin,
                &mut compact_out,
                &env,
                &prober,
            )
            .expect("compact probe");
            let compact_str = String::from_utf8(compact_out).expect("utf8");
            assert!(compact_str.contains("screener\tok\t88ms\tgemini-2.5-flash-lite"));
            assert!(compact_str.contains("auditor\tok\t194ms\tgemini-2.5-pro"));
        }
    }

    mod failure_capture_and_fast_forwarding {
        use super::*;

        #[test]
        fn test_unhealthy_endpoint_emits_caseload_probe_and_exits_with_code_2() {
            let failing_prober = MockProber {
                screener: ProbeEndpointResult {
                    status: ProbeStatus::Ok,
                    latency_ms: 92,
                    model: "gemini-2.5-flash-lite".to_string(),
                    error: None,
                },
                auditor: ProbeEndpointResult {
                    status: ProbeStatus::Error,
                    latency_ms: 64,
                    model: DEFAULT_AUDITOR_MODEL.to_string(),
                    error: Some(
                        "HTTP 401: Authentication or permission rejected. Hint: Verify `GEMINI_API_KEY`."
                            .to_string(),
                    ),
                },
            };

            let args = ProbeArgs {
                json: true,
                ..ProbeArgs::default()
            };
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();

            let err = run_probe_with_env_and_prober(
                &args,
                &mut stdin,
                &mut stdout,
                &test_env(),
                &failing_prober,
            )
            .expect_err("failing endpoint must return exit 2 error");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("HTTP 401"));
            assert!(err.to_string().contains("Hint:"));

            // Non-Throwing Diagnostic Capture Invariant: stdout still contains the enriched Caseload with .probe
            let out_str = String::from_utf8(stdout).expect("utf8 stdout");
            let parsed: Caseload = serde_json::from_str(&out_str).expect("parse caseload json");
            let probe = parsed.probe.expect("probe captured before exit 2");
            assert_eq!(probe.screener.status, ProbeStatus::Ok);
            assert_eq!(probe.auditor.status, ProbeStatus::Error);
        }

        #[test]
        fn test_missing_credentials_or_zero_timeout_exits_with_code_2() {
            let prober = MockProber::healthy();
            let mut stdin = Cursor::new(b"");
            let mut stdout = Vec::new();

            let no_key_err = run_probe_with_env_and_prober(
                &ProbeArgs::default(),
                &mut stdin,
                &mut stdout,
                &BTreeMap::new(),
                &prober,
            )
            .expect_err("missing key must fail");
            assert_eq!(no_key_err.exit_code(), 2);
            assert!(no_key_err.to_string().contains("Hint:"));

            let zero_timeout_err = run_probe_with_env_and_prober(
                &ProbeArgs {
                    timeout: Some(0),
                    ..ProbeArgs::default()
                },
                &mut stdin,
                &mut stdout,
                &test_env(),
                &prober,
            )
            .expect_err("zero timeout must fail");
            assert_eq!(zero_timeout_err.exit_code(), 2);
        }

        #[test]
        fn test_fast_forwards_past_configure_and_probe_when_already_populated() {
            let upstream = Caseload::new()
                .with_intake(CaseloadIntake {
                    pr_title: Some("feat: upstream".to_string()),
                    ..CaseloadIntake::default()
                })
                .with_config(CaseloadConfig::default())
                .with_probe(CaseloadProbe {
                    screener: ProbeEndpointResult {
                        status: ProbeStatus::Ok,
                        latency_ms: 33,
                        model: DEFAULT_SCREENER_MODEL.to_string(),
                        error: None,
                    },
                    auditor: ProbeEndpointResult {
                        status: ProbeStatus::Ok,
                        latency_ms: 77,
                        model: DEFAULT_AUDITOR_MODEL.to_string(),
                        error: None,
                    },
                });
            let raw = serde_json::to_string(&upstream).expect("serialize upstream");

            let args = ProbeArgs {
                caseload: Some("-".to_string()),
                json: true,
                ..ProbeArgs::default()
            };
            let mut stdin = Cursor::new(raw.into_bytes());
            let mut stdout = Vec::new();
            let prober = MockProber::healthy();

            let result = run_probe_with_env_and_prober(
                &args,
                &mut stdin,
                &mut stdout,
                &BTreeMap::new(),
                &prober,
            )
            .expect("fast-forward when .probe already populated");
            assert_eq!(result, upstream);
        }
    }
}
