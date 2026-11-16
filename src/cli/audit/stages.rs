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

use super::AuditArgs;
use super::guards::{
    has_admit_cli_overrides, has_docket_cli_overrides, has_vet_cli_overrides,
    resolve_audit_concurrency,
};
use crate::admit::{AdmitOptions, execute_admit};
use crate::audit::{AuditOptions, DEFAULT_AUDIT_TIMEOUT_MS, execute_audit};
use crate::caseload::{
    CanonAdjudication, Caseload, CaseloadEvidence, CaseloadVerdict, CaseloadVetting, VerdictStatus,
};
use crate::cli::CliError;
use crate::cli::admit::guards::resolve_admit_concurrency;
use crate::cli::closure::{ClosureBackend, has_provider_overrides};
use crate::cli::docket::guards::{resolve_docket_batch_size, resolve_docket_concurrency};
use crate::cli::provenance::StageDisposition;
use crate::cli::runners::{
    build_live_model_provider, resolve_branch_b_options, run_branch_b_configure,
};
use crate::cli::vet::guards::resolve_vet_concurrency;
use crate::cli::vet::stages::{
    populate_live_case_and_reference_snapshots, populate_live_target_snapshots,
};
use crate::configure::is_discriminative_model;
use crate::discover::WorkspaceSnapshot;
use crate::docket::{DocketOptions, effective_docket_batch_size, execute_docket};
use crate::provider::{DEFAULT_GENERATION_TIMEOUT_MS, StructuredModelProvider};
use crate::telemetry::{PipelineStage, TelemetrySession};
use crate::vet::{VetOptions, execute_vet};
use std::collections::BTreeMap;

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

fn fold_vetted_to_verdict(vetting: &CaseloadVetting, audit_threshold: f64) -> CaseloadVerdict {
    let adjudications: Vec<CanonAdjudication> = vetting
        .vetted_cases
        .iter()
        .filter_map(|canon_ref| {
            vetting
                .cases
                .get(canon_ref)
                .map(|assessment| CanonAdjudication {
                    canon_path: canon_ref.clone(),
                    compliance_summary: assessment.vetting_summary.clone(),
                    compliance_score: assessment.vetting_score,
                    status: VerdictStatus::from_score(assessment.vetting_score, audit_threshold),
                    annotations: Vec::new(),
                })
        })
        .collect();
    CaseloadVerdict::from_adjudications(adjudications)
}

fn try_pre_populated_short_circuit(
    caseload: &Caseload,
    docket_pre_populated: bool,
    evidence_pre_populated: bool,
    vetting_pre_populated: bool,
    audit_threshold: f64,
) -> Option<AuditCascadeOutcome> {
    if vetting_pre_populated
        && let Some(vetting) = caseload.vetting.as_ref()
        && vetting.is_all_vetted()
    {
        let cfg_disp = if caseload.config.is_some() {
            StageDisposition::PrePopulated
        } else {
            StageDisposition::SkippedAllVetted
        };
        let doc_disp = if caseload.docket.is_some() {
            StageDisposition::PrePopulated
        } else {
            StageDisposition::SkippedAllVetted
        };
        let adm_disp = if caseload.evidence.is_some() {
            StageDisposition::PrePopulated
        } else {
            StageDisposition::SkippedAllVetted
        };
        return Some(AuditCascadeOutcome {
            caseload: caseload
                .clone()
                .with_verdict(fold_vetted_to_verdict(vetting, audit_threshold)),
            configure_disp: cfg_disp,
            docket_disp: doc_disp,
            admit_disp: adm_disp,
            vet_disp: StageDisposition::PrePopulated,
            audit_disp: StageDisposition::SkippedAllVetted,
        });
    }

    if evidence_pre_populated
        && caseload
            .evidence
            .as_ref()
            .is_some_and(|ev| ev.active_cases.is_empty())
    {
        let cfg_disp = if caseload.config.is_some() {
            StageDisposition::PrePopulated
        } else {
            StageDisposition::SkippedZeroEvidence
        };
        let doc_disp = if caseload.docket.is_some() {
            StageDisposition::PrePopulated
        } else {
            StageDisposition::SkippedZeroEvidence
        };
        return Some(AuditCascadeOutcome {
            caseload: caseload.clone().with_verdict(CaseloadVerdict::default()),
            configure_disp: cfg_disp,
            docket_disp: doc_disp,
            admit_disp: StageDisposition::PrePopulated,
            vet_disp: StageDisposition::SkippedZeroEvidence,
            audit_disp: StageDisposition::SkippedZeroEvidence,
        });
    }

    if !evidence_pre_populated
        && docket_pre_populated
        && caseload
            .docket
            .as_ref()
            .is_some_and(|d| d.active_docket.is_empty())
    {
        let cfg_disp = if caseload.config.is_some() {
            StageDisposition::PrePopulated
        } else {
            StageDisposition::SkippedEmptyDocket
        };
        return Some(AuditCascadeOutcome {
            caseload: caseload
                .clone()
                .with_evidence(CaseloadEvidence::default())
                .with_verdict(CaseloadVerdict::default()),
            configure_disp: cfg_disp,
            docket_disp: StageDisposition::PrePopulated,
            admit_disp: StageDisposition::SkippedEmptyDocket,
            vet_disp: StageDisposition::SkippedEmptyDocket,
            audit_disp: StageDisposition::SkippedEmptyDocket,
        });
    }

    None
}

