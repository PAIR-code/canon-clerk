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

//! Multi-run consistency and flipper analysis benchmark harness (`docs/architecture/benchmarking.md`).

mod cli;
mod error;
mod extract;
mod prepare;
mod render;
mod stats;
mod types;

pub use cli::{
    BenchIoStreams, ConsistencyBenchCli, ConsistencyBenchFormat, HermeticBenchDeps,
    run_consistency_bench_cli, run_consistency_bench_hermetic,
};
pub use error::{
    BenchmarkStage, ConsistencyBenchError, DEFAULT_BENCHMARK_TRIALS, ENV_BENCH_BATCH_SIZE,
    ENV_BENCH_CONCURRENCY, ENV_BENCH_FORMAT, ENV_BENCH_INCLUDE_STABLE, ENV_BENCH_MODEL,
    ENV_BENCH_OUTPUT_JSON, ENV_BENCH_STAGE, ENV_BENCH_THRESHOLD, ENV_BENCH_TRIALS,
};
use extract::extract_stage_assessments;
#[cfg(test)]
use prepare::{UpstreamCaseloadInputs, prepare_upstream_caseload};
pub use render::render_consistency_markdown;
use stats::{TrialRecord, analyze_config_trials};
pub use types::{
    ConfigConsistencyReport, ConsistencyBenchOptions, ConsistencyBenchReport,
    ConsistencyConfigSpec, SubjectConsistency, SubjectObservation, TrialSummary,
};

use crate::admit::{AdmitOptions, execute_admit};
use crate::apprise::{AppriseOptions, execute_apprise};
use crate::audit::{AuditOptions, execute_audit};
use crate::caseload::{
    Caseload, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, DEFAULT_GOOGLE_GENERATOR_MODEL, HeuristicNode,
    normalize_unit_score,
};
use crate::discover::WorkspaceSnapshot;
use crate::docket::{DocketOptions, execute_docket};
use crate::provider::{StructuredModelProvider, TokenUsage};
use crate::telemetry::{
    PipelineStage, RecordingTelemetrySink, SystemTelemetryClock, TelemetryClock, TelemetryEvent,
    TelemetrySession,
};
use crate::vet::{VetOptions, execute_vet};
use prepare::clear_stage_and_downstream;

/// Executes `N` repeated trials per configuration against `caseload` using wall-clock telemetry.
pub fn execute_consistency_bench(
    options: &ConsistencyBenchOptions,
    caseload: &Caseload,
    workspace: &WorkspaceSnapshot,
    provider: &dyn StructuredModelProvider,
) -> Result<ConsistencyBenchReport, ConsistencyBenchError> {
    let clock = SystemTelemetryClock::new();
    let mut noop_observer = |_label: &str, _summary: &TrialSummary| {};
    execute_consistency_bench_observed(
        options,
        caseload,
        workspace,
        provider,
        &clock,
        &mut noop_observer,
    )
}

