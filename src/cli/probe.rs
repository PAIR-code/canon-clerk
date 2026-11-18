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

use crate::caseload::{Caseload, CaseloadProbe, HeuristicNode, ProbeStatus, ProbeTargetResult};
use crate::cli::CliError;
use crate::cli::args::{CaseloadIoArgs, ProviderConfigArgs};
use crate::cli::node_args::AllNodeModelArgs;
use crate::cli::runners::{
    resolve_branch_b_options, resolve_gemini_api_key, resolve_typesafe_api_key,
    run_branch_b_configure,
};
use crate::probe::{
    DEFAULT_PROBE_TIMEOUT_MS, ENV_PROBE_TIMEOUT_MS, EndpointProber, MultiProviderEndpointProber,
    ProbeOptions, execute_probe,
};
use crate::telemetry::{PipelineStage, TelemetrySession};
use clap::{Args, ValueEnum};
use std::collections::BTreeMap;
use std::env;
use std::io::{Read, Write};

const PROBE_EXAMPLES: &str = "\
Examples:
  # Verify connectivity and latency across deduplicated Google default endpoints:
  canon-clerk probe

  # Probe TypeSafe AI Jev endpoint standalone across discriminative nodes:
  canon-clerk probe --provider typesafe

  # Emit structured Caseload JSON with .config and .probe records:
  canon-clerk probe --json

  # Hybrid TypeSafe Jev screening/vetting + Google Gemini 4 Carbon audit probe:
  canon-clerk probe --provider typesafe --audit-model google:gemini-4-carbon

  # Override timeout and per-node models for diagnostic verification:
  canon-clerk probe --timeout 3000 --docket-model google:gemini-4-hydrogen";

/// Output presentation format for `canon-clerk probe`.
#[derive(ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProbeFormat {
    /// Human-readable endpoint reachability and latency report.
    #[default]
    Stylish,
    /// Full cumulative `Caseload` JSON envelope.
    Json,
    /// Tab-separated (`<model>\t<nodes_csv>\t<status>\t<latency_ms>ms\t<resolved_model_or_->`) one-line-per-target output.
    Compact,
}

/// Command-line arguments for `canon-clerk probe`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
#[command(
    about = "Verify live connectivity, credentials, and roundtrip latency for deduplicated model endpoints",
    long_about = "Diagnostic Network Health Check of the Caseload DAG (Branch B leaf). Executes the [configure, probe] closure by resolving active provider credentials, deduplicating `.config.nodes` into unique (model, thinking_level, reasoning_budget) targets, and probing them concurrently via lightweight single-shot pings, recording non-throwing diagnostics on `.probe`.",
    after_help = PROBE_EXAMPLES
)]
pub struct ProbeArgs {
    /// Per-target request timeout in milliseconds (default: `5000`; env: `CANON_CLERK_PROBE_TIMEOUT_MS`).
    #[arg(
        long = "timeout",
        value_name = "MS",
        help_heading = "Probe Options",
        help = "Per-target request timeout in milliseconds (default: 5000; fallback env: CANON_CLERK_PROBE_TIMEOUT_MS)"
    )]
    pub timeout: Option<u64>,

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
        default_value_t = ProbeFormat::Stylish,
        help_heading = "Caseload & Output",
        help = "Format endpoint probe diagnostic output"
    )]
    pub format: ProbeFormat,

    #[command(flatten)]
    pub io: CaseloadIoArgs,
}

