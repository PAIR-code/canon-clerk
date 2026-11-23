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

//! Guarded Branch A, JIT Branch B, and `docket` → `admit` → `vet` → `audit` execution steps for `canon-clerk audit`.

use super::finalize::{fold_vetted_to_verdict, try_pre_populated_short_circuit};
use super::guards::{
    has_admit_cli_overrides, has_docket_cli_overrides, has_vet_cli_overrides,
    resolve_audit_concurrency,
};
use super::{AuditArgs, OnMissingCredentials};
use crate::admit::{AdmitOptions, execute_admit};
use crate::audit::{AuditOptions, DEFAULT_AUDIT_TIMEOUT_MS, execute_audit};
use crate::caseload::{Caseload, CaseloadEvidence, CaseloadVerdict, HeuristicNode};
use crate::cli::CliError;
use crate::cli::admit::guards::resolve_admit_concurrency;
use crate::cli::closure::ClosureBackend;
use crate::cli::docket::guards::{resolve_docket_batch_size, resolve_docket_concurrency};
use crate::cli::provenance::StageDisposition;
use crate::cli::runners::{resolve_closure_branch_b_options, run_branch_b_configure_checked};
use crate::cli::vet::guards::resolve_vet_concurrency;
use crate::cli::vet::stages::{
    materialize_base_workspace_references, populate_live_case_and_reference_snapshots,
    populate_live_target_snapshots,
};
use crate::configure::{
    ConfigureError, ConfigureScope, StoredConfig, can_fast_forward_existing_config,
    is_discriminative_model, resolve_provider_connections,
};
use crate::discover::WorkspaceSnapshot;
use crate::docket::{DocketOptions, effective_docket_batch_size, execute_docket};
use crate::provider::{DEFAULT_GENERATION_TIMEOUT_MS, MultiModelProvider, StructuredModelProvider};
use crate::telemetry::{PipelineStage, TelemetrySession};
use crate::vet::{VetOptions, execute_vet};
use std::collections::BTreeMap;

pub(super) enum AuditCascadeResult {
    Completed(AuditCascadeOutcome),
    SkippedMissingCredentials {
        caseload: Caseload,
        missing_provider_slug: String,
    },
}

pub(super) struct AuditCascadeOutcome {
    pub(super) caseload: Caseload,
    pub(super) configure_disp: StageDisposition,
    pub(super) docket_disp: StageDisposition,
    pub(super) admit_disp: StageDisposition,
    pub(super) vet_disp: StageDisposition,
    pub(super) audit_disp: StageDisposition,
}

#[derive(Clone, Copy)]
pub(crate) struct AuditCascadeThresholds {
    pub(crate) docket_threshold: f64,
    pub(crate) admit_threshold: f64,
    pub(crate) vet_threshold: f64,
    pub(crate) audit_threshold: f64,
}

struct PrePopulatedFlags {
    docket: bool,
    evidence: bool,
    vetting: bool,
}

