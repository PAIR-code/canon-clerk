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

//! Context-budgeted multi-slice partitioning for discriminative `vet` (`docs/architecture/nodes/vet.md` §4.3).

use crate::audit::{
    RenderedFileExhibit, build_shared_case_header, format_pinned_reference_exhibits,
    is_metadata_plane_id, pack_rendered_exhibit_slices, partition_and_render_file_exhibits,
    render_admitted_file_exhibits, resolve_canon_reference_paths,
};
use crate::canon::Canon;
use crate::caseload::{CaseloadDiscovery, CaseloadIntake, ExhibitAssessment};
use crate::discover::WorkspaceSnapshot;
use std::collections::BTreeSet;
use std::fmt::Write as _;

const SLICE_FRAME_RESERVE_CHARS: usize = 256;

/// One context-budgeted discriminative `state` slice paired with the sliceable file exhibit paths it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VettingExhibitSlice {
    pub(super) state: String,
    pub(super) file_paths: Vec<String>,
}

/// Execution plan for evaluating one active case within a discriminative context ceiling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DiscriminativeCasePlan {
    /// One or more context-budgeted `state` slices whose conjunctive minimum score governs the case.
    Slices(Vec<VettingExhibitSlice>),
    /// Single exhibit or statutory reference set exceeds the discriminative window; escalate as `contested`.
    OversizedContest,
}

/// Builds a single-call or multi-slice discriminative `state` plan bounded by `max_state_chars`.
#[allow(
    clippy::too_many_arguments,
    reason = "Internal slice planner assembling case overview, canon statute, references, and admitted exhibits"
)]
pub(super) fn build_discriminative_case_plan(
    canon_ref: &str,
    canon: &Canon,
    admitted_exhibits: &[&ExhibitAssessment],
    full_prompt: &str,
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
    workspace: &WorkspaceSnapshot,
    max_state_chars: usize,
) -> DiscriminativeCasePlan {
    let admitted_file_paths: BTreeSet<String> = admitted_exhibits
        .iter()
        .map(|e| e.file_path.as_str())
        .filter(|id| !is_metadata_plane_id(id))
        .map(ToString::to_string)
        .collect();

    if full_prompt.len() <= max_state_chars {
        return DiscriminativeCasePlan::Slices(vec![VettingExhibitSlice {
            state: full_prompt.to_string(),
            file_paths: admitted_file_paths.into_iter().collect(),
        }]);
    }

    let reference_paths = resolve_canon_reference_paths(canon, workspace);
    let (shared_header, file_exhibits) = prepare_header_and_sliceable_exhibits(
        canon_ref,
        canon,
        admitted_exhibits,
        &admitted_file_paths,
        &reference_paths,
        intake,
        discovery,
        workspace,
        max_state_chars,
    );

    if file_exhibits.is_empty() {
        return if shared_header.len() <= max_state_chars {
            DiscriminativeCasePlan::Slices(vec![VettingExhibitSlice {
                state: shared_header,
                file_paths: Vec::new(),
            }])
        } else {
            DiscriminativeCasePlan::OversizedContest
        };
    }

    if shared_header.len() + SLICE_FRAME_RESERVE_CHARS >= max_state_chars {
        return DiscriminativeCasePlan::OversizedContest;
    }
    let body_budget = max_state_chars - shared_header.len() - SLICE_FRAME_RESERVE_CHARS;

    let Some(groups) = pack_rendered_exhibit_slices(&file_exhibits, body_budget, false) else {
        return DiscriminativeCasePlan::OversizedContest;
    };

    let total_slices = groups.len();
    let slices = groups
        .into_iter()
        .enumerate()
        .map(|(idx, group)| {
            let mut slice_state =
                String::with_capacity(shared_header.len() + SLICE_FRAME_RESERVE_CHARS + 1024);
            slice_state.push_str(&shared_header);
            let _ = writeln!(
                slice_state,
                "## Admitted Exhibits (Slice {} of {}, {} file exhibit(s))\n",
                idx + 1,
                total_slices,
                group.len()
            );
            let mut file_paths = Vec::with_capacity(group.len());
            for item in group {
                slice_state.push_str(&item.rendered);
                file_paths.push(item.file_path.clone());
            }
            VettingExhibitSlice {
                state: slice_state,
                file_paths,
            }
        })
        .collect();

    DiscriminativeCasePlan::Slices(slices)
}

#[allow(
    clippy::too_many_arguments,
    reason = "Internal helper pairing shared slice header with pinned vs. sliceable file exhibits"
)]
fn prepare_header_and_sliceable_exhibits(
    canon_ref: &str,
    canon: &Canon,
    admitted_exhibits: &[&ExhibitAssessment],
    admitted_file_paths: &BTreeSet<String>,
    reference_paths: &[String],
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
    workspace: &WorkspaceSnapshot,
    max_state_chars: usize,
) -> (String, Vec<RenderedFileExhibit>) {
    let mut header = build_shared_case_header(
        canon_ref,
        canon,
        admitted_exhibits,
        admitted_file_paths,
        reference_paths,
        intake,
        discovery,
        workspace,
    );

    let reference_set: BTreeSet<&str> = reference_paths.iter().map(String::as_str).collect();
    let (pinned_refs, non_ref_files) =
        partition_and_render_file_exhibits(admitted_exhibits, &reference_set, intake, workspace);

    if !pinned_refs.is_empty() && !non_ref_files.is_empty() {
        let refs_slice: Vec<&RenderedFileExhibit> = pinned_refs.iter().collect();
        let pinned_section = format_pinned_reference_exhibits(&refs_slice);
        if header.len() + pinned_section.len() <= max_state_chars / 2 {
            header.push_str(&pinned_section);
            return (header, non_ref_files);
        }
    }

    let all_files = render_admitted_file_exhibits(admitted_exhibits, intake, workspace);
    (header, all_files)
}
