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

//! Context-budgeted multi-slice partitioning and topological span fencing for discriminative `vet`
//! (`docs/architecture/nodes/vet.md` §4.3).

use super::decision::{max_discriminative_state_chars_for_profile, slice_fits_single_batch};
use crate::audit::{
    RenderedFileExhibit, build_shared_case_header, extract_file_exhibit_lines,
    format_file_exhibit_heading, format_pinned_reference_exhibits, is_metadata_plane_id,
    pack_rendered_exhibit_slices, partition_and_render_file_exhibits,
    resolve_canon_reference_paths,
};
use crate::canon::Canon;
use crate::caseload::{CaseloadDiscovery, CaseloadIntake, ExhibitAssessment};
use crate::discover::WorkspaceSnapshot;
use crate::provider::DecisionBudgetProfile;
use crate::spans::{ExhibitLine, TiledSpan, render_fenced_spans, tile_exhibit_lines};
use std::collections::BTreeSet;
use std::fmt::Write as _;

const SLICE_FRAME_RESERVE_CHARS: usize = 256;
const SPAN_FENCE_OVERHEAD_PER_SPAN: usize = 96;

/// One topologically tiled span fenced inside a discriminative `vet` slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VettingSpanTarget {
    pub(super) span_id: String,
    pub(super) file_path: String,
    pub(super) start_line: usize,
    pub(super) end_line: usize,
}

/// One context-budgeted discriminative `state` slice paired with the sliceable file paths and
/// localized span targets fenced inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VettingExhibitSlice {
    pub(super) state: String,
    pub(super) file_paths: Vec<String>,
    pub(super) spans: Vec<VettingSpanTarget>,
}

/// Execution plan for evaluating one active case within a discriminative context ceiling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DiscriminativeCasePlan {
    /// One or more context-budgeted `state` slices whose conjunctive minimum score governs the case.
    Slices(Vec<VettingExhibitSlice>),
    /// Single exhibit or statutory reference set exceeds the discriminative window; escalate as `contested`.
    OversizedContest,
}

struct PreparedFileExhibit {
    file_path: String,
    heading: String,
    fence_lang: &'static str,
    lines: Vec<ExhibitLine>,
    tiled_spans: Vec<TiledSpan>,
    estimated_chars: usize,
    estimated_unfenced_chars: usize,
    fallback_body: Option<String>,
}

impl PreparedFileExhibit {
    fn from_exhibit(
        exhibit: &ExhibitAssessment,
        intake: &CaseloadIntake,
        workspace: &WorkspaceSnapshot,
    ) -> Self {
        let file_path = exhibit.file_path.clone();
        let heading = format_file_exhibit_heading(&file_path, exhibit, intake);
        if let Some((is_diff, lines)) = extract_file_exhibit_lines(&file_path, intake, workspace) {
            let tiled_spans = tile_exhibit_lines(&file_path, &lines);
            let lines_chars: usize = lines.iter().map(|l| l.rendered.len() + 1).sum();
            let fence_overhead = if tiled_spans.len() > 1 {
                tiled_spans
                    .len()
                    .saturating_mul(SPAN_FENCE_OVERHEAD_PER_SPAN)
            } else {
                0
            };
            let estimated_unfenced_chars = heading.len() + lines_chars + 32;
            let estimated_chars = estimated_unfenced_chars + fence_overhead;
            Self {
                file_path,
                heading,
                fence_lang: if is_diff { "diff" } else { "text" },
                lines,
                tiled_spans,
                estimated_chars,
                estimated_unfenced_chars,
                fallback_body: None,
            }
        } else {
            let fallback = fallback_omission_note(&file_path, intake);
            let estimated_chars = heading.len() + fallback.len() + 8;
            Self {
                file_path,
                heading,
                fence_lang: "text",
                lines: Vec::new(),
                tiled_spans: Vec::new(),
                estimated_chars,
                estimated_unfenced_chars: estimated_chars,
                fallback_body: Some(fallback),
            }
        }
    }

    fn span_noul_count(&self) -> usize {
        if self.tiled_spans.len() > 1 {
            self.tiled_spans.len()
        } else {
            0
        }
    }