/// Executes `N` repeated trials per configuration with an explicit [`TelemetryClock`] and per-trial callback.
pub fn execute_consistency_bench_observed(
    options: &ConsistencyBenchOptions,
    caseload: &Caseload,
    workspace: &WorkspaceSnapshot,
    provider: &dyn StructuredModelProvider,
    clock: &dyn TelemetryClock,
    on_trial: &mut dyn FnMut(&str, &TrialSummary),
) -> Result<ConsistencyBenchReport, ConsistencyBenchError> {
    validate_bench_options(options)?;

    let candidate_canon_count = caseload
        .discovery
        .as_ref()
        .map_or(0, |d| d.candidate_canons.len());
    let model = resolved_model_for_stage(options.stage, caseload);
    let mut base_caseload = clear_stage_and_downstream(options.stage, caseload.clone());
    let needs_upstream_evidence =
        matches!(options.stage, BenchmarkStage::Vet | BenchmarkStage::Audit);
    if matches!(
        options.stage,
        BenchmarkStage::Admit | BenchmarkStage::Vet | BenchmarkStage::Audit
    ) && base_caseload.docket.is_none()
        && !(needs_upstream_evidence && base_caseload.evidence.is_some())
    {
        let docket_opts = DocketOptions::default();
        base_caseload = execute_docket(&docket_opts, base_caseload, workspace, provider)
            .map_err(ConsistencyBenchError::Docket)?;
    }
    if needs_upstream_evidence && base_caseload.evidence.is_none() {
        base_caseload = execute_admit(&AdmitOptions::default(), base_caseload, workspace, provider)
            .map_err(ConsistencyBenchError::Admit)?;
    }

    let mut config_reports = Vec::with_capacity(options.configs.len());
    for spec in &options.configs {
        let mut records = Vec::with_capacity(options.trials);
        for trial_idx in 1..=options.trials {
            let record = execute_single_trial(
                options.stage,
                spec,
                trial_idx,
                &base_caseload,
                workspace,
                provider,
                clock,
            )?;
            let positive_count = record
                .assessments
                .values()
                .filter(|a| a.is_positive)
                .count();
            let subject_count = record.assessments.len();
            let summary = TrialSummary {
                trial: trial_idx,
                subject_count,
                positive_count,
                negative_count: subject_count.saturating_sub(positive_count),
                duration_ms: record.duration_ms,
                prompt_tokens: record.usage.prompt_tokens,
                completion_tokens: record.usage.completion_tokens,
                reasoning_tokens: record.usage.reasoning_tokens,
                total_tokens: record.usage.total_tokens,
            };
            on_trial(&spec.label, &summary);
            records.push(record);
        }
        config_reports.push(analyze_config_trials(spec, options.stage, &model, &records));
    }

    Ok(ConsistencyBenchReport {
        stage: options.stage,
        trials: options.trials,
        candidate_canon_count,
        configs: config_reports,
    })
}

fn validate_bench_options(options: &ConsistencyBenchOptions) -> Result<(), ConsistencyBenchError> {
    if options.trials == 0 {
        return Err(ConsistencyBenchError::InvalidTrials);
    }
    if options.configs.is_empty() {
        return Err(ConsistencyBenchError::EmptyConfigs);
    }
    for spec in &options.configs {
        normalize_unit_score(spec.threshold)
            .map_err(|detail| ConsistencyBenchError::InvalidThreshold { detail })?;
        if spec.max_canons_per_call == Some(0) {
            return Err(ConsistencyBenchError::InvalidBatchSpec {
                detail: "max_canons_per_call must be at least 1, got 0".to_string(),
            });
        }
        if spec.max_concurrent_calls == Some(0) {
            return Err(ConsistencyBenchError::InvalidConcurrency {
                detail: "max_concurrent_calls must be at least 1, got 0".to_string(),
            });
        }
    }
    Ok(())
}

fn resolved_model_for_stage(stage: BenchmarkStage, caseload: &Caseload) -> String {
    let node = stage.heuristic_node();
    caseload
        .config
        .as_ref()
        .and_then(|c| c.model_for(node))
        .map(ToString::to_string)
        .unwrap_or_else(|| match node {
            HeuristicNode::Audit => DEFAULT_GOOGLE_GENERATOR_MODEL.to_string(),
            HeuristicNode::Apprise
            | HeuristicNode::Docket
            | HeuristicNode::Admit
            | HeuristicNode::Vet => DEFAULT_GOOGLE_DISCRIMINATOR_MODEL.to_string(),
        })
}

