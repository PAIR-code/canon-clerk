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

//! Aggregate prompt assembly and reason-first JSON Schema builder for `docket` (`docs/architecture/nodes/docket.md` §4).

use crate::canon::Canon;
use crate::caseload::{CaseloadDiscovery, CaseloadIntake, FileArtifact, LinkedIssueContext};
use crate::discover::WorkspaceSnapshot;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fmt::Write as _;

const MAX_MACRO_PATCH_CHARS: usize = 4_096;

pub(super) const DOCKET_SYSTEM_INSTRUCTION: &str = "\
You are the Clerk of the Court for Canon Clerk performing Macro Subject-Matter Jurisdiction Triage (`docket`).
Given an in-flight filing (pull request metadata, linked issues, target file scope, and modified file diffs) and a set of candidate engineering canons, determine whether each candidate canon has a colorable claim of subject-matter jurisdiction over this filing as a whole before opening a formal case on the active docket.

Evaluation Rules:
1. Exact Domain Indirection: For each entry in `cases`, set `canon_path` to the exact candidate canon path string presented under `### Canon:` without modification.
2. Reason First: Write a concise, concrete `colorability_summary` explaining why the canon does or does not have a colorable claim of jurisdiction over the filing BEFORE assigning `colorability_score`.
3. Positive Polarity Jurisdiction Scoring (`[0.0, 1.0]`):
   - Score >= 0.5 when the filing touches, introduces, or modifies code, configuration, documentation, or metadata governed by the canon's invariant or exceptions (even if the change already appears compliant—substantive merits compliance belongs to downstream `audit`).
   - Score < 0.5 when the canon's subject matter is orthogonal or inapplicable to the filing.
4. Completeness & Uniqueness: Return a `cases` array containing one entry per candidate canon identifier—no omissions, no duplicates, and no extra identifiers.";

/// Builds a constant-size domain-indirected, reason-first JSON Schema enforcing
/// `canon_path`, `colorability_summary`, then `colorability_score` (`docs/architecture/nodes/docket.md` §4.3).
pub(super) fn build_docket_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "cases": {
                "type": "array",
                "description": "Macro jurisdiction colorability assessments, containing one entry per candidate canon in the prompt.",
                "items": single_canon_case_schema()
            }
        },
        "propertyOrdering": ["cases"],
        "required": ["cases"]
    })
}

fn single_canon_case_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "canon_path": {
                "type": "string",
                "description": "Exact candidate canon path identifier from the prompt (`### Canon: ...`)."
            },
            "colorability_summary": {
                "type": "string",
                "description": "Concise rationale explaining why this canon does or does not claim subject-matter jurisdiction over the filing."
            },
            "colorability_score": {
                "type": "number",
                "minimum": 0.0,
                "maximum": 1.0,
                "description": "Continuous subject-matter jurisdiction score in [0.0, 1.0], where >= 0.5 indicates a colorable claim."
            }
        },
        "propertyOrdering": ["canon_path", "colorability_summary", "colorability_score"],
        "required": ["canon_path", "colorability_summary", "colorability_score"]
    })
}

/// Assembles the single-turn aggregate user prompt presenting the filing context and all candidate canons.
pub(super) fn build_docket_user_prompt(
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
    workspace: &WorkspaceSnapshot,
) -> Result<String, String> {
    let mut out = String::new();
    append_filing_metadata_section(&mut out, intake, discovery);
    append_file_exhibits_section(&mut out, intake, discovery);

    let _ = writeln!(
        out,
        "## Candidate Canons ({})\n",
        discovery.candidate_canons.len()
    );
    for canon_ref in &discovery.candidate_canons {
        let canon = workspace
            .canons
            .get(canon_ref)
            .ok_or_else(|| canon_ref.clone())?;
        let matched = discovery
            .triggers_join
            .get(canon_ref)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        append_canon_section(&mut out, canon_ref, canon, matched);
    }

    Ok(out)
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
    out.push('\n');
}

fn append_linked_issues(out: &mut String, issues: &[LinkedIssueContext]) {
    let _ = writeln!(out, "- Linked Issues:");
    for issue in issues {
        let title = non_blank(issue.title.as_deref()).unwrap_or("(untitled)");
        let _ = writeln!(out, "  - #{}: {title}", issue.number);
        if let Some(body) = non_blank(issue.body.as_deref()) {
            let _ = writeln!(out, "    {body}");
        }
    }
}

fn append_file_exhibits_section(
    out: &mut String,
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
) {
    if intake.diffs.is_empty() {
        return;
    }

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

    let selected: Vec<&FileArtifact> = intake
        .diffs
        .values()
        .filter(|art| !has_active_diff_overlap || active_set.contains(art.path.as_str()))
        .collect();

    let _ = writeln!(out, "## Modified File Exhibits ({})\n", selected.len());
    for artifact in selected {
        append_single_file_exhibit(out, artifact);
    }
}

fn append_single_file_exhibit(out: &mut String, artifact: &FileArtifact) {
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

    if let Some(patch) = non_blank(artifact.patch.as_deref()) {
        let bounded = truncate_chars(patch, MAX_MACRO_PATCH_CHARS);
        let _ = writeln!(out, "```diff\n{bounded}\n```");
    }
    out.push('\n');
}

fn append_canon_section(out: &mut String, canon_ref: &str, canon: &Canon, matched: &[String]) {
    let _ = writeln!(out, "### Canon: `{canon_ref}`");
    let _ = writeln!(out, "- ID: `{}`", canon.id);
    let _ = writeln!(out, "- Title: {}", canon.title);
    if !matched.is_empty() {
        let _ = writeln!(out, "- Triggered Exhibits: {}", matched.join(", "));
    }
    let body = canon.raw_body.trim();
    let statute = if body.is_empty() {
        canon.invariant.trim()
    } else {
        body
    };
    let _ = writeln!(out, "\n{statute}\n");
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
