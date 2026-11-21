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

use crate::canon::Canon;
use crate::caseload::{CaseloadIntake, ExhibitAssessment, FileArtifact, LinkedIssueContext};
use crate::discover::{GlobPattern, WorkspaceSnapshot, normalize_workspace_path};
use crate::spans::{
    ExhibitLine, parse_snapshot_lines, parse_unified_patch_lines, render_breadcrumb_skeleton,
};
use std::collections::BTreeSet;
use std::fmt::Write as _;

const MAX_AUDIT_EXHIBIT_CHARS: usize = 24_576;
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

/// Renders a single admitted exhibit (`ExhibitAssessment`) into `out` with 1-based line numbers for file exhibits.
pub(crate) fn append_admitted_exhibit_section(
    out: &mut String,
    exhibit: &ExhibitAssessment,
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
) {
    append_admitted_exhibit_section_with_spans(out, exhibit, intake, workspace, None);
}

/// Renders a single admitted exhibit, applying Strategy D breadcrumb skeleton elision for file
/// exhibits when `hot_spans` is non-empty (`docs/architecture/nodes/audit.md` §3–§4).
pub(crate) fn append_admitted_exhibit_section_with_spans(
    out: &mut String,
    exhibit: &ExhibitAssessment,
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
    hot_spans: Option<&[(usize, usize)]>,
) {
    match exhibit.file_path.as_str() {
        "pr_title" => append_pr_title_exhibit(out, exhibit, intake),
        "pr_body" => append_pr_body_exhibit(out, exhibit, intake),
        "linked_issues" => append_linked_issues_exhibit(out, exhibit, intake),
        "commit_messages" => append_commit_messages_exhibit(out, exhibit, intake),
        file_path => {
            append_admitted_file_exhibit(out, file_path, exhibit, intake, workspace, hot_spans);
        }
    }
}