/// Executes the `probe` CLI subcommand with an explicit environment map, [`EndpointProber`], and [`TelemetrySession`].
pub fn run_probe_with_telemetry(
    args: &ProbeArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    env: &BTreeMap<String, String>,
    prober: &dyn EndpointProber,
    telemetry: &TelemetrySession<'_>,
) -> Result<Caseload, CliError> {
    if args.rejected_model.is_some() {
        return Err(CliError::usage(
            "Ambiguous unqualified flag `--model` is disallowed on `canon-clerk probe`.\n  \
             Hint: Pass `--provider <slug>` or per-node `--apprise-model`, `--docket-model`, `--admit-model`, `--vet-model`, or `--audit-model`.",
        ));
    }
    let incoming_caseload = args.io.read_caseload(stdin)?;
    let timeout_ms = resolve_timeout_ms(args.timeout, env)?;
    let configure_options =
        resolve_branch_b_options(&args.provider, args.models.to_node_map(), env)?;

    let caseload = if let Some(existing) = incoming_caseload.as_ref()
        && existing.probe.is_some()
        && !configure_options.has_explicit_overrides()
    {
        existing.clone()
    } else {
        let configured = run_branch_b_configure(&configure_options, incoming_caseload, telemetry)?;
        let api_key = resolve_gemini_api_key(&configure_options).unwrap_or_default();
        let typesafe_api_key = resolve_typesafe_api_key(&configure_options).ok();
        let probe_options = ProbeOptions {
            api_key,
            typesafe_api_key,
            timeout_ms,
        };
        telemetry.run_stage(PipelineStage::Probe, |_scope| {
            execute_probe(&probe_options, prober, configured)
                .map_err(|err| CliError::usage(err.to_string()))
        })?
    };
    telemetry.finish_pipeline();

    args.io.write_output_caseload(&caseload)?;

    if let Some(probe) = &caseload.probe {
        render_probe_output(args, &caseload, probe, stdout)?;
        if !probe.is_healthy() {
            return Err(CliError::usage(format_unhealthy_probe_error(probe)));
        }
    }

    Ok(caseload)
}

/// Executes the `probe` CLI subcommand against an explicit environment map and hexagonal [`EndpointProber`].
pub fn run_probe_with_env_and_prober(
    args: &ProbeArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    env: &BTreeMap<String, String>,
    prober: &dyn EndpointProber,
) -> Result<Caseload, CliError> {
    run_probe_with_telemetry(args, stdin, stdout, env, prober, &TelemetrySession::noop())
}

