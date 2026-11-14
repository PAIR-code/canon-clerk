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
    append_admitted_exhibit_section, append_case_overview, append_governing_canon_statute,
    append_grounding_references_section, is_metadata_plane_id, resolve_canon_reference_paths,
};
use crate::canon::Canon;
use crate::caseload::{CaseloadDiscovery, CaseloadIntake, ExhibitAssessment};
use crate::discover::WorkspaceSnapshot;
use std::collections::BTreeSet;
use std::fmt::Write as _;

const SLICE_FRAME_RESERVE_CHARS: usize = 256;

/// Execution plan for evaluating one active case within a discriminative context ceiling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DiscriminativeCasePlan {
    /// One or more context-budgeted `state` slices whose conjunctive minimum score governs the case.
    Slices(Vec<String>),
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
    if full_prompt.len() <= max_state_chars {
        return DiscriminativeCasePlan::Slices(vec![full_prompt.to_string()]);
    }

    let admitted_file_paths: BTreeSet<String> = admitted_exhibits
        .iter()
        .map(|e| e.file_path.as_str())
        .filter(|id| !is_metadata_plane_id(id))
        .map(ToString::to_string)
        .collect();
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
            DiscriminativeCasePlan::Slices(vec![shared_header])
        } else {
            DiscriminativeCasePlan::OversizedContest
        };
    }

    if shared_header.len() + SLICE_FRAME_RESERVE_CHARS >= max_state_chars {
        return DiscriminativeCasePlan::OversizedContest;
    }
    let body_budget = max_state_chars - shared_header.len() - SLICE_FRAME_RESERVE_CHARS;

    let Some(groups) = pack_file_exhibit_slices(&file_exhibits, body_budget) else {
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
            for rendered in group {
                slice_state.push_str(rendered);
            }
            slice_state
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
) -> (String, Vec<String>) {
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
        let pinned_section = format_pinned_reference_exhibits(&pinned_refs);
        if header.len() + pinned_section.len() <= max_state_chars / 2 {
            header.push_str(&pinned_section);
            return (header, non_ref_files);
        }
    }

    let all_files = render_admitted_file_exhibits(admitted_exhibits, intake, workspace);
    (header, all_files)
}

#[allow(
    clippy::too_many_arguments,
    reason = "Internal header builder combining overview, statute, references, metadata planes, and path manifest"
)]
fn build_shared_case_header(
    canon_ref: &str,
    canon: &Canon,
    admitted_exhibits: &[&ExhibitAssessment],
    admitted_file_paths: &BTreeSet<String>,
    reference_paths: &[String],
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
    workspace: &WorkspaceSnapshot,
) -> String {
    let mut header = String::with_capacity(2048);

    append_case_overview(&mut header, canon_ref, intake, discovery);
    append_governing_canon_statute(&mut header, canon_ref, canon);
    append_grounding_references_section(
        &mut header,
        reference_paths,
        admitted_file_paths,
        workspace,
    );

    let metadata_exhibits: Vec<&&ExhibitAssessment> = admitted_exhibits
        .iter()
        .filter(|e| is_metadata_plane_id(&e.file_path))
        .collect();
    if !metadata_exhibits.is_empty() {
        let _ = writeln!(
            header,
            "## Admitted Metadata Exhibits ({})\n",
            metadata_exhibits.len()
        );
        for exhibit in metadata_exhibits {
            append_admitted_exhibit_section(&mut header, exhibit, intake, workspace);
        }
    }

    let _ = writeln!(header, "## All Admitted File Exhibit Paths in Filing");
    if admitted_file_paths.is_empty() {
        let _ = writeln!(header, "(none — metadata-only case)\n");
    } else {
        for (idx, path) in admitted_file_paths.iter().enumerate() {
            let _ = writeln!(header, "{}. `{path}`", idx + 1);
        }
        header.push('\n');
    }
    header
}

fn partition_and_render_file_exhibits(
    admitted_exhibits: &[&ExhibitAssessment],
    reference_set: &BTreeSet<&str>,
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
) -> (Vec<String>, Vec<String>) {
    let mut pinned_refs = Vec::new();
    let mut non_ref_files = Vec::new();
    for exhibit in admitted_exhibits {
        if is_metadata_plane_id(&exhibit.file_path) {
            continue;
        }
        let mut block = String::with_capacity(768);
        append_admitted_exhibit_section(&mut block, exhibit, intake, workspace);
        if reference_set.contains(exhibit.file_path.as_str()) {
            pinned_refs.push(block);
        } else {
            non_ref_files.push(block);
        }
    }
    (pinned_refs, non_ref_files)
}

fn format_pinned_reference_exhibits(pinned_refs: &[String]) -> String {
    let mut out = String::with_capacity(pinned_refs.iter().map(String::len).sum::<usize>() + 128);
    let _ = writeln!(
        out,
        "## Pinned Companion Reference Exhibits ({})\n",
        pinned_refs.len()
    );
    for block in pinned_refs {
        out.push_str(block);
    }
    out
}

fn render_admitted_file_exhibits(
    admitted_exhibits: &[&ExhibitAssessment],
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
) -> Vec<String> {
    let mut rendered = Vec::new();
    for exhibit in admitted_exhibits {
        if !is_metadata_plane_id(&exhibit.file_path) {
            let mut block = String::with_capacity(768);
            append_admitted_exhibit_section(&mut block, exhibit, intake, workspace);
            rendered.push(block);
        }
    }
    rendered
}

fn pack_file_exhibit_slices<'a>(
    rendered_exhibits: &'a [String],
    body_budget: usize,
) -> Option<Vec<Vec<&'a str>>> {
    let mut groups: Vec<Vec<&'a str>> = Vec::new();
    let mut current: Vec<&'a str> = Vec::new();
    let mut current_len = 0usize;

    for block in rendered_exhibits {
        if block.len() > body_budget {
            return None;
        }
        if !current.is_empty() && current_len + block.len() > body_budget {
            groups.push(current);
            current = Vec::new();
            current_len = 0;
        }
        current_len += block.len();
        current.push(block.as_str());
    }
    if !current.is_empty() {
        groups.push(current);
    }
    Some(groups)
}