#[allow(
    clippy::too_many_arguments,
    reason = "Internal stage helper threading CLI args, env, stored config, thresholds, caseload, workspace, backend, and telemetry"
)]
pub(super) fn execute_jit_configure_docket_admit_and_audit(
    args: &AuditArgs,
    env: &BTreeMap<String, String>,
    stored_config: Option<StoredConfig>,
    thresholds: AuditCascadeThresholds,
    mut caseload: Caseload,
    snapshot: &mut WorkspaceSnapshot,
    backend: &ClosureBackend<'_>,
    telemetry: &TelemetrySession<'_>,
) -> Result<AuditCascadeResult, CliError> {
    let provider_changed = args.provider.provider.is_some();
    let docket_pre_populated =
        caseload.docket.is_some() && !has_docket_cli_overrides(args) && !provider_changed;
    let evidence_pre_populated = caseload.evidence.is_some()
        && !has_admit_cli_overrides(args)
        && !has_docket_cli_overrides(args)
        && !provider_changed;
    let vetting_pre_populated =
        caseload.vetting.is_some() && evidence_pre_populated && !has_vet_cli_overrides(args);

    if let Some(short_circuit) = try_pre_populated_short_circuit(
        &caseload,
        docket_pre_populated,
        evidence_pre_populated,
        vetting_pre_populated,
        thresholds.audit_threshold,
    ) {
        return Ok(AuditCascadeResult::Completed(short_circuit));
    }

    materialize_base_workspace_references(&caseload, snapshot, backend)?;

    let required_nodes = compute_required_nodes(
        docket_pre_populated,
        evidence_pre_populated,
        vetting_pre_populated,
    );

    let configure_options = resolve_closure_branch_b_options(
        &args.provider,
        args.models.resolve_node_models()?,
        ConfigureScope::closure(required_nodes),
        stored_config,
        env,
    )?;
    let stored = configure_options.stored_config.as_ref();
    let stage_opts = build_stage_options(args, env, stored, thresholds)?;
    let retry_policy = args.transport.resolve_retry_policy(env, stored)?;

    let config_was_present =
        can_fast_forward_existing_config(&configure_options, caseload.config.as_ref());
    caseload.verdict = None;

    let configured = match run_configure_or_skip(
        &configure_options,
        caseload,
        args.on_missing_credentials,
        telemetry,
    )? {
        Ok(c) => c,
        Err(skipped) => return Ok(skipped),
    };
    let cfg_snapshot = configured.config.clone().unwrap_or_default();
    let docket_opts = stage_opts.docket_opts(&cfg_snapshot, thresholds.docket_threshold);
    let live_instance;
    let active_provider: &dyn StructuredModelProvider = match backend {
        ClosureBackend::Hermetic { provider, .. } => *provider,
        ClosureBackend::LiveDisk { .. } => match build_live_provider_or_skip(
            &configure_options,
            &cfg_snapshot,
            retry_policy,
            args.on_missing_credentials,
        )? {
            Ok(instance) => {
                live_instance = instance;
                &live_instance
            }
            Err(missing_provider_slug) => {
                return Ok(AuditCascadeResult::SkippedMissingCredentials {
                    caseload: configured,
                    missing_provider_slug,
                });
            }
        },
    };
    let configure_disp = if config_was_present {
        StageDisposition::PrePopulated
    } else {
        StageDisposition::Executed
    };

    execute_configured_spine(
        configured,
        configure_disp,
        PrePopulatedFlags {
            docket: docket_pre_populated,
            evidence: evidence_pre_populated,
            vetting: vetting_pre_populated,
        },
        (
            &docket_opts,
            &stage_opts.admit_opts,
            &stage_opts.vet_opts,
            &stage_opts.audit_opts,
        ),
        snapshot,
        backend,
        active_provider,
        telemetry,
    )
    .map(AuditCascadeResult::Completed)
}

struct ResolvedStageOptions {
    explicit_docket_batch_size: Option<usize>,
    docket_concurrency: Option<usize>,
    admit_opts: AdmitOptions,
    vet_opts: VetOptions,
    audit_opts: AuditOptions,
}

impl ResolvedStageOptions {
    fn docket_opts(
        &self,
        cfg_snapshot: &crate::caseload::CaseloadConfig,
        docket_threshold: f64,
    ) -> DocketOptions {
        let docket_model = cfg_snapshot
            .model_for(HeuristicNode::Docket)
            .unwrap_or_default();
        DocketOptions {
            threshold: docket_threshold,
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
            max_canons_per_call: effective_docket_batch_size(
                self.explicit_docket_batch_size,
                docket_model,
            ),
            max_concurrent_calls: self.docket_concurrency,
        }
    }
}

fn compute_required_nodes(
    docket_pre_populated: bool,
    evidence_pre_populated: bool,
    vetting_pre_populated: bool,
) -> Vec<HeuristicNode> {
    let mut nodes = Vec::new();
    if !docket_pre_populated && !evidence_pre_populated {
        nodes.push(HeuristicNode::Docket);
    }
    if !evidence_pre_populated {
        nodes.push(HeuristicNode::Admit);
    }
    if !vetting_pre_populated {
        nodes.push(HeuristicNode::Vet);
    }
    nodes.push(HeuristicNode::Audit);
    nodes
}

