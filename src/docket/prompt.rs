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

//! Context prefix and candidate-canon formatting for discriminative `docket` screening (`docs/architecture/nodes/docket.md` §4).

use crate::canon::Canon;
use crate::caseload::{CaseloadDiscovery, CaseloadIntake, FileArtifact, LinkedIssueContext};
use std::collections::BTreeSet;
use std::fmt::Write as _;

const MAX_MACRO_PATCH_CHARS: usize = 4_096;
const MIN_MACRO_PATCH_CHARS: usize = 160;
const FILE_EXHIBIT_FRAME_OVERHEAD_CHARS: usize = 160;
const MIN_FILE_EXHIBITS_BUDGET_CHARS: usize = 2_048;

/// Assembles a context-bounded shared filing prefix (`max_prefix_chars`), dividing patch preview
/// budget fairly across active modified file exhibits so every modified file header and diff preview
/// remains visible within bounded context windows.
pub(super) fn build_bounded_docket_filing_prefix(
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
    max_prefix_chars: usize,
) -> String {
    let mut out = String::new();
    append_filing_metadata_section(&mut out, intake, discovery);
    if out.len() > max_prefix_chars / 2 {
        out = truncate_chars(&out, max_prefix_chars / 2);
        out.push('\n');
    }
    if intake.diffs.is_empty() {
        return out;
    }

    let selected = select_active_diff_exhibits(intake, discovery);
    let remaining = max_prefix_chars
        .saturating_sub(out.len())
        .max(MIN_FILE_EXHIBITS_BUDGET_CHARS);
    let file_count = selected.len().max(1);
    let patch_pool =
        remaining.saturating_sub(file_count.saturating_mul(FILE_EXHIBIT_FRAME_OVERHEAD_CHARS));
    let per_file_cap =
        (patch_pool / file_count).clamp(MIN_MACRO_PATCH_CHARS, MAX_MACRO_PATCH_CHARS);
    append_selected_file_exhibits(&mut out, &selected, per_file_cap, max_prefix_chars);
    out
}

fn append_filing_metadata_section(
    out: &mut String,
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
) {
    let _ = writeln!(out, "## Filing & Scope Summary");
    let _ = writeln!(out, "- Intake Scope: {}", intake.scope.as_str());
    let _ = writeln!(out, "- Discovery Mode: {}", discovery.mode.as_str());
    if let Some(title) = non_blank(intake.pr_title.as_deref()) {
        let _ = writeln!(out, "- PR Title: {title}");
    }
    if let Some(body) = non_blank(intake.pr_body.as_deref()) {
        let _ = writeln!(out, "- PR Description: {body}");
    }
    if let Some(intent) = non_blank(intake.intent.as_deref()) {
        let _ = writeln!(out, "- Design Intent: {intent}");
    }
    if !intake.target_paths.is_empty() {
        let _ = writeln!(
            out,
            "- Target Directives: {}",
            intake.target_paths.join(", ")
        );
    }
    if !discovery.target_files.is_empty() {
        let _ = writeln!(
            out,
            "- Materialized Target Files: {}",
            discovery.target_files.join(", ")
        );
    }
    if let Some(issues) = intake.linked_issues.as_deref().filter(|v| !v.is_empty()) {
        append_linked_issues(out, issues);
    }
    if let Some(commits) = intake.commit_messages.as_deref().filter(|v| !v.is_empty()) {
        let _ = writeln!(out, "- Commit Messages ({}):", commits.len());
        crate::audit::metadata::append_commit_messages_block(out, commits, "  ", 25, 4_096);
    }
    out.push('\n');
}