#[allow(
    clippy::too_many_arguments,
    reason = "Internal single-trial helper binding stage, config spec, trial index, base caseload, workspace, provider, and clock"
)]
fn execute_single_trial(
    stage: BenchmarkStage,
    spec: &ConsistencyConfigSpec,
    trial: usize,
    base_caseload: &Caseload,
    workspace: &WorkspaceSnapshot,
    provider: &dyn StructuredModelProvider,
    clock: &dyn TelemetryClock,
) -> Result<TrialRecord, ConsistencyBenchError> {
    let sink = RecordingTelemetrySink::new();
    let session = TelemetrySession::new(&sink, clock);

    let enriched = match stage {
        BenchmarkStage::Apprise => {
            let opts = AppriseOptions {
                threshold: spec.threshold,
                timeout_ms: spec.timeout_ms,
                max_canons_per_call: spec.max_canons_per_call,
                max_concurrent_calls: spec.max_concurrent_calls,
            };
            session
                .run_stage(PipelineStage::Apprise, |scope| {
                    let instrumented = scope.instrument_provider(provider);
                    execute_apprise(&opts, base_caseload.clone(), workspace, &instrumented)
                })
                .map_err(ConsistencyBenchError::Apprise)?
        }
        BenchmarkStage::Docket => {
            let opts = DocketOptions {
                threshold: spec.threshold,
                timeout_ms: spec.timeout_ms,
                max_canons_per_call: spec.max_canons_per_call,
                max_concurrent_calls: spec.max_concurrent_calls,
            };
            session
                .run_stage(PipelineStage::Docket, |scope| {
                    let instrumented = scope.instrument_provider(provider);
                    execute_docket(&opts, base_caseload.clone(), workspace, &instrumented)
                })
                .map_err(ConsistencyBenchError::Docket)?
        }
        BenchmarkStage::Admit => {
            let opts = AdmitOptions {
                threshold: spec.threshold,
                timeout_ms: spec.timeout_ms,
                max_concurrent_calls: spec.max_concurrent_calls,
                ..AdmitOptions::default()
            };
            session
                .run_stage(PipelineStage::Admit, |scope| {
                    let instrumented = scope.instrument_provider(provider);
                    execute_admit(&opts, base_caseload.clone(), workspace, &instrumented)
                })
                .map_err(ConsistencyBenchError::Admit)?
        }
        BenchmarkStage::Vet => {
            let opts = VetOptions {
                threshold: spec.threshold,
                timeout_ms: spec.timeout_ms,
                max_concurrent_calls: spec.max_concurrent_calls,
                ..VetOptions::default()
            };
            session
                .run_stage(PipelineStage::Vet, |scope| {
                    let instrumented = scope.instrument_provider(provider);
                    execute_vet(&opts, base_caseload.clone(), workspace, &instrumented)
                })
                .map_err(ConsistencyBenchError::Vet)?
        }
        BenchmarkStage::Audit => {
            let opts = AuditOptions {
                threshold: spec.threshold,
                timeout_ms: spec.timeout_ms,
                max_concurrent_calls: spec.max_concurrent_calls,
                ..AuditOptions::default()
            };
            session
                .run_stage(PipelineStage::Audit, |scope| {
                    let instrumented = scope.instrument_provider(provider);
                    execute_audit(&opts, base_caseload.clone(), workspace, &instrumented)
                })
                .map_err(ConsistencyBenchError::Audit)?
        }
    };

    let (duration_ms, usage) = extract_stage_finish_metrics(&sink, stage.pipeline_stage());
    let assessments = extract_stage_assessments(stage, &enriched, spec.threshold)?;

    Ok(TrialRecord {
        trial,
        duration_ms,
        usage,
        assessments,
    })
}