fn run_configure_or_skip(
    configure_options: &crate::configure::ConfigureOptions,
    caseload: Caseload,
    on_missing: OnMissingCredentials,
    telemetry: &TelemetrySession<'_>,
) -> Result<Result<Caseload, AuditCascadeResult>, CliError> {
    match run_branch_b_configure_checked(configure_options, Some(caseload.clone()), telemetry) {
        Ok(c) => Ok(Ok(c)),
        Err(ConfigureError::MissingProviderApiKey { slug })
            if on_missing == OnMissingCredentials::Skip =>
        {
            Ok(Err(AuditCascadeResult::SkippedMissingCredentials {
                caseload,
                missing_provider_slug: slug,
            }))
        }
        Err(err) => Err(CliError::usage(err.to_string())),
    }
}

fn build_live_provider_or_skip(
    configure_options: &crate::configure::ConfigureOptions,
    cfg_snapshot: &crate::caseload::CaseloadConfig,
    retry_policy: crate::provider::RetryPolicy,
    on_missing: OnMissingCredentials,
) -> Result<Result<MultiModelProvider, String>, CliError> {
    match resolve_provider_connections(configure_options, cfg_snapshot) {
        Ok(connections) => Ok(Ok(MultiModelProvider::from_connections(
            connections,
            retry_policy,
        ))),
        Err(ConfigureError::MissingProviderApiKey { slug })
            if on_missing == OnMissingCredentials::Skip =>
        {
            Ok(Err(slug))
        }
        Err(err) => Err(CliError::usage(err.to_string())),
    }
}