/// Extracts the untruncated 1-based [`ExhibitLine`] stream for an admitted file exhibit,
/// returning `(is_diff_patch, lines)` when textual content or patch hunks are available.
pub(crate) fn extract_file_exhibit_lines(
    file_path: &str,
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
) -> Option<(bool, Vec<ExhibitLine>)> {
    if let Some(artifact) = intake.diffs.get(file_path) {
        if let Some(patch) = non_blank(artifact.patch.as_deref()) {
            return Some((true, parse_unified_patch_lines(patch)));
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

fn append_commit_messages_exhibit(
    out: &mut String,
    exhibit: &ExhibitAssessment,
    intake: &CaseloadIntake,
) {
    let _ = writeln!(
        out,
        "### Admitted Metadata Exhibit: `commit_messages` (score: {:.2})",
        exhibit.admissibility_score
    );
    if let Some(title) = non_blank(intake.pr_title.as_deref()) {
        let _ = writeln!(out, "{title}");
    }
    if let Some(body) = non_blank(intake.pr_body.as_deref()) {
        let bounded = truncate_chars(body, MAX_AUDIT_EXHIBIT_CHARS, "commit messages");
        let _ = writeln!(out, "{bounded}");
    }
    out.push('\n');
}

fn append_pr_title_exhibit(out: &mut String, exhibit: &ExhibitAssessment, intake: &CaseloadIntake) {
    let _ = writeln!(
        out,
        "### Admitted Metadata Exhibit: `pr_title` (score: {:.2})",
        exhibit.admissibility_score
    );
    if let Some(title) = non_blank(intake.pr_title.as_deref()) {
        let _ = writeln!(out, "{title}");
    }
    out.push('\n');
}

fn append_pr_body_exhibit(out: &mut String, exhibit: &ExhibitAssessment, intake: &CaseloadIntake) {
    let _ = writeln!(
        out,
        "### Admitted Metadata Exhibit: `pr_body` (score: {:.2})",
        exhibit.admissibility_score
    );
    if let Some(body) = non_blank(intake.pr_body.as_deref()) {
        let bounded = truncate_chars(body, MAX_AUDIT_EXHIBIT_CHARS, "PR description");
        let _ = writeln!(out, "{bounded}");
    }
    out.push('\n');
}

fn append_linked_issues_exhibit(
    out: &mut String,
    exhibit: &ExhibitAssessment,
    intake: &CaseloadIntake,
) {
    let _ = writeln!(
        out,
        "### Admitted Metadata Exhibit: `linked_issues` (score: {:.2})",
        exhibit.admissibility_score
    );
    if let Some(issues) = intake.linked_issues.as_deref().filter(|v| !v.is_empty()) {
        append_linked_issues(out, issues);
    }
    out.push('\n');
}

fn append_linked_issues(out: &mut String, issues: &[LinkedIssueContext]) {
    for issue in issues {
        let title = non_blank(issue.title.as_deref()).unwrap_or("(untitled)");
        let meta = format_linked_issue_metadata(issue);
        let _ = writeln!(out, "- Issue #{}: {title}{meta}", issue.number);
        if let Some(body) = non_blank(issue.body.as_deref()) {
            let bounded = truncate_chars(body, MAX_AUDIT_EXHIBIT_CHARS / 2, "issue body");
            let _ = writeln!(out, "  {bounded}");
        }
    }
}

fn format_linked_issue_metadata(issue: &LinkedIssueContext) -> String {
    let mut parts = Vec::new();
    if let Some(author) = non_blank(issue.author.as_deref()) {
        parts.push(format!("author: @{author}"));
    }
    let labels: Vec<&str> = issue
        .labels
        .iter()
        .filter_map(|label| non_blank(Some(label.as_str())))
        .collect();
    if !labels.is_empty() {
        parts.push(format!("labels: {}", labels.join(", ")));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" ({})", parts.join("; "))
    }
}

fn append_admitted_file_exhibit(
    out: &mut String,
    file_path: &str,
    exhibit: &ExhibitAssessment,
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
    hot_spans: Option<&[(usize, usize)]>,
) {
    let active_spans = hot_spans.filter(|spans| !spans.is_empty());
    if let Some(artifact) = intake.diffs.get(file_path) {
        append_diff_file_exhibit(out, artifact, exhibit, workspace, active_spans);
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
    workspace: &WorkspaceSnapshot,
    hot_spans: Option<&[(usize, usize)]>,
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
        let body = match hot_spans {
            Some(spans) => {
                let lines = parse_unified_patch_lines(patch);
                render_breadcrumb_skeleton(&artifact.path, &lines, spans)
            }
            None => annotate_unified_patch_lines(patch),
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

/// Annotates unified diff hunks with explicit 1-based target-file line numbers (`+  42 | ...`).
pub(crate) fn annotate_unified_patch_lines(patch: &str) -> String {
    let mut out = String::with_capacity(patch.len() + 256);
    let mut current_new_line: Option<usize> = None;

    for raw_line in patch.lines() {
        if let Some(start_new) = parse_hunk_new_start(raw_line) {
            current_new_line = Some(start_new);
            let _ = writeln!(out, "{raw_line}");
            continue;
        }

        match current_new_line {
            Some(line_no) if raw_line.starts_with('+') && !raw_line.starts_with("+++") => {
                let _ = writeln!(out, "+{line_no:>4} | {}", &raw_line[1..]);
                current_new_line = Some(line_no + 1);
            }
            Some(line_no) if raw_line.starts_with(' ') => {
                let _ = writeln!(out, " {line_no:>4} | {}", &raw_line[1..]);
                current_new_line = Some(line_no + 1);
            }
            Some(_) if raw_line.starts_with('-') && !raw_line.starts_with("---") => {
                let _ = writeln!(out, "    - | {}", &raw_line[1..]);
            }
            _ => {
                let _ = writeln!(out, "{raw_line}");
            }
        }
    }

    if out.ends_with('\n') {
        out.pop();
    }
    out
}

fn parse_hunk_new_start(line: &str) -> Option<usize> {
    let after_prefix = line.strip_prefix("@@ -")?;
    let (header_body, _) = after_prefix.split_once(" @@")?;
    let (_, new_range) = header_body.split_once(" +")?;
    let start_str = new_range
        .split_once(',')
        .map_or(new_range, |(start, _)| start);
    let parsed = start_str.trim().parse::<usize>().ok()?;
    Some(parsed.max(1))
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

fn truncate_chars(text: &str, max_chars: usize, label: &str) -> String {
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
    fn test_append_linked_issues_renders_author_and_labels() {
        let mut out = String::new();
        append_linked_issues(
            &mut out,
            &[LinkedIssueContext {
                number: 383,
                title: Some("Ingest linked_issues".to_string()),
                author: Some("jimbojw".to_string()),
                labels: vec!["type: feat".to_string(), "scope: core".to_string()],
                body: Some("Body text".to_string()),
                url: None,
            }],
        );
        assert!(out.contains(
            "- Issue #383: Ingest linked_issues (author: @jimbojw; labels: type: feat, scope: core)"
        ));
        assert!(out.contains("Body text"));
    }
}
