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

//! Guarded Branch A, JIT Branch B, and `docket` → `admit` → `vet` execution steps for `canon-clerk vet`.

use super::VetArgs;
use super::guards::{has_admit_cli_overrides, has_docket_cli_overrides, resolve_vet_concurrency};
use crate::admit::{AdmitOptions, execute_admit};
use crate::audit::resolve_canon_reference_paths;
use crate::caseload::{Caseload, CaseloadEvidence, CaseloadVetting};
use crate::cli::CliError;
use crate::cli::admit::guards::resolve_admit_concurrency;
use crate::cli::closure::{ClosureBackend, has_provider_overrides};
use crate::cli::docket::guards::{resolve_docket_batch_size, resolve_docket_concurrency};
use crate::cli::provenance::StageDisposition;
use crate::cli::runners::{
    build_live_model_provider, resolve_screener_branch_b_options, run_branch_b_configure,
};
use crate::discover::WorkspaceSnapshot;
use crate::docket::{DocketOptions, execute_docket};
use crate::provider::{DEFAULT_GENERATION_TIMEOUT_MS, StructuredModelProvider};
use crate::telemetry::{PipelineStage, TelemetrySession};
use crate::vet::{VetOptions, execute_vet};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct VetCascadeOutcome {
    pub(super) caseload: Caseload,
    pub(super) configure_disp: StageDisposition,
    pub(super) docket_disp: StageDisposition,
    pub(super) admit_disp: StageDisposition,
    pub(super) vet_disp: StageDisposition,
}

#[derive(Clone, Copy)]
pub(super) struct VetCascadeThresholds {
    pub(super) docket_threshold: f64,
    pub(super) admit_threshold: f64,
    pub(super) vet_threshold: f64,
}