fn build_stage_options(
    args: &AuditArgs,
    env: &BTreeMap<String, String>,
    stored: Option<&StoredConfig>,
    thresholds: AuditCascadeThresholds,
) -> Result<ResolvedStageOptions, CliError> {
    Ok(ResolvedStageOptions {
        explicit_docket_batch_size: resolve_docket_batch_size(args.docket_batch_size, env, stored)?,
        docket_concurrency: resolve_docket_concurrency(args.docket_concurrency, env, stored)?,
        admit_opts: AdmitOptions {
            threshold: thresholds.admit_threshold,
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
            max_concurrent_calls: resolve_admit_concurrency(args.admit_concurrency, env, stored)?,
        },
        vet_opts: VetOptions {
            threshold: thresholds.vet_threshold,
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
            max_concurrent_calls: resolve_vet_concurrency(args.vet_concurrency, env, stored)?,
            ..VetOptions::default()
        },
        audit_opts: AuditOptions {
            threshold: thresholds.audit_threshold,
            timeout_ms: DEFAULT_AUDIT_TIMEOUT_MS,
            max_concurrent_calls: resolve_audit_concurrency(args.audit_concurrency, env, stored)?,
        },
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "Internal spine runner threading pre-resolved stage options, workspace snapshot, backend, provider, and telemetry"
)]
fn execute_configured_spine(
    configured: Caseload,
    configure_disp: StageDisposition,
    pre: PrePopulatedFlags,
    stage_opts: (&DocketOptions, &AdmitOptions, &VetOptions, &AuditOptions),
    snapshot: &mut WorkspaceSnapshot,
    backend: &ClosureBackend<'_>,
    provider: &dyn StructuredModelProvider,
    telemetry: &TelemetrySession<'_>,
) -> Result<AuditCascadeOutcome, CliError> {
    let (docket_opts, admit_opts, vet_opts, audit_opts) = stage_opts;

    let (after_docket, docket_disp) = execute_docket_step(
        configured,
        pre.evidence || pre.docket,
        docket_opts,
        snapshot,
        provider,
        telemetry,
    )?;

    if !pre.evidence
        && after_docket
            .docket
            .as_ref()
            .is_some_and(|d| d.active_docket.is_empty())
    {
        return Ok(AuditCascadeOutcome {
            caseload: after_docket
                .with_evidence(CaseloadEvidence::default())
                .with_verdict(CaseloadVerdict::default()),
            configure_disp,
            docket_disp,
            admit_disp: StageDisposition::SkippedEmptyDocket,
            vet_disp: StageDisposition::SkippedEmptyDocket,
            audit_disp: StageDisposition::SkippedEmptyDocket,
        });
    }

    let (after_admit, admit_disp) = if pre.evidence {
        (after_docket, StageDisposition::PrePopulated)
    } else {
        let mut cleared = after_docket;
        cleared.evidence = None;
        cleared.vetting = None;
        populate_live_target_snapshots(&cleared, snapshot, backend);
        let admitted = telemetry
            .run_stage(PipelineStage::Admit, |scope| {
                let instrumented = scope.instrument_provider(provider);
                execute_admit(admit_opts, cleared, snapshot, &instrumented)
            })
            .map_err(|err| CliError::usage(err.to_string()))?;
        (admitted, StageDisposition::Executed)
    };

    if after_admit
        .evidence
        .as_ref()
        .is_some_and(|ev| ev.active_cases.is_empty())
    {
        return Ok(AuditCascadeOutcome {
            caseload: after_admit.with_verdict(CaseloadVerdict::default()),
            configure_disp,
            docket_disp,
            admit_disp,
            vet_disp: StageDisposition::SkippedZeroEvidence,
            audit_disp: StageDisposition::SkippedZeroEvidence,
        });
    }

    populate_live_case_and_reference_snapshots(&after_admit, snapshot, backend)?;
    let (after_vet, vet_disp) = execute_vet_step(
        after_admit,
        pre.vetting,
        vet_opts,
        snapshot,
        provider,
        telemetry,
    )?;

    if let Some(vetting) = after_vet.vetting.as_ref()
        && vetting.is_all_vetted()
    {
        let verdict = fold_vetted_to_verdict(vetting, audit_opts.threshold);
        return Ok(AuditCascadeOutcome {
            caseload: after_vet.with_verdict(verdict),
            configure_disp,
            docket_disp,
            admit_disp,
            vet_disp,
            audit_disp: StageDisposition::SkippedAllVetted,
        });
    }

    let after_audit = telemetry
        .run_stage(PipelineStage::Audit, |scope| {
            let instrumented = scope.instrument_provider(provider);
            execute_audit(audit_opts, after_vet, snapshot, &instrumented)
        })
        .map_err(|err| CliError::usage(err.to_string()))?;

    Ok(AuditCascadeOutcome {
        caseload: after_audit,
        configure_disp,
        docket_disp,
        admit_disp,
        vet_disp,
        audit_disp: StageDisposition::Executed,
    })
}

fn execute_docket_step(
    configured: Caseload,
    skip_docket: bool,
    docket_opts: &DocketOptions,
    snapshot: &WorkspaceSnapshot,
    provider: &dyn StructuredModelProvider,
    telemetry: &TelemetrySession<'_>,
) -> Result<(Caseload, StageDisposition), CliError> {
    if skip_docket {
        let disp = if configured.docket.is_some() {
            StageDisposition::PrePopulated
        } else {
            StageDisposition::SkippedZeroEvidence
        };
        return Ok((configured, disp));
    }
    let mut cleared = configured;
    cleared.docket = None;
    cleared.evidence = None;
    cleared.vetting = None;
    let docketed = telemetry
        .run_stage(PipelineStage::Docket, |scope| {
            let instrumented = scope.instrument_provider(provider);
            execute_docket(docket_opts, cleared, snapshot, &instrumented)
        })
        .map_err(|err| CliError::usage(err.to_string()))?;
    Ok((docketed, StageDisposition::Executed))
}

fn execute_vet_step(
    after_admit: Caseload,
    vetting_pre_populated: bool,
    vet_opts: &VetOptions,
    snapshot: &WorkspaceSnapshot,
    provider: &dyn StructuredModelProvider,
    telemetry: &TelemetrySession<'_>,
) -> Result<(Caseload, StageDisposition), CliError> {
    if vetting_pre_populated {
        return Ok((after_admit, StageDisposition::PrePopulated));
    }
    let uses_discriminative_vet = after_admit
        .config
        .as_ref()
        .and_then(|c| c.model_for(HeuristicNode::Vet))
        .is_some_and(is_discriminative_model);
    let mut cleared = after_admit;
    cleared.vetting = None;
    if !uses_discriminative_vet {
        return Ok((cleared, StageDisposition::SkippedGeneratorVet));
    }
    let vetted = telemetry
        .run_stage(PipelineStage::Vet, |scope| {
            let instrumented = scope.instrument_provider(provider);
            execute_vet(vet_opts, cleared, snapshot, &instrumented)
        })
        .map_err(|err| CliError::usage(err.to_string()))?;
    Ok((vetted, StageDisposition::Executed))
}