struct PrePopulatedFlags {
    docket: bool,
    evidence: bool,
    vetting: bool,
}

pub(super) fn execute_jit_configure_docket_admit_and_audit(
    args: &AuditArgs,
    env: &BTreeMap<String, String>,
    thresholds: AuditCascadeThresholds,
    mut caseload: Caseload,
    snapshot: &mut WorkspaceSnapshot,
    backend: &ClosureBackend<'_>,
    telemetry: &TelemetrySession<'_>,
) -> Result<AuditCascadeOutcome, CliError> {
    let provider_overridden = has_provider_overrides(&args.provider);
    let docket_pre_populated =
        caseload.docket.is_some() && !has_docket_cli_overrides(args) && !provider_overridden;
    let evidence_pre_populated = caseload.evidence.is_some()
        && !has_admit_cli_overrides(args)
        && !has_docket_cli_overrides(args)
        && !provider_overridden;
    let vetting_pre_populated =
        caseload.vetting.is_some() && evidence_pre_populated && !has_vet_cli_overrides(args);

    if let Some(short_circuit) = try_pre_populated_short_circuit(
        &caseload,
        docket_pre_populated,
        evidence_pre_populated,
        vetting_pre_populated,
        thresholds.audit_threshold,
    ) {
        return Ok(short_circuit);
    }

    let configure_options = resolve_branch_b_options(&args.provider, env)?;
    let stored = configure_options.stored_config.as_ref();
    let explicit_docket_batch_size =
        resolve_docket_batch_size(args.docket_batch_size, env, stored)?;
    let docket_concurrency = resolve_docket_concurrency(args.docket_concurrency, env, stored)?;
    let admit_opts = AdmitOptions {
        threshold: thresholds.admit_threshold,
        timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
        max_concurrent_calls: resolve_admit_concurrency(args.admit_concurrency, env, stored)?,
    };
    let vet_opts = VetOptions {
        threshold: thresholds.vet_threshold,
        timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
        max_concurrent_calls: resolve_vet_concurrency(args.vet_concurrency, env, stored)?,
    };
    let audit_opts = AuditOptions {
        threshold: thresholds.audit_threshold,
        timeout_ms: DEFAULT_AUDIT_TIMEOUT_MS,
        max_concurrent_calls: resolve_audit_concurrency(args.audit_concurrency, env, stored)?,
    };

    let retry_policy = args.transport.resolve_retry_policy(env, stored)?;

    let config_was_present =
        caseload.config.is_some() && !configure_options.has_explicit_overrides();
    caseload.verdict = None;

    let configured = run_branch_b_configure(&configure_options, Some(caseload), telemetry)?;
    let cfg_snapshot = configured.config.clone().unwrap_or_default();
    let docket_opts = DocketOptions {
        threshold: thresholds.docket_threshold,
        timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
        max_canons_per_call: effective_docket_batch_size(
            explicit_docket_batch_size,
            &cfg_snapshot.screener_model,
        ),
        max_concurrent_calls: docket_concurrency,
    };
    let live_instance;
    let active_provider: &dyn StructuredModelProvider = match backend {
        ClosureBackend::Hermetic { provider, .. } => *provider,
        ClosureBackend::LiveDisk { .. } => {
            live_instance =
                build_live_model_provider(&configure_options, &cfg_snapshot, retry_policy)?;
            &live_instance
        }
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
        (&docket_opts, &admit_opts, &vet_opts, &audit_opts),
        snapshot,
        backend,
        active_provider,
        telemetry,
    )
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

    populate_live_case_and_reference_snapshots(&after_admit, snapshot, backend);
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
    let uses_discriminative_screener = after_admit
        .config
        .as_ref()
        .is_some_and(|c| is_discriminative_model(&c.screener_model));
    let mut cleared = after_admit;
    cleared.vetting = None;
    if !uses_discriminative_screener {
        return Ok((cleared, StageDisposition::SkippedGenerativeScreener));
    }
    let vetted = telemetry
        .run_stage(PipelineStage::Vet, |scope| {
            let instrumented = scope.instrument_provider(provider);
            execute_vet(vet_opts, cleared, snapshot, &instrumented)
        })
        .map_err(|err| CliError::usage(err.to_string()))?;
    Ok((vetted, StageDisposition::Executed))
}
