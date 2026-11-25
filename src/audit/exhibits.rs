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

//! Admitted exhibit line-annotated rendering and statutory `references:` resolution for `audit` (`docs/architecture/nodes/audit.md` §4).

use super::metadata::{
    append_commit_messages_exhibit, append_linked_issues_exhibit, append_pr_body_exhibit,
    append_pr_title_exhibit,
};
use crate::canon::Canon;
use crate::caseload::{
    CaseloadIntake, ExhibitAssessment, FileArtifact, FileChangeStatus, WorkspaceState,
};
use crate::discover::{GlobPattern, WorkspaceSnapshot, normalize_workspace_path};
use crate::spans::{
    ExhibitLine, expand_diff_patch_lines, format_diff_patch_with_head_context,
    parse_snapshot_lines, render_breadcrumb_skeleton, render_exhibit_lines,
};
use std::collections::BTreeSet;
use std::fmt::Write as _;

pub(super) const MAX_AUDIT_EXHIBIT_CHARS: usize = 24_576;
const MAX_AUDIT_REFERENCE_CHARS: usize = 12_288;

/// Returns `true` if `exhibit_id` names a reserved filing metadata plane rather than a workspace file path.
pub(crate) fn is_metadata_plane_id(exhibit_id: &str) -> bool {
    matches!(
        exhibit_id,
        "pr_title" | "pr_body" | "linked_issues" | "commit_messages"
    )
}

/// Resolves workspace-relative POSIX file paths matching a canon's frontmatter `references:` globs
/// in deterministic lexicographical order.
pub fn resolve_canon_reference_paths(canon: &Canon, workspace: &WorkspaceSnapshot) -> Vec<String> {
    if canon.references.is_empty() {
        return Vec::new();
    }

    let mut matchers = Vec::new();
    for raw_pattern in &canon.references {
        let trimmed = raw_pattern.trim();
        if trimmed.is_empty() {
            continue;
        }
        let candidate = normalize_workspace_path(trimmed).unwrap_or_else(|_| trimmed.to_string());
        if let Ok(glob) = GlobPattern::parse(&candidate) {
            matchers.push(glob);
        }
    }
    if matchers.is_empty() {
        return Vec::new();
    }

    let mut matched = BTreeSet::new();
    for path in workspace
        .existing_files
        .iter()
        .chain(workspace.file_contents.keys())
    {
        if matchers.iter().any(|m| m.is_match(path)) {
            matched.insert(path.clone());
        }
    }
    matched.into_iter().collect()
}

/// Resolves post-patch UTF-8 file content for a modified, renamed, or copied diff artifact when
/// safely available (never returning unpatched base-branch file contents under `--workspace-state base`).
pub(crate) fn resolve_diff_post_patch_content<'a>(
    artifact: &'a FileArtifact,
    intake: &CaseloadIntake,
    workspace: &'a WorkspaceSnapshot,
) -> Option<&'a str> {
    if !matches!(
        artifact.status,
        FileChangeStatus::Modified | FileChangeStatus::Renamed | FileChangeStatus::Copied
    ) {
        return None;
    }
    non_blank(artifact.content.as_deref()).or_else(|| {
        let safe_to_read_snapshot = intake.workspace_state == WorkspaceState::Head
            || workspace.applied_base_diffs.contains(&artifact.path);
        if safe_to_read_snapshot {
            workspace
                .file_contents
                .get(&artifact.path)
                .and_then(|s| non_blank(Some(s.as_str())))
        } else {
            None
        }
    })
}

/// Formats a unified diff artifact's patch with head-context budget expansion (`docs/architecture/overview.md` §9).
pub(crate) fn format_diff_artifact_patch(
    artifact: &FileArtifact,
    patch: &str,
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
    diff_head_lines: usize,
) -> String {
    let post_patch = resolve_diff_post_patch_content(artifact, intake, workspace);
    format_diff_patch_with_head_context(&artifact.path, patch, post_patch, diff_head_lines)
}