    fn render_into(
        &self,
        out: &mut String,
        next_span_num: &mut usize,
        include_spans: bool,
    ) -> Vec<VettingSpanTarget> {
        let _ = writeln!(out, "{}", self.heading);
        if let Some(ref note) = self.fallback_body {
            let _ = writeln!(out, "{note}\n");
            return Vec::new();
        }
        if include_spans && self.tiled_spans.len() > 1 {
            let (fenced, descs) = render_fenced_spans(
                &self.file_path,
                &self.lines,
                &self.tiled_spans,
                next_span_num,
            );
            let _ = writeln!(out, "```{}\n{fenced}\n```\n", self.fence_lang);
            descs
                .into_iter()
                .map(|(span_id, start_line, end_line)| VettingSpanTarget {
                    span_id,
                    file_path: self.file_path.clone(),
                    start_line,
                    end_line,
                })
                .collect()
        } else {
            let mut body = String::with_capacity(self.estimated_unfenced_chars);
            for line in &self.lines {
                let _ = writeln!(body, "{}", line.rendered);
            }
            if body.ends_with('\n') {
                body.pop();
            }
            let _ = writeln!(out, "```{}\n{body}\n```\n", self.fence_lang);
            Vec::new()
        }
    }
}

fn fallback_omission_note(file_path: &str, intake: &CaseloadIntake) -> String {
    if let Some(artifact) = intake.diffs.get(file_path)
        && let Some(reason) = artifact.patch_omission_reason
    {
        return format!(
            "(Diff patch omitted: {reason:?}; anchor file-level annotations at line 1.)"
        );
    }
    "(Target file materialized in workspace scope; anchor file-level annotations at line 1.)"
        .to_string()
}

/// Builds a single-call or multi-slice discriminative `state` plan with adaptive topological span
/// fencing, bounded by `profile`'s single-batch `Noul` and character budgets.
#[allow(
    clippy::too_many_arguments,
    reason = "Internal slice planner assembling case overview, canon statute, references, and fenced admitted exhibits"
)]
pub(super) fn build_discriminative_case_plan(
    canon_ref: &str,
    canon: &Canon,
    admitted_exhibits: &[&ExhibitAssessment],
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
    workspace: &WorkspaceSnapshot,
    profile: DecisionBudgetProfile,
) -> DiscriminativeCasePlan {
    let max_state_chars = max_discriminative_state_chars_for_profile(profile);
    let admitted_file_paths: BTreeSet<String> = admitted_exhibits
        .iter()
        .map(|e| e.file_path.as_str())
        .filter(|id| !is_metadata_plane_id(id))
        .map(ToString::to_string)
        .collect();

    let reference_paths = resolve_canon_reference_paths(canon, workspace);
    let base_header = build_shared_case_header(
        canon_ref,
        canon,
        admitted_exhibits,
        &admitted_file_paths,
        &reference_paths,
        intake,
        discovery,
        workspace,
    );

    let all_prepared: Vec<PreparedFileExhibit> = admitted_exhibits
        .iter()
        .filter(|e| !is_metadata_plane_id(&e.file_path))
        .map(|e| PreparedFileExhibit::from_exhibit(e, intake, workspace))
        .collect();

    if all_prepared.is_empty() {
        return if base_header.len() <= max_state_chars {
            DiscriminativeCasePlan::Slices(vec![VettingExhibitSlice {
                state: base_header,
                file_paths: Vec::new(),
                spans: Vec::new(),
            }])
        } else {
            DiscriminativeCasePlan::OversizedContest
        };
    }

    let total_all_spans: usize = all_prepared
        .iter()
        .map(PreparedFileExhibit::span_noul_count)
        .sum();
    let single_slice_body_est: usize =
        if 2 * (1 + all_prepared.len() + total_all_spans) <= profile.max_nouls_per_call {
            all_prepared.iter().map(|p| p.estimated_chars).sum()
        } else {
            all_prepared
                .iter()
                .map(|p| p.estimated_unfenced_chars)
                .sum()
        };
    if base_header.len() + SLICE_FRAME_RESERVE_CHARS + single_slice_body_est <= max_state_chars {
        let slice = assemble_slice(&base_header, &all_prepared, 0, 1, profile, max_state_chars);
        if slice.state.len() <= max_state_chars {
            return DiscriminativeCasePlan::Slices(vec![slice]);
        }
    }

    // Multi-slice path: pin admitted reference diffs in the shared header when budget permits.
    let (shared_header, sliceable_prepared) = prepare_multi_slice_header_and_files(
        base_header,
        admitted_exhibits,
        &reference_paths,
        all_prepared,
        intake,
        workspace,
        max_state_chars,
    );

    if shared_header.len() + SLICE_FRAME_RESERVE_CHARS >= max_state_chars {
        return DiscriminativeCasePlan::OversizedContest;
    }
    let body_budget = max_state_chars - shared_header.len() - SLICE_FRAME_RESERVE_CHARS;

    let size_proxies: Vec<RenderedFileExhibit> = sliceable_prepared
        .iter()
        .enumerate()
        .map(|(idx, item)| RenderedFileExhibit {
            file_path: idx.to_string(),
            rendered: " ".repeat(item.estimated_chars),
        })
        .collect();

    let Some(groups) = pack_rendered_exhibit_slices(&size_proxies, body_budget, false) else {
        return DiscriminativeCasePlan::OversizedContest;
    };

    let total_slices = groups.len();
    let mut slices = Vec::with_capacity(total_slices);
    for (slice_idx, group) in groups.into_iter().enumerate() {
        let group_items: Vec<&PreparedFileExhibit> = group
            .into_iter()
            .filter_map(|proxy| proxy.file_path.parse::<usize>().ok())
            .filter_map(|i| sliceable_prepared.get(i))
            .collect();
        let slice = assemble_slice_refs(
            &shared_header,
            &group_items,
            slice_idx,
            total_slices,
            profile,
            max_state_chars,
        );
        if slice.state.len() > max_state_chars {
            return DiscriminativeCasePlan::OversizedContest;
        }
        slices.push(slice);
    }

    DiscriminativeCasePlan::Slices(slices)
}

