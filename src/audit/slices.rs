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

//! Context-budgeted trial slicing and vet-cleared exhibit pruning for `audit` (`docs/architecture/nodes/audit.md` §3–§4).

use super::exhibits::{
    append_admitted_exhibit_section, append_admitted_exhibit_section_with_spans,
    append_grounding_references_section, is_metadata_plane_id, resolve_canon_reference_paths,
};
use super::prompt::{
    append_case_overview, append_governing_canon_statute, build_audit_case_user_prompt,
};
use super::slice_pack::{
    RenderedFileExhibit, format_pinned_reference_exhibits, pack_rendered_exhibit_slices,
};
use crate::canon::Canon;
use crate::caseload::{CaseloadDiscovery, CaseloadIntake, ExhibitAssessment};
use crate::discover::WorkspaceSnapshot;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// Default per-slice prompt character budget (~15k tokens) keeping generative reasoning latency bounded.
pub(crate) const MAX_AUDIT_TRIAL_CHARS: usize = 60_000;

pub(crate) const SLICE_FRAME_RESERVE_CHARS: usize = 512;

/// Prepared isolated prompt turn for one slice of a contested `audit` case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AuditTrialSlice {
    pub(super) user_prompt: String,
    pub(super) annotatable_file_paths: BTreeSet<String>,
    pub(super) primary_fallback_path: Option<String>,
}

/// Plans one or more context-budgeted `AuditTrialSlice` turns for a contested case, pruning
/// summarily vetted file exhibits when `contested_subset` identifies a proper subset of admitted files
/// and rendering Strategy D breadcrumb skeletons for files with entries in `contested_spans`.
#[allow(
    clippy::too_many_arguments,
    reason = "Internal trial slice planner combining case exhibits, optional vet-contested subset/spans, intake, discovery, and workspace"
)]
pub(super) fn build_audit_case_slices(
    canon_ref: &str,
    canon: &Canon,
    admitted_exhibits: &[&ExhibitAssessment],
    contested_subset: Option<&BTreeSet<String>>,
    contested_spans: &BTreeMap<String, Vec<(usize, usize)>>,
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
    workspace: &WorkspaceSnapshot,
    max_trial_chars: usize,
    diff_head_lines: usize,
) -> Vec<AuditTrialSlice> {
    let all_admitted_file_paths: BTreeSet<String> = admitted_exhibits
        .iter()
        .map(|e| e.file_path.as_str())
        .filter(|id| !is_metadata_plane_id(id))
        .map(ToString::to_string)
        .collect();

    let effective_contested =
        resolve_effective_contested_subset(contested_subset, &all_admitted_file_paths);

    let has_active_spans = all_admitted_file_paths
        .iter()
        .any(|p| contested_spans.get(p).is_some_and(|v| !v.is_empty()));

    if effective_contested.is_none() && !has_active_spans {
        let (full_prompt, full_paths) = build_audit_case_user_prompt(
            canon_ref,
            canon,
            admitted_exhibits,
            intake,
            discovery,
            workspace,
            diff_head_lines,
        );
        if full_prompt.len() <= max_trial_chars {
            let primary_fallback_path = full_paths.iter().next().cloned();
            return vec![AuditTrialSlice {
                user_prompt: full_prompt,
                annotatable_file_paths: full_paths,
                primary_fallback_path,
            }];
        }
    }

    let reference_paths = resolve_canon_reference_paths(canon, workspace);
    let (shared_header, pinned_annotatable_paths, sliceable_files) =
        prepare_audit_header_and_sliceable_files(
            canon_ref,
            canon,
            admitted_exhibits,
            &all_admitted_file_paths,
            effective_contested.as_ref(),
            contested_spans,
            &reference_paths,
            intake,
            discovery,
            workspace,
            max_trial_chars,
            diff_head_lines,
        );

    if sliceable_files.is_empty() {
        let mut single_prompt = shared_header;
        append_slice_admitted_file_paths(&mut single_prompt, &pinned_annotatable_paths);
        let primary_fallback_path = pinned_annotatable_paths.iter().next().cloned();
        return vec![AuditTrialSlice {
            user_prompt: single_prompt,
            annotatable_file_paths: pinned_annotatable_paths,
            primary_fallback_path,
        }];
    }

    let body_budget = max_trial_chars
        .saturating_sub(shared_header.len() + SLICE_FRAME_RESERVE_CHARS)
        .max(1);
    let groups = pack_rendered_exhibit_slices(&sliceable_files, body_budget, true)
        .unwrap_or_else(|| sliceable_files.iter().map(|item| vec![item]).collect());

    let total_slices = groups.len();
    let was_pruned = effective_contested.is_some();
    let total_admitted_files = all_admitted_file_paths.len();
    let vetted_pruned_count = total_admitted_files.saturating_sub(
        effective_contested
            .as_ref()
            .map_or(total_admitted_files, BTreeSet::len),
    );

    groups
        .into_iter()
        .enumerate()
        .map(|(idx, group)| {
            let mut slice_prompt =
                String::with_capacity(shared_header.len() + SLICE_FRAME_RESERVE_CHARS + 1024);
            slice_prompt.push_str(&shared_header);
            append_slice_exhibit_heading(
                &mut slice_prompt,
                idx + 1,
                total_slices,
                group.len(),
                was_pruned,
                total_admitted_files,
                vetted_pruned_count,
            );

            let mut annotatable_file_paths = pinned_annotatable_paths.clone();
            let primary_fallback_path = group.first().map(|item| item.file_path.clone());
            for item in group {
                slice_prompt.push_str(&item.rendered);
                annotatable_file_paths.insert(item.file_path.clone());
            }
            append_slice_admitted_file_paths(&mut slice_prompt, &annotatable_file_paths);

            AuditTrialSlice {
                user_prompt: slice_prompt,
                primary_fallback_path: primary_fallback_path
                    .or_else(|| annotatable_file_paths.iter().next().cloned()),
                annotatable_file_paths,
            }
        })
        .collect()
}