/// Renders a single admitted exhibit (`ExhibitAssessment`) into `out` with 1-based line numbers for file exhibits.
pub(crate) fn append_admitted_exhibit_section(
    out: &mut String,
    exhibit: &ExhibitAssessment,
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
    diff_head_lines: usize,
) {
    append_admitted_exhibit_section_with_spans(
        out,
        exhibit,
        intake,
        workspace,
        None,
        diff_head_lines,
    );
}

/// Renders a single admitted exhibit, applying Strategy D breadcrumb skeleton elision for file
/// exhibits when `hot_spans` is non-empty (`docs/architecture/nodes/audit.md` §3–§4).
pub(crate) fn append_admitted_exhibit_section_with_spans(
    out: &mut String,
    exhibit: &ExhibitAssessment,
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
    hot_spans: Option<&[(usize, usize)]>,
    diff_head_lines: usize,
) {
    match exhibit.file_path.as_str() {
        "pr_title" => append_pr_title_exhibit(out, exhibit, intake),
        "pr_body" => append_pr_body_exhibit(out, exhibit, intake),
        "linked_issues" => append_linked_issues_exhibit(out, exhibit, intake),
        "commit_messages" => append_commit_messages_exhibit(out, exhibit, intake),
        file_path => {
            append_admitted_file_exhibit(
                out,
                file_path,
                exhibit,
                intake,
                workspace,
                hot_spans,
                diff_head_lines,
            );
        }
    }
}

/// Extracts the untruncated 1-based [`ExhibitLine`] stream for an admitted file exhibit,
/// returning `(is_diff_patch, lines)` when textual content or patch hunks are available.
pub(crate) fn extract_file_exhibit_lines(
    file_path: &str,
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
    diff_head_lines: usize,
) -> Option<(bool, Vec<ExhibitLine>)> {
    if let Some(artifact) = intake.diffs.get(file_path) {
        if let Some(patch) = non_blank(artifact.patch.as_deref()) {
            let post_patch = resolve_diff_post_patch_content(artifact, intake, workspace);
            return Some((
                true,
                expand_diff_patch_lines(&artifact.path, patch, post_patch, diff_head_lines),
            ));
        }
        let content = non_blank(artifact.content.as_deref()).or_else(|| {
            workspace
                .file_contents
                .get(&artifact.path)
                .and_then(|s| non_blank(Some(s.as_str())))
        });
        return content.map(|text| (false, parse_snapshot_lines(text)));
    }

    workspace
        .file_contents
        .get(file_path)
        .and_then(|s| non_blank(Some(s.as_str())))
        .map(|text| (false, parse_snapshot_lines(text)))
}

/// Formats the standard `### Admitted File Exhibit: ...` heading line for a file exhibit.
pub(crate) fn format_file_exhibit_heading(
    file_path: &str,
    exhibit: &ExhibitAssessment,
    intake: &CaseloadIntake,
) -> String {
    if let Some(artifact) = intake.diffs.get(file_path) {
        match &artifact.previous_path {
            Some(prev) => format!(
                "### Admitted File Exhibit: `{}` ({} from `{}`, +{} / -{}, score: {:.2})",
                artifact.path,
                artifact.status.as_str(),
                prev,
                artifact.lines_added,
                artifact.lines_deleted,
                exhibit.admissibility_score
            ),
            None => format!(
                "### Admitted File Exhibit: `{}` ({}, +{} / -{}, score: {:.2})",
                artifact.path,
                artifact.status.as_str(),
                artifact.lines_added,
                artifact.lines_deleted,
                exhibit.admissibility_score
            ),
        }
    } else {
        format!(
            "### Admitted File Exhibit: `{file_path}` (Target Workspace File, score: {:.2})",
            exhibit.admissibility_score
        )
    }
}