fn try_pre_populated_short_circuit(
    caseload: &Caseload,
    docket_pre_populated: bool,
    evidence_pre_populated: bool,
) -> Option<VetCascadeOutcome> {
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
        return Some(VetCascadeOutcome {
            caseload: caseload.clone().with_vetting(CaseloadVetting::default()),
            configure_disp: cfg_disp,
            docket_disp: doc_disp,
            admit_disp: StageDisposition::PrePopulated,
            vet_disp: StageDisposition::SkippedZeroEvidence,
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
        return Some(VetCascadeOutcome {
            caseload: caseload
                .clone()
                .with_evidence(CaseloadEvidence::default())
                .with_vetting(CaseloadVetting::default()),
            configure_disp: cfg_disp,
            docket_disp: StageDisposition::PrePopulated,
            admit_disp: StageDisposition::SkippedEmptyDocket,
            vet_disp: StageDisposition::SkippedEmptyDocket,
        });
    }

    None
}

pub(super) fn execute_jit_configure_docket_admit_and_vet(
    args: &VetArgs,
    env: &BTreeMap<String, String>,
    thresholds: VetCascadeThresholds,
    mut caseload: Caseload,
    snapshot: &mut WorkspaceSnapshot,
    backend: &ClosureBackend<'_>,
    telemetry: &TelemetrySession<'_>,
) -> Result<VetCascadeOutcome, CliError> {
    let provider_overridden = has_provider_overrides(&args.provider);
    let docket_pre_populated =
        caseload.docket.is_some() && !has_docket_cli_overrides(args) && !provider_overridden;
    let evidence_pre_populated = caseload.evidence.is_some()
        && !has_admit_cli_overrides(args)
        && !has_docket_cli_overrides(args)
        && !provider_overridden;

    if let Some(short_circuit) =
        try_pre_populated_short_circuit(&caseload, docket_pre_populated, evidence_pre_populated)
    {
        return Ok(short_circuit);
    }

    let configure_options = resolve_screener_branch_b_options(&args.provider, env)?;
    let stored = configure_options.stored_config.as_ref();
    let docket_opts = DocketOptions {
        threshold: thresholds.docket_threshold,
        timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
        max_canons_per_call: resolve_docket_batch_size(args.docket_batch_size, env, stored)?,
        max_concurrent_calls: resolve_docket_concurrency(args.docket_concurrency, env, stored)?,
    };
    let admit_opts = AdmitOptions {
        threshold: thresholds.admit_threshold,
        timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
        max_concurrent_calls: resolve_admit_concurrency(args.admit_concurrency, env, stored)?,
    };
    let vet_opts = VetOptions {
        threshold: thresholds.vet_threshold,
        timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
        max_concurrent_calls: resolve_vet_concurrency(args.vet_concurrency, env, stored)?,
        allow_oversized_contest_fallback: false,
    };

    let config_was_present =
        caseload.config.is_some() && !configure_options.has_explicit_overrides();
    caseload.vetting = None;
    caseload.verdict = None;

    let configured = run_branch_b_configure(&configure_options, Some(caseload), telemetry)?;
    let cfg_snapshot = configured.config.clone().unwrap_or_default();
    let live_instance;
    let active_provider: &dyn StructuredModelProvider = match backend {
        ClosureBackend::Hermetic { provider, .. } => *provider,
        ClosureBackend::LiveDisk { .. } => {
            live_instance = build_live_model_provider(&configure_options, &cfg_snapshot)?;
            &live_instance
        }
    };
    let configure_disp = if config_was_present {
        StageDisposition::PrePopulated
    } else {
        StageDisposition::Executed
    };

    execute_configured_vet_spine(
        configured,
        configure_disp,
        docket_pre_populated,
        evidence_pre_populated,
        (&docket_opts, &admit_opts, &vet_opts),
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
fn execute_configured_vet_spine(
    configured: Caseload,
    configure_disp: StageDisposition,
    docket_pre_populated: bool,
    evidence_pre_populated: bool,
    stage_opts: (&DocketOptions, &AdmitOptions, &VetOptions),
    snapshot: &mut WorkspaceSnapshot,
    backend: &ClosureBackend<'_>,
    provider: &dyn StructuredModelProvider,
    telemetry: &TelemetrySession<'_>,
) -> Result<VetCascadeOutcome, CliError> {
    let (docket_opts, admit_opts, vet_opts) = stage_opts;

    let (after_docket, docket_disp) = if evidence_pre_populated || docket_pre_populated {
        let disp = if configured.docket.is_some() {
            StageDisposition::PrePopulated
        } else {
            StageDisposition::SkippedZeroEvidence
        };
        (configured, disp)
    } else {
        let mut cleared = configured;
        cleared.docket = None;
        cleared.evidence = None;
        let docketed = telemetry
            .run_stage(PipelineStage::Docket, |scope| {
                let instrumented = scope.instrument_provider(provider);
                execute_docket(docket_opts, cleared, snapshot, &instrumented)
            })
            .map_err(|err| CliError::usage(err.to_string()))?;
        (docketed, StageDisposition::Executed)
    };

    if !evidence_pre_populated
        && after_docket
            .docket
            .as_ref()
            .is_some_and(|d| d.active_docket.is_empty())
    {
        return Ok(VetCascadeOutcome {
            caseload: after_docket
                .with_evidence(CaseloadEvidence::default())
                .with_vetting(CaseloadVetting::default()),
            configure_disp,
            docket_disp,
            admit_disp: StageDisposition::SkippedEmptyDocket,
            vet_disp: StageDisposition::SkippedEmptyDocket,
        });
    }

    let (after_admit, admit_disp) = if evidence_pre_populated {
        (after_docket, StageDisposition::PrePopulated)
    } else {
        let mut cleared = after_docket;
        cleared.evidence = None;
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
        return Ok(VetCascadeOutcome {
            caseload: after_admit.with_vetting(CaseloadVetting::default()),
            configure_disp,
            docket_disp,
            admit_disp,
            vet_disp: StageDisposition::SkippedZeroEvidence,
        });
    }

    populate_live_case_and_reference_snapshots(&after_admit, snapshot, backend);
    let after_vet = telemetry
        .run_stage(PipelineStage::Vet, |scope| {
            let instrumented = scope.instrument_provider(provider);
            execute_vet(vet_opts, after_admit, snapshot, &instrumented)
        })
        .map_err(|err| CliError::usage(err.to_string()))?;

    Ok(VetCascadeOutcome {
        caseload: after_vet,
        configure_disp,
        docket_disp,
        admit_disp,
        vet_disp: StageDisposition::Executed,
    })
}

pub(crate) fn populate_live_target_snapshots(
    caseload: &Caseload,
    snapshot: &mut WorkspaceSnapshot,
    backend: &ClosureBackend<'_>,
) {
    let ClosureBackend::LiveDisk { workspace_root } = backend else {
        return;
    };
    let Some(discovery) = &caseload.discovery else {
        return;
    };
    let diffs = caseload.intake.as_ref().map(|i| &i.diffs);
    let paths_to_load: Vec<String> = discovery
        .target_files
        .iter()
        .filter(|path| {
            diffs.is_none_or(|d| {
                d.get(*path)
                    .is_none_or(|art| art.patch.is_none() && art.content.is_none())
            })
        })
        .cloned()
        .collect();
    if !paths_to_load.is_empty() {
        snapshot.populate_file_contents_from_disk(workspace_root, &paths_to_load);
    }
}

pub(crate) fn populate_live_case_and_reference_snapshots(
    caseload: &Caseload,
    snapshot: &mut WorkspaceSnapshot,
    backend: &ClosureBackend<'_>,
) {
    populate_live_target_snapshots(caseload, snapshot, backend);
    let ClosureBackend::LiveDisk { workspace_root } = backend else {
        return;
    };
    let Some(evidence) = &caseload.evidence else {
        return;
    };
    let mut paths_to_load = BTreeSet::new();
    for canon_ref in &evidence.active_cases {
        for admitted in evidence.admitted_exhibits_for(canon_ref) {
            paths_to_load.insert(admitted.file_path.clone());
        }
        if let Some(canon) = snapshot.canons.get(canon_ref) {
            for ref_path in resolve_canon_reference_paths(canon, snapshot) {
                paths_to_load.insert(ref_path);
            }
        }
    }
    let collected: Vec<String> = paths_to_load.into_iter().collect();
    if !collected.is_empty() {
        snapshot.populate_file_contents_from_disk(workspace_root, &collected);
    }
}
