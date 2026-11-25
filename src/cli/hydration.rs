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

//! Shared workspace snapshot hydration and `--workspace-state base` overlay helpers across
//! `admit`, `vet`, and `audit` (`docs/architecture/overview.md` §9).

use crate::audit::{
    apply_base_diff_to_references, collect_base_reference_disk_paths,
    materialize_base_diff_exhibits, resolve_canon_reference_paths,
};
use crate::caseload::{Caseload, CaseloadIntake, FileChangeStatus, WorkspaceState};
use crate::cli::CliError;
use crate::cli::closure::ClosureBackend;
use crate::discover::WorkspaceSnapshot;
use std::collections::BTreeSet;

/// Populates UTF-8 file contents for path-only target files and (when `diff_head_lines > 0`)
/// hydrates post-patch head-context contents for modified, renamed, or copied diff exhibits.
pub(crate) fn populate_live_target_snapshots(
    caseload: &Caseload,
    snapshot: &mut WorkspaceSnapshot,
    backend: &ClosureBackend<'_>,
    diff_head_lines: usize,
) {
    let Some(discovery) = &caseload.discovery else {
        return;
    };
    let intake = caseload.intake.as_ref();
    let diffs = intake.map(|i| &i.diffs);

    if let ClosureBackend::LiveDisk { workspace_root } = backend {
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

    if diff_head_lines > 0
        && let Some(intake) = intake
        && !intake.diffs.is_empty()
    {
        let diff_paths: Vec<String> = if !discovery.target_files.is_empty() {
            discovery
                .target_files
                .iter()
                .filter(|path| intake.diffs.contains_key(*path))
                .cloned()
                .collect()
        } else {
            intake.diffs.keys().cloned().collect()
        };
        hydrate_diff_head_context_paths(&diff_paths, intake, snapshot, backend);
    }
}

/// Hydrates base-branch reference files from disk (when live) and applies `.intake.diffs` in
/// memory for all candidate/active canons whenever `.intake.workspace_state == WorkspaceState::Base`.
pub(crate) fn materialize_base_workspace_references(
    caseload: &Caseload,
    snapshot: &mut WorkspaceSnapshot,
    backend: &ClosureBackend<'_>,
) -> Result<(), CliError> {
    let Some(intake) = caseload.intake.as_ref() else {
        return Ok(());
    };
    if intake.workspace_state != WorkspaceState::Base || intake.diffs.is_empty() {
        return Ok(());
    }

    let mut canon_refs = BTreeSet::new();
    if let Some(disc) = caseload.discovery.as_ref() {
        for canon_ref in &disc.candidate_canons {
            canon_refs.insert(canon_ref.clone());
        }
    }
    if let Some(evidence) = caseload.evidence.as_ref() {
        for canon_ref in &evidence.active_cases {
            canon_refs.insert(canon_ref.clone());
        }
    }
    let canon_list: Vec<String> = canon_refs.into_iter().collect();
    if canon_list.is_empty() {
        return Ok(());
    }

    if let ClosureBackend::LiveDisk { workspace_root } = backend {
        let base_paths = collect_base_reference_disk_paths(&canon_list, intake, snapshot);
        if !base_paths.is_empty() {
            snapshot.populate_file_contents_from_disk(workspace_root, &base_paths);
        }
    }

    apply_base_diff_to_references(&canon_list, intake, snapshot)
        .map_err(|err| CliError::usage(err.to_string()))
}

/// Hydrates admitted case exhibits, statutory `references:`, and diff head-context overlays
/// prior to `vet` and `audit`.
pub(crate) fn populate_live_case_and_reference_snapshots(
    caseload: &Caseload,
    snapshot: &mut WorkspaceSnapshot,
    backend: &ClosureBackend<'_>,
    diff_head_lines: usize,
) -> Result<(), CliError> {
    materialize_base_workspace_references(caseload, snapshot, backend)?;
    populate_live_target_snapshots(caseload, snapshot, backend, diff_head_lines);
    let Some(evidence) = &caseload.evidence else {
        return Ok(());
    };
    let intake = caseload.intake.as_ref();
    let is_base_state = intake.is_some_and(|i| i.workspace_state == WorkspaceState::Base);

    let mut admitted_paths = BTreeSet::new();
    let mut disk_paths_to_load = BTreeSet::new();
    for canon_ref in &evidence.active_cases {
        for admitted in evidence.admitted_exhibits_for(canon_ref) {
            admitted_paths.insert(admitted.file_path.clone());
            let is_base_diff_path =
                is_base_state && intake.is_some_and(|i| i.diffs.contains_key(&admitted.file_path));
            if !is_base_diff_path {
                disk_paths_to_load.insert(admitted.file_path.clone());
            }
        }
        if let Some(canon) = snapshot.canons.get(canon_ref) {
            for ref_path in resolve_canon_reference_paths(canon, snapshot) {
                disk_paths_to_load.insert(ref_path);
            }
        }
    }

    if diff_head_lines > 0
        && let Some(intake) = intake
    {
        let admitted_vec: Vec<String> = admitted_paths.into_iter().collect();
        hydrate_diff_head_context_paths(&admitted_vec, intake, snapshot, backend);
    }

    if let ClosureBackend::LiveDisk { workspace_root } = backend {
        let collected: Vec<String> = disk_paths_to_load.into_iter().collect();
        if !collected.is_empty() {
            snapshot.populate_file_contents_from_disk(workspace_root, &collected);
        }
    }
    Ok(())
}

fn hydrate_diff_head_context_paths(
    paths: &[String],
    intake: &CaseloadIntake,
    snapshot: &mut WorkspaceSnapshot,
    backend: &ClosureBackend<'_>,
) {
    if paths.is_empty() || intake.diffs.is_empty() {
        return;
    }
    match intake.workspace_state {
        WorkspaceState::Head => {
            if let ClosureBackend::LiveDisk { workspace_root } = backend {
                let head_paths: Vec<String> = paths
                    .iter()
                    .filter(|path| {
                        intake.diffs.get(*path).is_some_and(|art| {
                            matches!(
                                art.status,
                                FileChangeStatus::Modified
                                    | FileChangeStatus::Renamed
                                    | FileChangeStatus::Copied
                            ) && art.patch.is_some()
                                && art.content.is_none()
                        })
                    })
                    .cloned()
                    .collect();
                if !head_paths.is_empty() {
                    snapshot.populate_file_contents_from_disk(workspace_root, &head_paths);
                }
            }
        }
        WorkspaceState::Base => {
            let workspace_root = match backend {
                ClosureBackend::LiveDisk { workspace_root } => Some(*workspace_root),
                ClosureBackend::Hermetic { .. } => None,
            };
            materialize_base_diff_exhibits(paths, intake, snapshot, workspace_root);
        }
    }
}
