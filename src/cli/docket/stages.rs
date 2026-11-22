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

//! Guarded Branch A and JIT Branch B execution steps for `canon-clerk docket`.

use super::DocketArgs;
use super::guards::{resolve_docket_batch_size, resolve_docket_concurrency};
use crate::caseload::{Caseload, HeuristicNode};
use crate::cli::CliError;
use crate::cli::args::CanonSelectionArgs;
use crate::cli::closure::{
    ClosureBackend, has_filing_metadata_overrides, merge_metadata_onto_intake,
};
use crate::cli::provenance::StageDisposition;
use crate::cli::runners::{
    build_live_model_provider, map_discover_error, resolve_closure_branch_b_options,
    run_branch_b_configure,
};
use crate::configure::{ConfigureScope, StoredConfig, can_fast_forward_existing_config};
use crate::discover::{DiscoverOptions, WorkspaceSnapshot, execute_discover};
use crate::docket::{DocketOptions, effective_docket_batch_size, execute_docket};
use crate::intake::{IntakeError, IntakeOptions, execute_intake};
use crate::provider::DEFAULT_GENERATION_TIMEOUT_MS;
use crate::telemetry::{PipelineStage, TelemetrySession};
use std::collections::BTreeMap;

pub(super) fn resolve_docket_intake(
    intake_opts: &IntakeOptions,
    incoming: Option<Caseload>,
    telemetry: &TelemetrySession<'_>,
) -> Result<(Caseload, StageDisposition, bool), CliError> {
    if !intake_opts.has_designated_source() {
        let existing = incoming.ok_or_else(|| {
            CliError::usage("Missing upstream caseload or filing inputs for `docket`.")
        })?;
        return Ok((existing, StageDisposition::PrePopulated, false));
    }

    telemetry.run_stage(PipelineStage::Intake, |_scope| {
        execute_docket_intake_body(intake_opts, incoming)
    })
}

fn execute_docket_intake_body(
    intake_opts: &IntakeOptions,
    mut incoming: Option<Caseload>,
) -> Result<(Caseload, StageDisposition, bool), CliError> {
    let cli_has_targets = intake_opts.all_targets
        || intake_opts.patch_content.is_some()
        || intake_opts.target_paths.is_some();

    if !cli_has_targets
        && let Some(existing) = incoming.as_mut()
        && let Some(prev_intake) = existing.intake.take()
    {
        let pr_meta_added = has_filing_metadata_overrides(intake_opts);
        let merged = merge_metadata_onto_intake(prev_intake, intake_opts);
        existing.docket = None;
        existing.intake = Some(merged);
        if pr_meta_added {
            existing.discovery = None;
            existing.validation = None;
        }
        return Ok((existing.clone(), StageDisposition::Executed, pr_meta_added));
    }

    let mut updated = execute_intake(intake_opts, incoming).map_err(|err| match err {
        IntakeError::MissingInputSource | IntakeError::MalformedDiff(_) => {
            CliError::usage(err.to_string())
        }
    })?;
    updated.discovery = None;
    updated.validation = None;
    updated.docket = None;
    Ok((updated, StageDisposition::Executed, true))
}

pub(super) fn resolve_docket_discover(
    canons: &CanonSelectionArgs,
    mut caseload: Caseload,
    snapshot: &WorkspaceSnapshot,
    intake_changed: bool,
    telemetry: &TelemetrySession<'_>,
) -> Result<(Caseload, StageDisposition), CliError> {
    if caseload.discovery.is_some() && !intake_changed && canons.is_empty() {
        return Ok((caseload, StageDisposition::PrePopulated));
    }

    caseload.discovery = None;
    caseload.validation = None;
    caseload.docket = None;

    let discover_opts = DiscoverOptions {
        all_canons: canons.all_canons,
        canon_patterns: canons.canon.clone(),
        include_builtins: canons.include_builtins(),
    };

    let updated = telemetry.run_stage(PipelineStage::Discover, |_scope| {
        execute_discover(&discover_opts, caseload, snapshot).map_err(map_discover_error)
    })?;
    Ok((updated, StageDisposition::Executed))
}