fn resolve_effective_contested_subset(
    contested_subset: Option<&BTreeSet<String>>,
    all_admitted_file_paths: &BTreeSet<String>,
) -> Option<BTreeSet<String>> {
    let subset = contested_subset?;
    if subset.is_empty() || all_admitted_file_paths.is_empty() {
        return None;
    }
    let matched: BTreeSet<String> = all_admitted_file_paths
        .intersection(subset)
        .cloned()
        .collect();
    if matched.is_empty() || matched.len() >= all_admitted_file_paths.len() {
        None
    } else {
        Some(matched)
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Internal helper assembling shared header, pinned companion references, and sliceable contested exhibits"
)]
fn prepare_audit_header_and_sliceable_files(
    canon_ref: &str,
    canon: &Canon,
    admitted_exhibits: &[&ExhibitAssessment],
    all_admitted_file_paths: &BTreeSet<String>,
    effective_contested: Option<&BTreeSet<String>>,
    contested_spans: &BTreeMap<String, Vec<(usize, usize)>>,
    reference_paths: &[String],
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
    workspace: &WorkspaceSnapshot,
    max_trial_chars: usize,
    diff_head_lines: usize,
) -> (String, BTreeSet<String>, Vec<RenderedFileExhibit>) {
    let mut header = build_shared_case_header(
        canon_ref,
        canon,
        admitted_exhibits,
        all_admitted_file_paths,
        reference_paths,
        intake,
        discovery,
        workspace,
    );
    let reference_set: BTreeSet<&str> = reference_paths.iter().map(String::as_str).collect();

    if let Some(contested_set) = effective_contested {
        let mut vet_cleared_refs = Vec::new();
        let mut contested_refs = Vec::new();
        let mut contested_non_refs = Vec::new();

        for exhibit in admitted_exhibits {
            if is_metadata_plane_id(&exhibit.file_path) {
                continue;
            }
            let is_ref = reference_set.contains(exhibit.file_path.as_str());
            let is_contested = contested_set.contains(&exhibit.file_path);
            if !is_contested && !is_ref {
                continue;
            }
            let spans = is_contested
                .then(|| contested_spans.get(&exhibit.file_path).map(Vec::as_slice))
                .flatten();
            let rendered = render_single_file_exhibit_with_spans(
                exhibit,
                intake,
                workspace,
                spans,
                diff_head_lines,
            );
            if is_contested && is_ref {
                contested_refs.push(rendered);
            } else if is_contested {
                contested_non_refs.push(rendered);
            } else {
                vet_cleared_refs.push(rendered);
            }
        }

        let pin_contested_refs = !contested_refs.is_empty() && !contested_non_refs.is_empty();
        let mut pinned_blocks: Vec<&RenderedFileExhibit> = vet_cleared_refs.iter().collect();
        if pin_contested_refs {
            pinned_blocks.extend(contested_refs.iter());
        }

        let mut pinned_annotatable = BTreeSet::new();
        if !pinned_blocks.is_empty() {
            let pinned_section = format_pinned_reference_exhibits(&pinned_blocks);
            if header.len() + pinned_section.len() <= (max_trial_chars * 3) / 4 {
                header.push_str(&pinned_section);
                if pin_contested_refs {
                    for item in &contested_refs {
                        pinned_annotatable.insert(item.file_path.clone());
                    }
                    return (header, pinned_annotatable, contested_non_refs);
                }
                let mut all_contested = contested_refs;
                all_contested.extend(contested_non_refs);
                all_contested.sort_by(|a, b| a.file_path.cmp(&b.file_path));
                return (header, pinned_annotatable, all_contested);
            }
        }

        let mut all_contested = contested_refs;
        all_contested.extend(contested_non_refs);
        all_contested.sort_by(|a, b| a.file_path.cmp(&b.file_path));
        return (header, pinned_annotatable, all_contested);
    }

    let (pinned_refs, non_ref_files) = partition_and_render_file_exhibits_with_spans(
        admitted_exhibits,
        &reference_set,
        contested_spans,
        intake,
        workspace,
        diff_head_lines,
    );
    if !pinned_refs.is_empty() && !non_ref_files.is_empty() {
        let refs_slice: Vec<&RenderedFileExhibit> = pinned_refs.iter().collect();
        let pinned_section = format_pinned_reference_exhibits(&refs_slice);
        if header.len() + pinned_section.len() <= max_trial_chars / 2 {
            header.push_str(&pinned_section);
            let pinned_annotatable = pinned_refs.into_iter().map(|r| r.file_path).collect();
            return (header, pinned_annotatable, non_ref_files);
        }
    }

    let all_files = render_admitted_file_exhibits_with_spans(
        admitted_exhibits,
        contested_spans,
        intake,
        workspace,
        diff_head_lines,
    );
    (header, BTreeSet::new(), all_files)
}