/// Executes the `probe` CLI subcommand using the process environment and live [`MultiProviderEndpointProber`].
pub fn run_probe(
    args: &ProbeArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
) -> Result<Caseload, CliError> {
    let env: BTreeMap<String, String> = env::vars().collect();
    let prober = MultiProviderEndpointProber::new();
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
    let effective_format = if args.io.json {
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

fn join_nodes(nodes: &[HeuristicNode], sep: &str) -> String {
    nodes
        .iter()
        .map(|n| n.as_str())
        .collect::<Vec<_>>()
        .join(sep)
}

fn render_compact_output(probe: &CaseloadProbe, stdout: &mut dyn Write) -> std::io::Result<()> {
    for target in &probe.targets {
        write_compact_target_line(target, stdout)?;
    }
    Ok(())
}

fn write_compact_target_line(
    target: &ProbeTargetResult,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    let nodes_csv = join_nodes(&target.nodes, ",");
    let resolved = target.resolved_model.as_deref().unwrap_or("-");
    match &target.error {
        Some(err) => writeln!(
            stdout,
            "{}\t{nodes_csv}\t{}\t{}ms\t{resolved}\t{err}",
            target.model,
            target.status.as_str(),
            target.latency_ms,
        ),
        None => writeln!(
            stdout,
            "{}\t{nodes_csv}\t{}\t{}ms\t{resolved}",
            target.model,
            target.status.as_str(),
            target.latency_ms,
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
    for target in &probe.targets {
        write_stylish_target_line(target, stdout)?;
    }
    Ok(())
}

fn write_stylish_target_line(
    target: &ProbeTargetResult,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    let nodes_list = join_nodes(&target.nodes, ", ");
    match target.status {
        ProbeStatus::Ok => {
            let resolved = target
                .resolved_model
                .as_deref()
                .unwrap_or(target.model.as_str());
            writeln!(
                stdout,
                "  ✓ {} [{nodes_list}] : ok ({}ms, resolved: {resolved})",
                target.model, target.latency_ms
            )
        }
        ProbeStatus::Error => {
            let reason = target.error.as_deref().unwrap_or("unreachable");
            writeln!(
                stdout,
                "  ✗ {} [{nodes_list}] : error ({}ms) — {reason}",
                target.model, target.latency_ms
            )
        }
    }
}

fn format_unhealthy_probe_error(probe: &CaseloadProbe) -> String {
    let details: Vec<String> = probe
        .targets
        .iter()
        .filter(|t| t.status == ProbeStatus::Error)
        .map(|t| {
            let msg = t.error.as_deref().unwrap_or("Endpoint probe failed.");
            format!("{} [{}]: {msg}", t.model, join_nodes(&t.nodes, ","))
        })
        .collect();
    format!(
        "One or more model endpoint probes failed:\n  - {}",
        details.join("\n  - ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{
        CaseloadConfig, CaseloadIntake, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL,
        DEFAULT_GOOGLE_GENERATOR_MODEL, TYPESAFE_PROVIDER, ThinkingLevel,
    };
    use crate::cli::{Cli, Commands};
    use crate::configure::{ENV_GEMINI_API_KEY, ENV_TYPESAFE_API_KEY};
    use crate::probe::ProbeEndpointResult;
    use clap::Parser;
    use std::io::Cursor;

    struct MockProber {
        discriminator: ProbeEndpointResult,
        generator: ProbeEndpointResult,
    }

    impl MockProber {
        fn healthy() -> Self {
            Self {
                discriminator: ProbeEndpointResult {
                    status: ProbeStatus::Ok,
                    latency_ms: 88,
                    resolved_model: Some("gemini-decision-1".to_string()),
                    error: None,
                },
                generator: ProbeEndpointResult {
                    status: ProbeStatus::Ok,
                    latency_ms: 194,
                    resolved_model: Some("gemini-4-carbon".to_string()),
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
            _thinking_level: Option<ThinkingLevel>,
            _reasoning_budget: Option<u32>,
        ) -> ProbeEndpointResult {
            if model_specifier.contains("decision")
                || model_specifier.contains("hydrogen")
                || model_specifier.contains("flash")
                || model_specifier.contains("jev")
            {
                self.discriminator.clone()
            } else {
                self.generator.clone()
            }
        }
    }

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
        fn test_parse_probe_subcommand_flags() {
            let cli = Cli::try_parse_from([
                "canon-clerk",
                "probe",
                "--timeout",
                "3500",
                "--provider",
                "typesafe",
                "--audit-model",
                "google:gemini-2.5-pro",
                "--json",
            ])
            .expect("parse probe subcommand");

            match cli.command {
                Some(Commands::Probe(args)) => {
                    assert_eq!(args.timeout, Some(3500));
                    assert_eq!(args.provider.provider.as_deref(), Some("typesafe"));
                    assert_eq!(
                        args.models.audit_model.as_deref(),
                        Some("google:gemini-2.5-pro")
                    );
                    assert!(args.io.json);
                }
                _ => panic!("expected Probe subcommand"),
            }
        }
    }

    mod telescoping_and_output_formats {
        use super::*;

        #[test]
        fn test_telescopes_configure_and_deduplicated_probe_into_json_output_without_key_leakage() {
            let secret = "synthetic-secret-probe-key-never-echo-777";
            let args = ProbeArgs {
                provider: ProviderConfigArgs {
                    gemini_api_key: Some(secret.to_string()),
                    ..ProviderConfigArgs::default()
                },
                io: CaseloadIoArgs {
                    json: true,
                    ..CaseloadIoArgs::default()
                },
                ..ProbeArgs::default()
            };
            let mut stdout = Vec::new();
            let prober = MockProber::healthy();

            let caseload = run_probe_with_env_and_prober(
                &args,
                &mut Cursor::new(b""),
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
            assert_eq!(prb.targets.len(), 2);
            assert_eq!(
                prb.targets[0].nodes,
                vec![
                    HeuristicNode::Apprise,
                    HeuristicNode::Docket,
                    HeuristicNode::Admit,
                    HeuristicNode::Vet
                ]
            );
            assert_eq!(prb.targets[0].latency_ms, 88);
            assert_eq!(prb.targets[1].nodes, vec![HeuristicNode::Audit]);
            assert_eq!(prb.targets[1].latency_ms, 194);
        }

        #[test]
        fn test_stylish_and_compact_formats_render_deduplicated_targets_and_typesafe_standalone() {
            let prober = MockProber::healthy();
            let env = test_env();

            let stylish_args = ProbeArgs {
                format: ProbeFormat::Stylish,
                ..ProbeArgs::default()
            };
            let mut stylish_out = Vec::new();
            run_probe_with_env_and_prober(
                &stylish_args,
                &mut Cursor::new(b""),
                &mut stylish_out,
                &env,
                &prober,
            )
            .expect("stylish probe");
            let stylish_str = String::from_utf8(stylish_out).expect("utf8");
            assert!(stylish_str.contains("Endpoint Health Check (HEALTHY):"));
            assert!(
                stylish_str
                    .contains("google:gemini-decision-1 [apprise, docket, admit, vet] : ok (88ms")
            );
            assert!(stylish_str.contains("google:gemini-4-carbon [audit] : ok (194ms"));

            let compact_args = ProbeArgs {
                format: ProbeFormat::Compact,
                ..ProbeArgs::default()
            };
            let mut compact_out = Vec::new();
            run_probe_with_env_and_prober(
                &compact_args,
                &mut Cursor::new(b""),
                &mut compact_out,
                &env,
                &prober,
            )
            .expect("compact probe");
            let compact_str = String::from_utf8(compact_out).expect("utf8");
            assert!(compact_str.contains(
                "google:gemini-decision-1\tapprise,docket,admit,vet\tok\t88ms\tgemini-decision-1"
            ));
            assert!(
                compact_str.contains("google:gemini-4-carbon\taudit\tok\t194ms\tgemini-4-carbon")
            );

            // Standalone `--provider typesafe` probes 1 deduplicated target with only TYPESAFE_API_KEY
            let ts_secret = "synthetic-typesafe-probe-secret-888";
            let mut ts_env = BTreeMap::new();
            ts_env.insert(ENV_TYPESAFE_API_KEY.to_string(), ts_secret.to_string());
            ts_env.insert(
                "XDG_CONFIG_HOME".to_string(),
                "/nonexistent-canon-clerk-xdg-dir".to_string(),
            );
            let jev_prober = MockProber {
                discriminator: ProbeEndpointResult {
                    status: ProbeStatus::Ok,
                    latency_ms: 99,
                    resolved_model: Some("jev-1.14.0".to_string()),
                    error: None,
                },
                generator: MockProber::healthy().generator,
            };
            let ts_args = ProbeArgs {
                provider: ProviderConfigArgs {
                    provider: Some(TYPESAFE_PROVIDER.to_string()),
                    ..ProviderConfigArgs::default()
                },
                format: ProbeFormat::Compact,
                ..ProbeArgs::default()
            };
            let mut ts_out = Vec::new();
            let ts_caseload = run_probe_with_env_and_prober(
                &ts_args,
                &mut Cursor::new(b""),
                &mut ts_out,
                &ts_env,
                &jev_prober,
            )
            .expect("standalone --provider typesafe probe");
            let ts_rendered = String::from_utf8(ts_out).expect("utf8");
            assert_eq!(
                ts_rendered.trim(),
                "typesafe:jev-latest\tapprise,docket,admit,vet\tok\t99ms\tjev-1.14.0"
            );
            assert!(!ts_rendered.contains(ts_secret));
            assert_eq!(ts_caseload.probe.expect("probe").targets.len(), 1);
        }
    }

    mod failure_capture_and_fast_forwarding {
        use super::*;

        #[test]
        fn test_unhealthy_endpoint_emits_caseload_probe_and_exits_with_code_2() {
            let failing_prober = MockProber {
                discriminator: ProbeEndpointResult {
                    status: ProbeStatus::Ok,
                    latency_ms: 92,
                    resolved_model: Some("gemini-decision-1".to_string()),
                    error: None,
                },
                generator: ProbeEndpointResult {
                    status: ProbeStatus::Error,
                    latency_ms: 64,
                    resolved_model: None,
                    error: Some(
                        "HTTP 401: Authentication or permission rejected. Hint: Verify `GEMINI_API_KEY`."
                            .to_string(),
                    ),
                },
            };

            let args = ProbeArgs {
                io: CaseloadIoArgs {
                    json: true,
                    ..CaseloadIoArgs::default()
                },
                ..ProbeArgs::default()
            };
            let mut stdout = Vec::new();

            let err = run_probe_with_env_and_prober(
                &args,
                &mut Cursor::new(b""),
                &mut stdout,
                &test_env(),
                &failing_prober,
            )
            .expect_err("failing endpoint must return exit 2 error");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("HTTP 401"));
            assert!(err.to_string().contains("Hint:"));

            let out_str = String::from_utf8(stdout).expect("utf8 stdout");
            let parsed: Caseload = serde_json::from_str(&out_str).expect("parse caseload json");
            let probe = parsed.probe.expect("probe captured before exit 2");
            assert_eq!(probe.targets[0].status, ProbeStatus::Ok);
            assert_eq!(probe.targets[1].status, ProbeStatus::Error);
        }

        #[test]
        fn test_missing_credentials_or_zero_timeout_exits_with_code_2() {
            let prober = MockProber::healthy();
            let mut empty_env = BTreeMap::new();
            empty_env.insert(
                "XDG_CONFIG_HOME".to_string(),
                "/nonexistent-canon-clerk-xdg-dir".to_string(),
            );
            let no_key_err = run_probe_with_env_and_prober(
                &ProbeArgs::default(),
                &mut Cursor::new(b""),
                &mut Vec::new(),
                &empty_env,
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
                &mut Cursor::new(b""),
                &mut Vec::new(),
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
                    targets: vec![
                        ProbeTargetResult {
                            model: DEFAULT_GOOGLE_DISCRIMINATOR_MODEL.to_string(),
                            nodes: vec![
                                HeuristicNode::Apprise,
                                HeuristicNode::Docket,
                                HeuristicNode::Admit,
                                HeuristicNode::Vet,
                            ],
                            thinking_level: None,
                            reasoning_budget: None,
                            status: ProbeStatus::Ok,
                            latency_ms: 33,
                            resolved_model: Some("gemini-decision-1".to_string()),
                            error: None,
                        },
                        ProbeTargetResult {
                            model: DEFAULT_GOOGLE_GENERATOR_MODEL.to_string(),
                            nodes: vec![HeuristicNode::Audit],
                            thinking_level: None,
                            reasoning_budget: None,
                            status: ProbeStatus::Ok,
                            latency_ms: 77,
                            resolved_model: Some("gemini-4-carbon".to_string()),
                            error: None,
                        },
                    ],
                });
            let raw = serde_json::to_string(&upstream).expect("serialize upstream");

            let args = ProbeArgs {
                io: CaseloadIoArgs {
                    caseload: Some("-".to_string()),
                    json: true,
                    ..CaseloadIoArgs::default()
                },
                ..ProbeArgs::default()
            };
            let result = run_probe_with_env_and_prober(
                &args,
                &mut Cursor::new(raw.into_bytes()),
                &mut Vec::new(),
                &BTreeMap::new(),
                &MockProber::healthy(),
            )
            .expect("fast-forward when .probe already populated");
            assert_eq!(result, upstream);
        }
    }

    mod telemetry_events {
        use super::*;
        use crate::telemetry::{RecordingTelemetrySink, SteppingTelemetryClock, TelemetryEvent};

        #[test]
        fn test_probe_closure_emits_configure_and_probe_stage_events() {
            let sink = RecordingTelemetrySink::new();
            let clock = SteppingTelemetryClock::new(1_767_225_600_000, 10);
            let session = TelemetrySession::new(&sink, &clock);

            let args = ProbeArgs {
                io: CaseloadIoArgs {
                    json: true,
                    ..CaseloadIoArgs::default()
                },
                ..ProbeArgs::default()
            };
            let prober = MockProber::healthy();
            run_probe_with_telemetry(
                &args,
                &mut Cursor::new(b""),
                &mut Vec::new(),
                &test_env(),
                &prober,
                &session,
            )
            .expect("probe succeeds");

            let events = sink.events();
            assert_eq!(events.len(), 5);
            assert!(matches!(
                events[0],
                TelemetryEvent::StageStart {
                    stage: PipelineStage::Configure,
                    ..
                }
            ));
            assert!(matches!(
                events[1],
                TelemetryEvent::StageFinish {
                    stage: PipelineStage::Configure,
                    usage: None,
                    ..
                }
            ));
            assert!(matches!(
                events[2],
                TelemetryEvent::StageStart {
                    stage: PipelineStage::Probe,
                    ..
                }
            ));
            assert!(matches!(
                events[3],
                TelemetryEvent::StageFinish {
                    stage: PipelineStage::Probe,
                    usage: None,
                    ..
                }
            ));
            assert!(matches!(
                events[4],
                TelemetryEvent::PipelineFinish {
                    total_tokens: 0,
                    ..
                }
            ));
        }
    }

    mod single_shot_isolation {
        use super::*;
        use crate::cli::Cli;
        use crate::probe::MultiProviderEndpointProber;
        use clap::Parser;
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::thread;
        use std::time::{Duration, Instant};

        #[test]
        fn test_probe_executes_single_shot_without_sleeping_on_429_retry_after() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");
            let hits = Arc::new(AtomicUsize::new(0));
            let hits_ref = Arc::clone(&hits);

            let server = thread::spawn(move || {
                // `probe` checks 2 deduplicated default Google targets concurrently (2 single-shot calls total).
                for _ in 0..2 {
                    let (mut stream, _) = listener.accept().expect("accept probe connection");
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                    let mut buf = [0_u8; 2048];
                    let _ = stream.read(&mut buf);
                    hits_ref.fetch_add(1, Ordering::SeqCst);
                    let body = r#"{"error":{"message":"Please retry in 5s"}}"#;
                    let resp = format!(
                        "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nRetry-After: 5\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes());
                    let _ = stream.flush();
                }
            });

            let prober = MultiProviderEndpointProber::with_base_urls(
                format!("http://{addr}/v1beta"),
                format!("http://{addr}/v1"),
            );
            let args = ProbeArgs::default();
            let mut stdin = Cursor::new(Vec::<u8>::new());
            let mut stdout = Vec::<u8>::new();

            let start = Instant::now();
            let err =
                run_probe_with_env_and_prober(&args, &mut stdin, &mut stdout, &test_env(), &prober)
                    .expect_err("429 probe must return unhealthy exit 2");
            assert_eq!(err.exit_code(), 2);
            assert!(
                start.elapsed() < Duration::from_millis(800),
                "probe must never sleep for 5s Retry-After cooldown"
            );
            assert_eq!(
                hits.load(Ordering::SeqCst),
                2,
                "probe must issue exactly 1 single-shot request per deduplicated target (0 retries)"
            );
            server.join().expect("server joined");
        }

        #[test]
        fn test_probe_rejects_transport_retry_flags_as_unknown_arguments() {
            for flag in ["--max-retries", "--max-backoff-ms"] {
                let err = Cli::try_parse_from(["canon-clerk", "probe", flag, "1"])
                    .expect_err("probe must reject retry flags");
                assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
            }
        }
    }
}