fn prepare_multi_slice_header_and_files(
    mut header: String,
    admitted_exhibits: &[&ExhibitAssessment],
    reference_paths: &[String],
    all_prepared: Vec<PreparedFileExhibit>,
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
    max_state_chars: usize,
) -> (String, Vec<PreparedFileExhibit>) {
    let reference_set: BTreeSet<&str> = reference_paths.iter().map(String::as_str).collect();
    let (pinned_refs, non_ref_files) =
        partition_and_render_file_exhibits(admitted_exhibits, &reference_set, intake, workspace);

    if !pinned_refs.is_empty() && !non_ref_files.is_empty() {
        let refs_slice: Vec<&RenderedFileExhibit> = pinned_refs.iter().collect();
        let pinned_section = format_pinned_reference_exhibits(&refs_slice);
        if header.len() + pinned_section.len() <= max_state_chars / 2 {
            header.push_str(&pinned_section);
            let non_ref_prepared: Vec<PreparedFileExhibit> = all_prepared
                .into_iter()
                .filter(|item| !reference_set.contains(item.file_path.as_str()))
                .collect();
            return (header, non_ref_prepared);
        }
    }

    (header, all_prepared)
}

fn assemble_slice(
    shared_header: &str,
    items: &[PreparedFileExhibit],
    slice_idx: usize,
    total_slices: usize,
    profile: DecisionBudgetProfile,
    max_state_chars: usize,
) -> VettingExhibitSlice {
    let refs: Vec<&PreparedFileExhibit> = items.iter().collect();
    assemble_slice_refs(
        shared_header,
        &refs,
        slice_idx,
        total_slices,
        profile,
        max_state_chars,
    )
}

fn assemble_slice_refs(
    shared_header: &str,
    items: &[&PreparedFileExhibit],
    slice_idx: usize,
    total_slices: usize,
    profile: DecisionBudgetProfile,
    max_state_chars: usize,
) -> VettingExhibitSlice {
    let span_count: usize = items.iter().map(|i| i.span_noul_count()).sum();
    if span_count > 0 && 2 * (1 + items.len() + span_count) <= profile.max_nouls_per_call {
        let fenced = render_slice_refs(shared_header, items, slice_idx, total_slices, true);
        if fenced.state.len() <= max_state_chars && slice_fits_single_batch(&fenced, profile) {
            return fenced;
        }
    }
    render_slice_refs(shared_header, items, slice_idx, total_slices, false)
}

fn render_slice_refs(
    shared_header: &str,
    items: &[&PreparedFileExhibit],
    slice_idx: usize,
    total_slices: usize,
    include_spans: bool,
) -> VettingExhibitSlice {
    let body_est: usize = if include_spans {
        items.iter().map(|i| i.estimated_chars).sum()
    } else {
        items.iter().map(|i| i.estimated_unfenced_chars).sum()
    };
    let mut state =
        String::with_capacity(shared_header.len() + body_est + SLICE_FRAME_RESERVE_CHARS);
    state.push_str(shared_header);
    if total_slices > 1 {
        let _ = writeln!(
            state,
            "## Admitted Exhibits (Slice {} of {}, {} file exhibit(s))\n",
            slice_idx + 1,
            total_slices,
            items.len()
        );
    } else {
        let _ = writeln!(
            state,
            "## Admitted Exhibits ({} file exhibit(s))\n",
            items.len()
        );
    }

    let mut file_paths = Vec::with_capacity(items.len());
    let mut spans = Vec::new();
    let mut next_span_num = 1usize;
    for item in items {
        let file_spans = item.render_into(&mut state, &mut next_span_num, include_spans);
        file_paths.push(item.file_path.clone());
        spans.extend(file_spans);
    }

    VettingExhibitSlice {
        state,
        file_paths,
        spans,
    }
}