#[allow(
    clippy::too_many_arguments,
    reason = "Shared case header builder assembling overview, governing statute, references, metadata planes, and filing path manifest"
)]
pub(crate) fn build_shared_case_header(
    canon_ref: &str,
    canon: &Canon,
    admitted_exhibits: &[&ExhibitAssessment],
    all_admitted_file_paths: &BTreeSet<String>,
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
        all_admitted_file_paths,
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
            append_admitted_exhibit_section(&mut header, exhibit, intake, workspace, 0);
        }
    }

    let _ = writeln!(header, "## All Admitted File Exhibit Paths in Filing");
    if all_admitted_file_paths.is_empty() {
        let _ = writeln!(header, "(none — metadata-only case)\n");
    } else {
        for (idx, path) in all_admitted_file_paths.iter().enumerate() {
            let _ = writeln!(header, "{}. `{path}`", idx + 1);
        }
        header.push('\n');
    }
    header
}

fn partition_and_render_file_exhibits_with_spans(
    admitted_exhibits: &[&ExhibitAssessment],
    reference_set: &BTreeSet<&str>,
    contested_spans: &BTreeMap<String, Vec<(usize, usize)>>,
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
    diff_head_lines: usize,
) -> (Vec<RenderedFileExhibit>, Vec<RenderedFileExhibit>) {
    let mut pinned_refs = Vec::new();
    let mut non_ref_files = Vec::new();
    for exhibit in admitted_exhibits {
        if is_metadata_plane_id(&exhibit.file_path) {
            continue;
        }
        let spans = contested_spans.get(&exhibit.file_path).map(Vec::as_slice);
        let rendered = render_single_file_exhibit_with_spans(
            exhibit,
            intake,
            workspace,
            spans,
            diff_head_lines,
        );
        if reference_set.contains(exhibit.file_path.as_str()) {
            pinned_refs.push(rendered);
        } else {
            non_ref_files.push(rendered);
        }
    }
    (pinned_refs, non_ref_files)
}

