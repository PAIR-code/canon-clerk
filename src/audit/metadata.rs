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

//! Admitted filing metadata plane renderers for `audit` (shared with `vet`) and bounded
//! `commit_messages` rendering helpers reused across `docket`, `admit`, and `audit`.

use super::exhibits::{MAX_AUDIT_EXHIBIT_CHARS, non_blank, truncate_chars};
use crate::caseload::{
    CaseloadIntake, CommitMessageContext, ExhibitAssessment, LinkedIssueContext,
};
use std::fmt::Write as _;

const MAX_AUDIT_COMMIT_ITEMS: usize = 50;

/// Renders a bounded, chronologically indexed list of commit messages with optional 7-char short SHA
/// and explicit omission counts (`... (+N additional commit(s))`) when capped by item count or
/// total character budget.
pub(crate) fn append_commit_messages_block(
    out: &mut String,
    commits: &[CommitMessageContext],
    indent: &str,
    max_items: usize,
    max_total_chars: usize,
) {
    let mut used_chars = 0usize;
    let mut rendered = 0usize;

    for (idx, commit) in commits.iter().enumerate() {
        if rendered >= max_items {
            break;
        }
        let message = commit.message.trim();
        if message.is_empty() {
            continue;
        }

        let short_sha = commit
            .sha
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.chars().take(7).collect::<String>());
        let prefix = match short_sha {
            Some(sha) => format!("{indent}{}. [{sha}] ", idx + 1),
            None => format!("{indent}{}. ", idx + 1),
        };
        let continuation_indent = format!("{indent}   ");

        let remaining_budget = max_total_chars.saturating_sub(used_chars);
        if rendered > 0 && remaining_budget < prefix.len() + 24 {
            break;
        }
        let msg_budget = remaining_budget.saturating_sub(prefix.len()).max(32);
        let bounded_message = truncate_commit_message(message, msg_budget);

        let mut lines = bounded_message.lines();
        if let Some(first) = lines.next() {
            let _ = writeln!(out, "{prefix}{first}");
            used_chars += prefix.len() + first.len() + 1;
            for cont in lines {
                if cont.is_empty() {
                    out.push('\n');
                    used_chars += 1;
                } else {
                    let _ = writeln!(out, "{continuation_indent}{cont}");
                    used_chars += continuation_indent.len() + cont.len() + 1;
                }
            }
            rendered += 1;
        }
    }

    let omitted = commits.len().saturating_sub(rendered);
    if omitted > 0 {
        let _ = writeln!(out, "{indent}... (+{omitted} additional commit(s))");
    }
}

fn truncate_commit_message(message: &str, max_chars: usize) -> String {
    match message.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => format!("{}... [truncated]", &message[..byte_idx]),
        None => message.to_string(),
    }
}

pub(super) fn append_commit_messages_exhibit(
    out: &mut String,
    exhibit: &ExhibitAssessment,
    intake: &CaseloadIntake,
) {
    let _ = writeln!(
        out,
        "### Admitted Metadata Exhibit: `commit_messages` (score: {:.2})",
        exhibit.admissibility_score
    );
    if let Some(commits) = intake.commit_messages.as_deref().filter(|v| !v.is_empty()) {
        append_commit_messages_block(
            out,
            commits,
            "",
            MAX_AUDIT_COMMIT_ITEMS,
            MAX_AUDIT_EXHIBIT_CHARS,
        );
    }
    out.push('\n');
}

pub(super) fn append_pr_title_exhibit(
    out: &mut String,
    exhibit: &ExhibitAssessment,
    intake: &CaseloadIntake,
) {
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

pub(super) fn append_pr_body_exhibit(
    out: &mut String,
    exhibit: &ExhibitAssessment,
    intake: &CaseloadIntake,
) {
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

pub(super) fn append_linked_issues_exhibit(
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

pub(super) fn append_linked_issues(out: &mut String, issues: &[LinkedIssueContext]) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::AdmissibilityStatus;

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

    #[test]
    fn test_append_commit_messages_exhibit_renders_short_sha_multiline_and_omission_notice() {
        let intake = CaseloadIntake {
            pr_title: Some("should-not-appear-in-commit-exhibit".to_string()),
            commit_messages: Some(vec![
                CommitMessageContext {
                    sha: Some("abcdef1234567890".to_string()),
                    message: "feat(cli): add commit_messages\n\nBody details.".to_string(),
                },
                CommitMessageContext {
                    sha: None,
                    message: "fix: second commit".to_string(),
                },
            ]),
            ..CaseloadIntake::default()
        };
        let exhibit = ExhibitAssessment {
            file_path: "commit_messages".to_string(),
            admissibility_summary: "Relevant".to_string(),
            admissibility_score: 0.95,
            admissibility_status: AdmissibilityStatus::Admitted,
        };
        let mut out = String::new();
        append_commit_messages_exhibit(&mut out, &exhibit, &intake);
        assert!(out.contains("1. [abcdef1] feat(cli): add commit_messages"));
        assert!(out.contains("   Body details."));
        assert!(out.contains("2. fix: second commit"));
        assert!(!out.contains("should-not-appear-in-commit-exhibit"));

        let mut capped = String::new();
        append_commit_messages_block(
            &mut capped,
            intake.commit_messages.as_deref().unwrap(),
            "  ",
            1,
            4096,
        );
        assert!(capped.contains("  1. [abcdef1] feat(cli): add commit_messages"));
        assert!(capped.contains("  ... (+1 additional commit(s))"));
    }
}