fn append_admitted_file_exhibit(
    out: &mut String,
    file_path: &str,
    exhibit: &ExhibitAssessment,
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
    hot_spans: Option<&[(usize, usize)]>,
    diff_head_lines: usize,
) {
    let active_spans = hot_spans.filter(|spans| !spans.is_empty());
    if let Some(artifact) = intake.diffs.get(file_path) {
        append_diff_file_exhibit(
            out,
            artifact,
            exhibit,
            intake,
            workspace,
            active_spans,
            diff_head_lines,
        );
    } else {
        let _ = writeln!(
            out,
            "### Admitted File Exhibit: `{file_path}` (Target Workspace File, score: {:.2})",
            exhibit.admissibility_score
        );
        if let Some(content) = workspace
            .file_contents
            .get(file_path)
            .and_then(|s| non_blank(Some(s.as_str())))
        {
            let body = match active_spans {
                Some(spans) => {
                    let lines = parse_snapshot_lines(content);
                    render_breadcrumb_skeleton(file_path, &lines, spans)
                }
                None => annotate_snapshot_lines(content),
            };
            let bounded = truncate_chars(&body, MAX_AUDIT_EXHIBIT_CHARS, "file snapshot");
            let _ = writeln!(out, "```text\n{bounded}\n```");
        } else {
            let _ = writeln!(
                out,
                "(Target file materialized in workspace scope; anchor file-level annotations at line 1.)"
            );
        }
    }
    out.push('\n');
}

fn append_diff_file_exhibit(
    out: &mut String,
    artifact: &FileArtifact,
    exhibit: &ExhibitAssessment,
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
    hot_spans: Option<&[(usize, usize)]>,
    diff_head_lines: usize,
) {
    match &artifact.previous_path {
        Some(prev) => {
            let _ = writeln!(
                out,
                "### Admitted File Exhibit: `{}` ({} from `{}`, +{} / -{}, score: {:.2})",
                artifact.path,
                artifact.status.as_str(),
                prev,
                artifact.lines_added,
                artifact.lines_deleted,
                exhibit.admissibility_score
            );
        }
        None => {
            let _ = writeln!(
                out,
                "### Admitted File Exhibit: `{}` ({}, +{} / -{}, score: {:.2})",
                artifact.path,
                artifact.status.as_str(),
                artifact.lines_added,
                artifact.lines_deleted,
                exhibit.admissibility_score
            );
        }
    }

    if let Some(patch) = non_blank(artifact.patch.as_deref()) {
        let post_patch = resolve_diff_post_patch_content(artifact, intake, workspace);
        let lines = expand_diff_patch_lines(&artifact.path, patch, post_patch, diff_head_lines);
        let body = match hot_spans {
            Some(spans) => render_breadcrumb_skeleton(&artifact.path, &lines, spans),
            None => render_exhibit_lines(&lines),
        };
        let bounded = truncate_chars(&body, MAX_AUDIT_EXHIBIT_CHARS, "diff patch");
        let _ = writeln!(out, "```diff\n{bounded}\n```");
    } else if let Some(content) = non_blank(artifact.content.as_deref()).or_else(|| {
        workspace
            .file_contents
            .get(&artifact.path)
            .and_then(|s| non_blank(Some(s.as_str())))
    }) {
        let body = match hot_spans {
            Some(spans) => {
                let lines = parse_snapshot_lines(content);
                render_breadcrumb_skeleton(&artifact.path, &lines, spans)
            }
            None => annotate_snapshot_lines(content),
        };
        let bounded = truncate_chars(&body, MAX_AUDIT_EXHIBIT_CHARS, "file snapshot");
        let _ = writeln!(out, "```text\n{bounded}\n```");
    } else if let Some(reason) = artifact.patch_omission_reason {
        let _ = writeln!(
            out,
            "(Diff patch omitted: {:?}; anchor file-level annotations at line 1.)",
            reason
        );
    }
}