fn render_admitted_file_exhibits_with_spans(
    admitted_exhibits: &[&ExhibitAssessment],
    contested_spans: &BTreeMap<String, Vec<(usize, usize)>>,
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
    diff_head_lines: usize,
) -> Vec<RenderedFileExhibit> {
    admitted_exhibits
        .iter()
        .filter(|exhibit| !is_metadata_plane_id(&exhibit.file_path))
        .map(|exhibit| {
            let spans = contested_spans.get(&exhibit.file_path).map(Vec::as_slice);
            render_single_file_exhibit_with_spans(
                exhibit,
                intake,
                workspace,
                spans,
                diff_head_lines,
            )
        })
        .collect()
}

fn render_single_file_exhibit_with_spans(
    exhibit: &ExhibitAssessment,
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
    hot_spans: Option<&[(usize, usize)]>,
    diff_head_lines: usize,
) -> RenderedFileExhibit {
    let mut rendered = String::with_capacity(768);
    append_admitted_exhibit_section_with_spans(
        &mut rendered,
        exhibit,
        intake,
        workspace,
        hot_spans,
        diff_head_lines,
    );
    RenderedFileExhibit {
        file_path: exhibit.file_path.clone(),
        rendered,
    }
}

pub(crate) fn partition_and_render_file_exhibits(
    admitted_exhibits: &[&ExhibitAssessment],
    reference_set: &BTreeSet<&str>,
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
    diff_head_lines: usize,
) -> (Vec<RenderedFileExhibit>, Vec<RenderedFileExhibit>) {
    partition_and_render_file_exhibits_with_spans(
        admitted_exhibits,
        reference_set,
        &BTreeMap::new(),
        intake,
        workspace,
        diff_head_lines,
    )
}

fn append_slice_exhibit_heading(
    out: &mut String,
    slice_number: usize,
    total_slices: usize,
    group_len: usize,
    was_pruned: bool,
    total_admitted_files: usize,
    vetted_pruned_count: usize,
) {
    if was_pruned && total_slices == 1 {
        let _ = writeln!(
            out,
            "## Admitted Exhibits (Contested Subset: {group_len} of {total_admitted_files} admitted file exhibit(s); {vetted_pruned_count} cleared by vet)\n"
        );
    } else if was_pruned {
        let _ = writeln!(
            out,
            "## Admitted Exhibits (Contested Trial Slice {slice_number} of {total_slices}, {group_len} file exhibit(s); {vetted_pruned_count} cleared by vet)\n"
        );
    } else {
        let _ = writeln!(
            out,
            "## Admitted Exhibits (Trial Slice {slice_number} of {total_slices}, {group_len} file exhibit(s))\n"
        );
    }
}

fn append_slice_admitted_file_paths(out: &mut String, annotatable_file_paths: &BTreeSet<String>) {
    let _ = writeln!(out, "## Admitted File Exhibit Paths");
    if annotatable_file_paths.is_empty() {
        let _ = writeln!(out, "(none — metadata-only case; emit `annotations: []`)");
    } else {
        for (idx, path) in annotatable_file_paths.iter().enumerate() {
            let _ = writeln!(out, "{}. `{path}`", idx + 1);
        }
    }
}
