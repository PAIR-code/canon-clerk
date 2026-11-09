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

//! Guarded Branch A and JIT Branch B execution steps for `canon-clerk apprise`.

use super::render::StageDisposition;
use crate::apprise::{AppriseOptions, execute_apprise};
use crate::canon::ParseCanonOptions;
use crate::caseload::{Caseload, CaseloadIntake, IntakeScope};
use crate::cli::CliError;
use crate::cli::args::{CanonSelectionArgs, ProviderConfigArgs};
use crate::cli::runners::{
    map_discover_error, resolve_branch_b_options, resolve_gemini_api_key, run_branch_b_configure,
};
use crate::discover::{DiscoverOptions, WorkspaceSnapshot, execute_discover};
use crate::intake::{IntakeError, IntakeOptions, execute_intake};
use crate::provider::{
    DEFAULT_GENERATION_TIMEOUT_MS, GeminiModelProvider, StructuredModelProvider,
};
use crate::validate::{ValidateError, ValidateOptions, execute_validate};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub(super) enum AppriseBackend<'a> {
    LiveDisk {
        workspace_root: &'a Path,
    },
    Hermetic {
        workspace: &'a WorkspaceSnapshot,
        provider: &'a dyn StructuredModelProvider,
    },
}

pub(super) fn resolve_apprise_intake(
    intake_opts: &IntakeOptions,
    mut incoming: Option<Caseload>,
) -> Result<(Caseload, StageDisposition, bool), CliError> {
    if !intake_opts.has_designated_source() {
        let existing = incoming.ok_or_else(|| {
            CliError::usage("Missing upstream caseload or filing inputs for `apprise`.")
        })?;
        return Ok((existing, StageDisposition::PrePopulated, false));
    }

    let cli_has_targets = intake_opts.all_targets
        || intake_opts.patch_content.is_some()
        || intake_opts.target_paths.is_some();

    if !cli_has_targets
        && let Some(existing) = incoming.as_mut()
        && let Some(prev_intake) = existing.intake.take()
    {
        let merged = merge_metadata_onto_intake(prev_intake, intake_opts);
        existing.apprisal = None;
        existing.intake = Some(merged);
        return Ok((existing.clone(), StageDisposition::Executed, false));
    }

    let mut effective_opts = intake_opts.clone();
    if effective_opts.intent.is_none() {
        effective_opts.intent = incoming
            .as_ref()
            .and_then(|c| c.intake.as_ref())
            .and_then(|i| i.intent.clone());
    }

    let mut updated = execute_intake(&effective_opts, incoming).map_err(|err| match err {
        IntakeError::MissingInputSource | IntakeError::MalformedDiff(_) => {
            CliError::usage(err.to_string())
        }
    })?;
    if cli_has_targets {
        updated.discovery = None;
        updated.validation = None;
        updated.apprisal = None;
    }
    Ok((updated, StageDisposition::Executed, cli_has_targets))
}

fn merge_metadata_onto_intake(mut base: CaseloadIntake, opts: &IntakeOptions) -> CaseloadIntake {
    if let Some(intent) = opts
        .intent
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        base.intent = Some(intent.to_string());
    }
    if let Some(title) = opts
        .pr_title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        base.pr_title = Some(title.to_string());
    }
    if let Some(body) = opts
        .pr_body
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        base.pr_body = Some(body.to_string());
    }
    base
}

pub(super) fn materialize_snapshot(
    backend: &AppriseBackend<'_>,
    canon_patterns: &[String],
    caseload: &Caseload,
) -> Result<WorkspaceSnapshot, CliError> {
    match backend {
        AppriseBackend::Hermetic { workspace, .. } => Ok((*workspace).clone()),
        AppriseBackend::LiveDisk { workspace_root } => {
            let mut snapshot = WorkspaceSnapshot::from_disk(workspace_root, canon_patterns)
                .map_err(map_discover_error)?;
            if let Some(disc) = &caseload.discovery {
                for canon_ref in &disc.candidate_canons {
                    if !snapshot.canons.contains_key(canon_ref) {
                        let full_path = workspace_root.join(canon_ref);
                        if full_path.is_file()
                            && let Ok(raw) = fs::read_to_string(&full_path)
                        {
                            let opts = ParseCanonOptions::with_source_ref(canon_ref.clone());
                            snapshot = snapshot.with_canon_markdown(&raw, &opts);
                        }
                    }
                }
            }
            Ok(snapshot)
        }
    }
}

pub(super) fn resolve_apprise_discover(
    canons: &CanonSelectionArgs,
    mut caseload: Caseload,
    snapshot: &WorkspaceSnapshot,
    target_changed: bool,
) -> Result<(Caseload, StageDisposition), CliError> {
    if caseload.discovery.is_some() && !target_changed && canons.is_empty() {
        return Ok((caseload, StageDisposition::PrePopulated));
    }

    caseload.discovery = None;
    caseload.validation = None;
    caseload.apprisal = None;

    let intake_has_targets = caseload.intake.as_ref().is_some_and(|i| {
        i.scope == IntakeScope::AllTargets || !i.target_paths.is_empty() || !i.diffs.is_empty()
    });
    let discover_opts = DiscoverOptions {
        all_canons: canons.all_canons || (!intake_has_targets && !canons.canon.is_empty()),
        canon_patterns: canons.canon.clone(),
    };

    let updated =
        execute_discover(&discover_opts, caseload, snapshot).map_err(map_discover_error)?;
    Ok((updated, StageDisposition::Executed))
}

pub(super) fn resolve_apprise_validate(
    caseload: Caseload,
    snapshot: &WorkspaceSnapshot,
) -> Result<(Caseload, StageDisposition), CliError> {
    if caseload.validation.is_some() {
        return Ok((caseload, StageDisposition::PrePopulated));
    }
    let updated = execute_validate(&ValidateOptions::default(), caseload, snapshot).map_err(
        |err| match err {
            ValidateError::MissingDiscovery => CliError::usage(err.to_string()),
            ValidateError::Discover(d) => map_discover_error(d),
        },
    )?;
    Ok((updated, StageDisposition::Executed))
}

pub(super) fn execute_jit_configure_and_apprise(
    provider_args: &ProviderConfigArgs,
    env: &BTreeMap<String, String>,
    threshold: f64,
    mut caseload: Caseload,
    snapshot: &WorkspaceSnapshot,
    backend: &AppriseBackend<'_>,
) -> Result<(Caseload, StageDisposition), CliError> {
    let configure_options = resolve_branch_b_options(provider_args, env)?;
    let config_was_present =
        caseload.config.is_some() && !configure_options.has_explicit_overrides();
    caseload.apprisal = None;

    let configured = run_branch_b_configure(&configure_options, Some(caseload))?;
    let api_key = resolve_gemini_api_key(&configure_options)?;
    let apprise_opts = AppriseOptions {
        threshold,
        timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
    };

    let enriched = match backend {
        AppriseBackend::Hermetic { provider, .. } => {
            execute_apprise(&apprise_opts, configured, snapshot, *provider)
        }
        AppriseBackend::LiveDisk { .. } => {
            let live_provider = GeminiModelProvider::new(api_key);
            execute_apprise(&apprise_opts, configured, snapshot, &live_provider)
        }
    }
    .map_err(|err| CliError::usage(err.to_string()))?;

    let disp = if config_was_present {
        StageDisposition::PrePopulated
    } else {
        StageDisposition::Executed
    };
    Ok((enriched, disp))
}