fn append_linked_issues(out: &mut String, issues: &[LinkedIssueContext]) {
    let _ = writeln!(out, "- Linked Issues:");
    for issue in issues {
        let title = non_blank(issue.title.as_deref()).unwrap_or("(untitled)");
        let meta = format_linked_issue_metadata(issue);
        let _ = writeln!(out, "  - #{}: {title}{meta}", issue.number);
        if let Some(body) = non_blank(issue.body.as_deref()) {
            let _ = writeln!(out, "    {body}");
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

fn select_active_diff_exhibits<'a>(
    intake: &'a CaseloadIntake,
    discovery: &CaseloadDiscovery,
) -> Vec<&'a FileArtifact> {
    let active_set: BTreeSet<&str> = discovery
        .active_exhibits
        .files
        .iter()
        .map(String::as_str)
        .collect();
    let has_active_diff_overlap = intake
        .diffs
        .keys()
        .any(|path| active_set.contains(path.as_str()));

    intake
        .diffs
        .values()
        .filter(|art| !has_active_diff_overlap || active_set.contains(art.path.as_str()))
        .collect()
}

fn append_selected_file_exhibits(
    out: &mut String,
    selected: &[&FileArtifact],
    per_file_patch_cap: usize,
    total_char_ceiling: usize,
) {
    if selected.is_empty() {
        return;
    }

    let _ = writeln!(out, "## Modified File Exhibits ({})\n", selected.len());
    for artifact in selected {
        let include_patch = out.len() < total_char_ceiling;
        append_single_file_exhibit(out, artifact, per_file_patch_cap, include_patch);
    }
}

fn append_single_file_exhibit(
    out: &mut String,
    artifact: &FileArtifact,
    per_file_patch_cap: usize,
    include_patch: bool,
) {
    match &artifact.previous_path {
        Some(prev) => {
            let _ = writeln!(
                out,
                "### File: `{}` ({} from `{}`, +{} / -{})",
                artifact.path,
                artifact.status.as_str(),
                prev,
                artifact.lines_added,
                artifact.lines_deleted
            );
        }
        None => {
            let _ = writeln!(
                out,
                "### File: `{}` ({}, +{} / -{})",
                artifact.path,
                artifact.status.as_str(),
                artifact.lines_added,
                artifact.lines_deleted
            );
        }
    }

    if include_patch && let Some(patch) = non_blank(artifact.patch.as_deref()) {
        let bounded = truncate_chars(patch, per_file_patch_cap);
        let _ = writeln!(out, "```diff\n{bounded}\n```");
    }
    out.push('\n');
}

pub(super) fn append_canon_section(
    out: &mut String,
    canon_ref: &str,
    canon: &Canon,
    matched: &[String],
) {
    let _ = writeln!(out, "### Canon: `{canon_ref}`");
    let _ = writeln!(out, "- ID: `{}`", canon.id);
    let _ = writeln!(out, "- Title: {}", canon.title);
    if !canon.triggers.is_empty() {
        let _ = writeln!(out, "- Triggers: {}", canon.triggers.join(", "));
    }
    if !canon.exists.is_empty() {
        let _ = writeln!(out, "- Exists: {}", canon.exists.join(", "));
    }
    if !canon.inspect.is_empty() {
        let inspect_list: Vec<String> = canon
            .inspect
            .iter()
            .map(|p| {
                if p.optional {
                    format!("{}?", p.token.as_str())
                } else {
                    p.token.as_str().to_string()
                }
            })
            .collect();
        let _ = writeln!(out, "- Inspect: {}", inspect_list.join(", "));
    }
    if !canon.references.is_empty() {
        let _ = writeln!(out, "- References: {}", canon.references.join(", "));
    }
    if !canon.tags.is_empty() {
        let _ = writeln!(out, "- Tags: {}", canon.tags.join(", "));
    }
    if !matched.is_empty() {
        let _ = writeln!(out, "- Triggered Exhibits: {}", matched.join(", "));
    }
    let _ = writeln!(out, "\n{}", canon.invariant.trim());
    let _ = writeln!(out, "- Exact `canon_path`: `{canon_ref}`\n");
}

fn non_blank(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|s| !s.is_empty())
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut char_indices = text.char_indices();
    match char_indices.nth(max_chars) {
        Some((byte_idx, _)) => format!(
            "{}\n... [patch truncated for macro screening]",
            &text[..byte_idx]
        ),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_canon_section_renders_only_invariant_excluding_non_normative_directives() {
        let canon = crate::canon::parse_canon(
            "Boolean-returning functions MUST specify polarity.\n\n\
             Precedent: Effective Java (Item 56).\n\n\
             Rationale: Prevents inverted caller assumptions.\n\n\
             Remediation: Document both true and false outcomes.",
            &crate::canon::ParseCanonOptions::with_source_ref(
                ".canons/boolean-return-docs-must-specify-polarity.canon.md",
            ),
        )
        .expect("parse canon");

        let mut out = String::new();
        append_canon_section(
            &mut out,
            ".canons/boolean-return-docs-must-specify-polarity.canon.md",
            &canon,
            &["src/lib.rs".to_string()],
        );
        assert!(out.contains("Boolean-returning functions MUST specify polarity."));
        assert!(!out.contains("Effective Java"));
        assert!(!out.contains("Prevents inverted caller assumptions"));
        assert!(!out.contains("Document both true and false outcomes"));
    }

    #[test]
    fn test_append_linked_issues_renders_author_and_labels() {
        let mut out = String::new();
        append_linked_issues(
            &mut out,
            &[LinkedIssueContext {
                number: 383,
                title: Some("Ingest linked_issues".to_string()),
                author: Some("jimbojw".to_string()),
                labels: vec!["type: feat".to_string(), "scope: cli".to_string()],
                body: Some("Wire ingestion.".to_string()),
                url: None,
            }],
        );
        assert!(out.contains(
            "#383: Ingest linked_issues (author: @jimbojw; labels: type: feat, scope: cli)"
        ));
        assert!(out.contains("Wire ingestion."));
    }
}