#[allow(
    clippy::too_many_arguments,
    reason = "Internal stage helper threading CLI args, env, stored config, threshold, caseload, workspace, backend, and telemetry"
)]
pub(super) fn execute_jit_configure_and_docket(
    args: &DocketArgs,
    env: &BTreeMap<String, String>,
    stored_config: Option<StoredConfig>,
    threshold: f64,
    mut caseload: Caseload,
    snapshot: &WorkspaceSnapshot,
    backend: &ClosureBackend<'_>,
    telemetry: &TelemetrySession<'_>,
) -> Result<(Caseload, StageDisposition), CliError> {
    let configure_options = resolve_closure_branch_b_options(
        &args.provider,
        args.models.resolve_node_models()?,
        ConfigureScope::closure([HeuristicNode::Docket]),
        stored_config,
        env,
    )?;
    let max_canons_per_call = resolve_docket_batch_size(
        args.docket_batch_size,
        env,
        configure_options.stored_config.as_ref(),
    )?;
    let max_concurrent_calls = resolve_docket_concurrency(
        args.docket_concurrency,
        env,
        configure_options.stored_config.as_ref(),
    )?;
    let retry_policy = args
        .transport
        .resolve_retry_policy(env, configure_options.stored_config.as_ref())?;
    let config_was_present =
        can_fast_forward_existing_config(&configure_options, caseload.config.as_ref());
    caseload.docket = None;

    let configured = run_branch_b_configure(&configure_options, Some(caseload), telemetry)?;
    let cfg_snapshot = configured.config.clone().unwrap_or_default();
    let live_provider = match backend {
        ClosureBackend::Hermetic { .. } => None,
        ClosureBackend::LiveDisk { .. } => Some(build_live_model_provider(
            &configure_options,
            &cfg_snapshot,
            retry_policy,
        )?),
    };
    let docket_model = cfg_snapshot
        .model_for(HeuristicNode::Docket)
        .unwrap_or_default();
    let docket_opts = DocketOptions {
        threshold,
        timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
        max_canons_per_call: effective_docket_batch_size(max_canons_per_call, docket_model),
        max_concurrent_calls,
    };

    let enriched = telemetry
        .run_stage(PipelineStage::Docket, |scope| match backend {
            ClosureBackend::Hermetic { provider, .. } => {
                let instrumented = scope.instrument_provider(*provider);
                execute_docket(&docket_opts, configured, snapshot, &instrumented)
            }
            ClosureBackend::LiveDisk { .. } => {
                let active = live_provider
                    .as_ref()
                    .ok_or(crate::docket::DocketError::MissingConfig)?;
                let instrumented = scope.instrument_provider(active);
                execute_docket(&docket_opts, configured, snapshot, &instrumented)
            }
        })
        .map_err(|err| CliError::usage(err.to_string()))?;

    let disp = if config_was_present {
        StageDisposition::PrePopulated
    } else {
        StageDisposition::Executed
    };
    Ok((enriched, disp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{
        CaseloadDiscovery, CaseloadDocket, CaseloadIntake, CaseloadValidation, LinkedIssueContext,
    };

    #[test]
    fn test_docket_caseload_override_with_linked_issues_replaces_and_invalidates_upstream() {
        let incoming = Caseload::new()
            .with_intake(CaseloadIntake {
                target_paths: vec!["src/lib.rs".to_string()],
                linked_issues: Some(vec![LinkedIssueContext {
                    number: 383,
                    title: Some("Original issue".to_string()),
                    author: None,
                    labels: Vec::new(),
                    body: None,
                    url: None,
                }]),
                ..CaseloadIntake::default()
            })
            .with_discovery(CaseloadDiscovery::default())
            .with_validation(CaseloadValidation::default())
            .with_docket(CaseloadDocket::default());

        let intake_opts = IntakeOptions {
            linked_issues: Some(vec![LinkedIssueContext {
                number: 391,
                title: Some("Override issue".to_string()),
                author: None,
                labels: Vec::new(),
                body: None,
                url: None,
            }]),
            ..IntakeOptions::default()
        };

        let (updated, disp, intake_changed) =
            resolve_docket_intake(&intake_opts, Some(incoming), &TelemetrySession::noop())
                .expect("merge linked_issues onto incoming docket caseload");

        assert_eq!(disp, StageDisposition::Executed);
        assert!(intake_changed);
        assert!(updated.discovery.is_none());
        assert!(updated.validation.is_none());
        assert!(updated.docket.is_none());
        let intake = updated.intake.expect("intake");
        assert_eq!(intake.target_paths, vec!["src/lib.rs".to_string()]);
        let issues = intake.linked_issues.expect("linked_issues");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 391);
    }
}