/// Appends resolved static `references:` files as authoritative statutory grounding context.
pub(crate) fn append_grounding_references_section(
    out: &mut String,
    reference_paths: &[String],
    admitted_file_paths: &BTreeSet<String>,
    workspace: &WorkspaceSnapshot,
) {
    if reference_paths.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "## Statutory Grounding References ({})\n\n\
         *(Authoritative workspace reference files declared in canon `references:`. \
         Do NOT emit `annotations` against reference paths unless they also appear in `## Admitted File Exhibit Paths`.)*\n",
        reference_paths.len()
    );
    for ref_path in reference_paths {
        if admitted_file_paths.contains(ref_path) {
            let _ = writeln!(
                out,
                "### Reference: `{ref_path}` *(also admitted above as a filing exhibit)*\n"
            );
            continue;
        }
        let _ = writeln!(out, "### Reference: `{ref_path}`");
        if let Some(content) = workspace
            .file_contents
            .get(ref_path)
            .and_then(|s| non_blank(Some(s.as_str())))
        {
            let bounded = truncate_chars(content, MAX_AUDIT_REFERENCE_CHARS, "reference file");
            let _ = writeln!(out, "```text\n{bounded}\n```\n");
        } else {
            let _ = writeln!(out, "(Present in workspace at `{ref_path}`.)\n");
        }
    }
}

/// Annotates full-file UTF-8 snapshots with 1-based line numbers (`   1 | ...`).
pub(crate) fn annotate_snapshot_lines(content: &str) -> String {
    let mut out = String::with_capacity(content.len() + 128);
    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        let _ = writeln!(out, "{line_no:>4} | {line}");
    }
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

pub(crate) fn non_blank(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|s| !s.is_empty())
}

pub(super) fn truncate_chars(text: &str, max_chars: usize, label: &str) -> String {
    let mut char_indices = text.char_indices();
    match char_indices.nth(max_chars) {
        Some((byte_idx, _)) => format!(
            "{}\n... [{label} truncated for trial prompt]",
            &text[..byte_idx]
        ),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_diff_post_patch_content_guards_base_workspace_state() {
        let artifact = FileArtifact {
            path: "src/lib.rs".to_string(),
            status: FileChangeStatus::Modified,
            previous_path: None,
            lines_added: 1,
            lines_deleted: 1,
            patch: Some("@@ -5,1 +5,1 @@\n-old\n+new".to_string()),
            patch_omission_reason: None,
            content: None,
            content_omission_reason: None,
        };
        let ws = WorkspaceSnapshot::new().with_file_content("src/lib.rs", "base content\n");

        // Under Head state, workspace.file_contents is trusted as post-patch.
        let head_intake = CaseloadIntake {
            workspace_state: WorkspaceState::Head,
            ..CaseloadIntake::default()
        };
        assert_eq!(
            resolve_diff_post_patch_content(&artifact, &head_intake, &ws),
            Some("base content")
        );

        // Under Base state without an applied overlay, unpatched base content MUST NOT leak.
        let base_intake = CaseloadIntake {
            workspace_state: WorkspaceState::Base,
            ..CaseloadIntake::default()
        };
        assert_eq!(
            resolve_diff_post_patch_content(&artifact, &base_intake, &ws),
            None
        );

        // Once applied_base_diffs records the path, the overlaid post-patch content is returned.
        let mut overlaid_ws = ws;
        overlaid_ws
            .file_contents
            .insert("src/lib.rs".to_string(), "post-patch content\n".to_string());
        overlaid_ws
            .applied_base_diffs
            .insert("src/lib.rs".to_string());
        assert_eq!(
            resolve_diff_post_patch_content(&artifact, &base_intake, &overlaid_ws),
            Some("post-patch content")
        );

        // Added/Deleted artifacts are no-ops for head-context lookup.
        let added = FileArtifact {
            status: FileChangeStatus::Added,
            ..artifact
        };
        assert_eq!(
            resolve_diff_post_patch_content(&added, &head_intake, &overlaid_ws),
            None
        );
    }
}