fn extract_stage_finish_metrics(
    sink: &RecordingTelemetrySink,
    expected_stage: PipelineStage,
) -> (u64, TokenUsage) {
    for event in sink.events() {
        if let TelemetryEvent::StageFinish {
            stage,
            duration_ms,
            usage,
        } = event
            && stage == expected_stage
        {
            return (duration_ms, usage.unwrap_or_default());
        }
    }
    (0, TokenUsage::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::cli::CanonSelectionArgs;
    use crate::configure::ConfigureOptions;
    use crate::docket::DEFAULT_DOCKET_THRESHOLD;
    use crate::intake::IntakeOptions;
    use crate::provider::{
        DEFAULT_GENERATION_TIMEOUT_MS, DecisionRequest, DecisionResponse, GenerationRequest,
        GenerationResponse, ProviderError,
    };
    use crate::telemetry::SteppingTelemetryClock;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct ScriptedDocketProvider {
        turn: AtomicUsize,
    }

    impl ScriptedDocketProvider {
        fn new() -> Self {
            Self {
                turn: AtomicUsize::new(0),
            }
        }
    }

    impl StructuredModelProvider for ScriptedDocketProvider {
        fn generate_structured(
            &self,
            _request: &GenerationRequest,
        ) -> Result<GenerationResponse, ProviderError> {
            panic!("docket benchmark must not invoke generate_structured")
        }

        fn evaluate_decisions(
            &self,
            _request: &DecisionRequest,
        ) -> Result<DecisionResponse, ProviderError> {
            let idx = self.turn.fetch_add(1, Ordering::SeqCst);
            let flip_score = if idx == 2 { 0.20 } else { 0.88 };
            let scores = BTreeMap::from([
                (".canons/cli/flipper.md".to_string(), flip_score),
                (".canons/cli/steady.md".to_string(), 0.05),
            ]);
            Ok(DecisionResponse {
                scores,
                resolved_model: "gemini-decision-1".to_string(),
                usage: TokenUsage {
                    prompt_tokens: 200,
                    completion_tokens: 0,
                    reasoning_tokens: 0,
                    total_tokens: 200,
                },
                latency_ms: 110,
            })
        }
    }

    fn docket_fixture() -> (Caseload, WorkspaceSnapshot) {
        let ws = WorkspaceSnapshot::new()
            .with_file("src/cli/mod.rs")
            .with_canon_markdown(
                "---\ntriggers:\n  - \"src/cli/**\"\ntags:\n  - cli\n---\nCLI flags MUST use kebab-case.\n\nRationale: POSIX consistency.\n",
                &ParseCanonOptions::with_source_ref(".canons/cli/flipper.md"),
            )
            .with_canon_markdown(
                "---\ntriggers:\n  - \"src/cli/**\"\ntags:\n  - cli\n---\nCLI errors MUST suggest remediations.\n\nRationale: Actionable recovery.\n",
                &ParseCanonOptions::with_source_ref(".canons/cli/steady.md"),
            );
        let intake_opts = IntakeOptions {
            target_paths: Some(vec!["src/cli/mod.rs".to_string()]),
            ..IntakeOptions::default()
        };
        let cfg_opts = ConfigureOptions {
            gemini_api_key: Some("test-key".to_string()),
            ..ConfigureOptions::default()
        }
        .with_node_model(HeuristicNode::Docket, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL)
        .with_node_model(HeuristicNode::Admit, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL);
        let caseload = prepare_upstream_caseload(&UpstreamCaseloadInputs {
            stage: BenchmarkStage::Docket,
            intake_opts: &intake_opts,
            canons: &CanonSelectionArgs::default(),
            configure_opts: &cfg_opts,
            incoming: None,
            workspace: &ws,
        })
        .expect("prepare docket caseload");
        (caseload, ws)
    }

    mod docket_multi_trial_execution {
        use super::*;

        #[test]
        fn test_executes_five_docket_trials_and_captures_flippers_and_telemetry() {
            let (caseload, ws) = docket_fixture();
            let provider = ScriptedDocketProvider::new();
            let clock = SteppingTelemetryClock::new(1_767_225_600_000, 125);
            let options = ConsistencyBenchOptions {
                stage: BenchmarkStage::Docket,
                trials: DEFAULT_BENCHMARK_TRIALS,
                configs: vec![ConsistencyConfigSpec {
                    label: "unbounded".to_string(),
                    threshold: DEFAULT_DOCKET_THRESHOLD,
                    max_canons_per_call: None,
                    max_concurrent_calls: None,
                    timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
                }],
            };

            let mut observed_trials = Vec::new();
            let report = execute_consistency_bench_observed(
                &options,
                &caseload,
                &ws,
                &provider,
                &clock,
                &mut |label, summary| observed_trials.push((label.to_string(), summary.trial)),
            )
            .expect("execute docket consistency bench");

            assert_eq!(observed_trials.len(), 5);
            assert_eq!(report.candidate_canon_count, 2);
            let cfg = &report.configs[0];
            assert_eq!(cfg.positive_counts, vec![1, 1, 0, 1, 1]);
            assert_eq!(cfg.flipper_count, 1);
            assert_eq!(cfg.total_tokens, 1000);
            assert_eq!(cfg.latency_median_ms, 125);

            let flippers = cfg.flippers();
            assert_eq!(flippers.len(), 1);
            assert_eq!(flippers[0].canon_path, ".canons/cli/flipper.md");
            assert_eq!(flippers[0].positive_count, 4);
            assert_eq!(flippers[0].negative_count, 1);
            assert!((flippers[0].agreement_rate - 0.8).abs() < 1e-9);
        }
    }

    mod validation_and_error_handling {
        use super::*;

        #[test]
        fn test_rejects_zero_trials_empty_configs_and_invalid_threshold() {
            let (caseload, ws) = docket_fixture();
            let provider = ScriptedDocketProvider::new();

            let err_zero = execute_consistency_bench(
                &ConsistencyBenchOptions {
                    trials: 0,
                    ..ConsistencyBenchOptions::default()
                },
                &caseload,
                &ws,
                &provider,
            )
            .expect_err("0 trials must fail");
            assert_eq!(err_zero.exit_code(), 2);

            let err_empty = execute_consistency_bench(
                &ConsistencyBenchOptions {
                    configs: Vec::new(),
                    ..ConsistencyBenchOptions::default()
                },
                &caseload,
                &ws,
                &provider,
            )
            .expect_err("empty configs must fail");
            assert_eq!(err_empty.exit_code(), 2);

            let err_thresh = execute_consistency_bench(
                &ConsistencyBenchOptions {
                    configs: vec![ConsistencyConfigSpec {
                        threshold: 1.5,
                        ..ConsistencyConfigSpec::default()
                    }],
                    ..ConsistencyBenchOptions::default()
                },
                &caseload,
                &ws,
                &provider,
            )
            .expect_err("out-of-bounds threshold must fail");
            assert_eq!(err_thresh.exit_code(), 2);
        }
    }

    mod audit_multi_trial_execution {
        use super::*;
        use crate::audit::DEFAULT_AUDIT_THRESHOLD;

        struct ScriptedAuditProvider {
            docket_calls: AtomicUsize,
            admit_calls: AtomicUsize,
            audit_steady_calls: AtomicUsize,
            audit_flip_calls: AtomicUsize,
        }

        impl ScriptedAuditProvider {
            fn new() -> Self {
                Self {
                    docket_calls: AtomicUsize::new(0),
                    admit_calls: AtomicUsize::new(0),
                    audit_steady_calls: AtomicUsize::new(0),
                    audit_flip_calls: AtomicUsize::new(0),
                }
            }
        }

        impl StructuredModelProvider for ScriptedAuditProvider {
            fn generate_structured(
                &self,
                request: &GenerationRequest,
            ) -> Result<GenerationResponse, ProviderError> {
                let payload = if request.user_prompt.contains(".canons/cli/steady.md") {
                    // Steady `fail` across all 3 trials, even though remediation prose and line spans vary!
                    let turn = self.audit_steady_calls.fetch_add(1, Ordering::SeqCst);
                    json!({
                        "compliance_summary": format!("Remediation wording variant #{turn}: add a Hint line."),
                        "compliance_score": 0.08 + (turn as f64) * 0.03,
                        "annotations": [{
                            "path": "src/cli/mod.rs",
                            "start_line": 10 + turn,
                            "end_line": 12 + turn * 2,
                            "annotation_level": "failure",
                            "message": format!("Line-span variant #{turn}")
                        }]
                    })
                } else {
                    let turn = self.audit_flip_calls.fetch_add(1, Ordering::SeqCst);
                    let score = if turn == 1 { 0.88 } else { 0.12 };
                    json!({
                        "compliance_summary": format!("Flipper trial {turn}."),
                        "compliance_score": score,
                        "annotations": []
                    })
                };

                Ok(GenerationResponse {
                    payload,
                    resolved_model: "gemini-2.5-pro".to_string(),
                    thoughts: Vec::new(),
                    usage: TokenUsage {
                        prompt_tokens: 300,
                        completion_tokens: 70,
                        reasoning_tokens: 130,
                        total_tokens: 500,
                    },
                    latency_ms: 210,
                })
            }

            fn evaluate_decisions(
                &self,
                request: &DecisionRequest,
            ) -> Result<DecisionResponse, ProviderError> {
                let mut scores = BTreeMap::new();
                if request.questions.keys().any(|k| k.contains("::")) {
                    self.admit_calls.fetch_add(1, Ordering::SeqCst);
                    for key in request.questions.keys() {
                        scores.insert(key.clone(), 0.94);
                    }
                } else {
                    self.docket_calls.fetch_add(1, Ordering::SeqCst);
                    for key in request.questions.keys() {
                        scores.insert(key.clone(), 0.92);
                    }
                }
                Ok(DecisionResponse {
                    scores,
                    resolved_model: "gemini-decision-1".to_string(),
                    usage: TokenUsage {
                        prompt_tokens: 150,
                        completion_tokens: 0,
                        reasoning_tokens: 0,
                        total_tokens: 150,
                    },
                    latency_ms: 60,
                })
            }
        }

        #[test]
        fn test_audit_bench_prepares_upstream_once_and_ignores_prose_and_span_variance_on_stable_verdicts()
         {
            let (caseload, ws) = docket_fixture();
            let provider = ScriptedAuditProvider::new();
            let report = execute_consistency_bench(
                &ConsistencyBenchOptions {
                    stage: BenchmarkStage::Audit,
                    trials: 3,
                    configs: vec![ConsistencyConfigSpec {
                        label: "unbounded".to_string(),
                        threshold: DEFAULT_AUDIT_THRESHOLD,
                        max_canons_per_call: None,
                        max_concurrent_calls: None,
                        timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
                    }],
                },
                &caseload,
                &ws,
                &provider,
            )
            .expect("run 3-trial audit bench");

            assert_eq!(provider.docket_calls.load(Ordering::SeqCst), 1);
            assert_eq!(provider.admit_calls.load(Ordering::SeqCst), 1);
            assert_eq!(provider.audit_steady_calls.load(Ordering::SeqCst), 3);
            assert_eq!(provider.audit_flip_calls.load(Ordering::SeqCst), 3);

            let cfg = &report.configs[0];
            assert_eq!(cfg.subject_count, 2);
            assert_eq!(
                cfg.flipper_count, 1,
                "only 1D pass/fail verdict flip counts as a flipper; varying prose/spans on steady fail must produce 0 flippers"
            );
            assert_eq!(cfg.flippers()[0].canon_path, ".canons/cli/flipper.md");

            let steady = cfg
                .subjects
                .iter()
                .find(|s| s.canon_path == ".canons/cli/steady.md")
                .expect("steady subject");
            assert!(!steady.is_flipper);
            assert_eq!(steady.agreement_rate, 1.0);
            assert_eq!(steady.negative_count, 3);
        }
    }

    mod admit_multi_trial_execution {
        use super::*;
        use crate::admit::DEFAULT_ADMIT_THRESHOLD;

        struct ScriptedAdmitProvider {
            docket_calls: AtomicUsize,
            admit_calls: AtomicUsize,
        }

        impl ScriptedAdmitProvider {
            fn new() -> Self {
                Self {
                    docket_calls: AtomicUsize::new(0),
                    admit_calls: AtomicUsize::new(0),
                }
            }
        }

        impl StructuredModelProvider for ScriptedAdmitProvider {
            fn generate_structured(
                &self,
                _request: &GenerationRequest,
            ) -> Result<GenerationResponse, ProviderError> {
                panic!("admit benchmark must not invoke generate_structured")
            }

            fn evaluate_decisions(
                &self,
                request: &DecisionRequest,
            ) -> Result<DecisionResponse, ProviderError> {
                let mut scores = BTreeMap::new();
                if request.questions.keys().any(|k| k.contains("::")) {
                    let idx = self.admit_calls.fetch_add(1, Ordering::SeqCst);
                    let score = if idx == 1 { 0.30 } else { 0.85 };
                    for key in request.questions.keys() {
                        scores.insert(key.clone(), score);
                    }
                } else {
                    self.docket_calls.fetch_add(1, Ordering::SeqCst);
                    scores.insert(".canons/cli/flipper.md".to_string(), 0.90);
                    scores.insert(".canons/cli/steady.md".to_string(), 0.05);
                }
                Ok(DecisionResponse {
                    scores,
                    resolved_model: "gemini-decision-1".to_string(),
                    usage: TokenUsage {
                        prompt_tokens: 150,
                        completion_tokens: 0,
                        reasoning_tokens: 0,
                        total_tokens: 150,
                    },
                    latency_ms: 80,
                })
            }
        }

        #[test]
        fn test_executes_admit_trials_against_single_prepared_docket_and_tracks_2d_flippers() {
            let (caseload, ws) = docket_fixture();
            let provider = ScriptedAdmitProvider::new();
            let report = execute_consistency_bench(
                &ConsistencyBenchOptions {
                    stage: BenchmarkStage::Admit,
                    trials: 3,
                    configs: vec![ConsistencyConfigSpec {
                        label: "unbounded".to_string(),
                        threshold: DEFAULT_ADMIT_THRESHOLD,
                        max_canons_per_call: None,
                        max_concurrent_calls: None,
                        timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
                    }],
                },
                &caseload,
                &ws,
                &provider,
            )
            .expect("run 3-trial admit bench");

            assert_eq!(provider.docket_calls.load(Ordering::SeqCst), 1);
            assert_eq!(provider.admit_calls.load(Ordering::SeqCst), 3);
            let cfg = &report.configs[0];
            assert_eq!(cfg.flipper_count, 1);
            assert_eq!(
                cfg.flippers()[0].exhibit_path.as_deref(),
                Some("src/cli/mod.rs")
            );
            assert_eq!(cfg.flippers()[0].positive_count, 2);
            assert_eq!(cfg.flippers()[0].negative_count, 1);
        }
    }

    mod report_json_round_trip {
        use super::*;

        #[test]
        fn test_report_serializes_clean_json_omitting_none_exhibit_path() {
            let (caseload, ws) = docket_fixture();
            let provider = ScriptedDocketProvider::new();
            let report = execute_consistency_bench(
                &ConsistencyBenchOptions {
                    stage: BenchmarkStage::Docket,
                    trials: 2,
                    configs: vec![ConsistencyConfigSpec::default()],
                },
                &caseload,
                &ws,
                &provider,
            )
            .expect("run 2-trial docket bench");

            let encoded = serde_json::to_string(&report).expect("serialize report");
            assert!(
                !encoded.contains("exhibit_path"),
                "1D canon subjects must omit `exhibit_path` on wire"
            );
            let decoded: ConsistencyBenchReport =
                serde_json::from_str(&encoded).expect("deserialize report");
            assert_eq!(decoded, report);
        }
    }
}
