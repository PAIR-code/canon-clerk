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

//! Per-case prompt assembly and reason-first JSON Schema builder for `admit` (`docs/architecture/nodes/admit.md` §4).

use super::exhibits::{append_candidate_exhibit_section, non_blank};
use crate::canon::Canon;
use crate::caseload::{CaseloadDiscovery, CaseloadIntake};
use crate::discover::WorkspaceSnapshot;
use serde_json::{Value, json};
use std::fmt::Write as _;

pub(super) const ADMIT_SYSTEM_INSTRUCTION: &str = "\
You are the Clerk of the Court for Canon Clerk performing Micro Evidentiary Admissibility Triage (`admit`).
Given a single active Case (one governing engineering canon from the active docket) and the set of candidate exhibits (modified file diffs, target workspace files, and inspected pull request metadata planes such as `pr_title`, `pr_body`, or `linked_issues`), determine whether each candidate exhibit is admissible as relevant evidence prior to substantive judicial adjudication (`audit`).

Evaluation Rules:
1. Exact Domain Indirection: For each entry in `exhibits`, set `file_path` to the exact candidate exhibit identifier string presented under `### Candidate Exhibit:` (and listed under `## Required file_path Identifiers`) without modification.
2. Reason First: Write a concise, concrete `admissibility_summary` explaining why the candidate exhibit is or is not relevant evidence for evaluating compliance or exception criteria under the governing canon BEFORE assigning `admissibility_score`.
3. Positive Polarity Evidentiary Relevance Scoring & Rubric Bands (`[0.0, 1.0]`):
   - Score `0.75 - 0.95` (Admitted, `>= 0.5`) when the candidate exhibit contains code modifications, declarations, tests, documentation, or PR/issue metadata directly relevant to verifying compliance, detecting a violation, or evaluating a declared `Exception` under the governing canon.
   - NEVER Exclude Because of Compliance: Evidentiary admissibility depends strictly on relevance to the governing canon, NEVER on whether the exhibit violates the rule. A file or metadata plane that touches the governed construct and cleanly obeys the rule MUST score `0.75 - 0.95` so the trial judge (`audit`) receives affirmative evidence of compliance.
   - Omission & Companion-Artifact Rules: When the governing canon requires changes of a given kind to update or include a companion file or metadata (for example, `feat` PRs updating `README.md`, refactors omitting shims, or PRs linking motivating issues) and the companion artifact is absent from the diff, you MUST admit (`0.75 - 0.95`) the exhibits evidencing the triggering change (such as `pr_title`, `pr_body`, `linked_issues`, or the primary modified feature/module files).
   - Metadata Plane Hygiene: Admit `pr_title`, `pr_body`, or `linked_issues` (`0.75 - 0.95`) when the canon governs commits/PRs/workflow scope or declares exceptions grounded in PR/issue justification; exclude (`0.05 - 0.25`) generic metadata planes when the canon strictly governs self-contained source-code structure, naming, or syntax with no metadata-dependent criteria.
   - Score `0.05 - 0.25` (Excluded, `< 0.5`) when a candidate file or metadata plane does not touch, evidence, or contextualize the specific construct governed by the canon.
   - Avoid ambiguous boundary scores in `0.35 - 0.65`: commit decisively to `>= 0.75` when relevant or `<= 0.25` when irrelevant.
4. Completeness & Uniqueness: Return an `exhibits` array containing one entry per candidate exhibit identifier in the exact order listed—no omissions, no duplicates, and no extra identifiers.";

/// Builds a constant-size domain-indirected, reason-first JSON Schema enforcing
/// `file_path`, `admissibility_summary`, then `admissibility_score` (`docs/architecture/nodes/admit.md` §4.4).
pub(super) fn build_admit_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "exhibits": {
                "type": "array",
                "description": "Micro evidentiary admissibility assessments, containing one entry per candidate exhibit in the prompt.",
                "items": single_exhibit_schema()
            }
        },
        "propertyOrdering": ["exhibits"],
        "required": ["exhibits"]
    })
}

fn single_exhibit_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "file_path": {
                "type": "string",
                "description": "Exact candidate exhibit identifier from the prompt (`### Candidate Exhibit: ...`), either a repository-relative file path or metadata plane token (`pr_title`, `pr_body`, `linked_issues`)."
            },
            "admissibility_summary": {
                "type": "string",
                "description": "Concise rationale explaining why this candidate exhibit is or is not admissible as relevant evidence for the governing canon."
            },
            "admissibility_score": {
                "type": "number",
                "minimum": 0.0,
                "maximum": 1.0,
                "description": "Continuous evidentiary relevance score in [0.0, 1.0], where >= 0.5 admits the exhibit into evidence for trial."
            }
        },
        "propertyOrdering": ["file_path", "admissibility_summary", "admissibility_score"],
        "required": ["file_path", "admissibility_summary", "admissibility_score"]
    })
}

/// Assembles the isolated micro-triage user prompt for a single active case (`canon_ref`)
/// and its resolved `candidate_exhibits`.
pub(super) fn build_admit_case_user_prompt(
    canon_ref: &str,
    canon: &Canon,
    candidate_exhibits: &[String],
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
    workspace: &WorkspaceSnapshot,
) -> String {
    let mut out = String::with_capacity(1024 + candidate_exhibits.len() * 512);

    append_filing_overview(&mut out, intake, discovery);
    append_governing_canon_section(&mut out, canon_ref, canon);

    let _ = writeln!(
        out,
        "## Candidate Exhibits ({})\n",
        candidate_exhibits.len()
    );
    for exhibit_id in candidate_exhibits {
        append_candidate_exhibit_section(&mut out, exhibit_id, intake, workspace);
    }

    let _ = writeln!(
        out,
        "## Required `file_path` Identifiers ({})",
        candidate_exhibits.len()
    );
    for (idx, exhibit_id) in candidate_exhibits.iter().enumerate() {
        let _ = writeln!(out, "{}. `{exhibit_id}`", idx + 1);
    }

    out
}

fn append_filing_overview(
    out: &mut String,
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
) {
    let _ = writeln!(out, "## Filing Overview");
    let _ = writeln!(out, "- Intake Scope: {}", intake.scope.as_str());
    let _ = writeln!(out, "- Discovery Mode: {}", discovery.mode.as_str());
    if !intake.diffs.is_empty() {
        let touched: Vec<&str> = intake.diffs.keys().map(String::as_str).collect();
        let _ = writeln!(
            out,
            "- Modified Files in Filing ({}): {}",
            touched.len(),
            touched.join(", ")
        );
    } else if !discovery.target_files.is_empty() {
        let _ = writeln!(
            out,
            "- Target Files in Scope ({}): {}",
            discovery.target_files.len(),
            discovery.target_files.join(", ")
        );
    }
    if let Some(intent) = non_blank(intake.intent.as_deref()) {
        let _ = writeln!(out, "- Design Intent: {intent}");
    }
    out.push('\n');
}

fn append_governing_canon_section(out: &mut String, canon_ref: &str, canon: &Canon) {
    let _ = writeln!(out, "## Active Case Governing Canon: `{canon_ref}`");
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
    let body = canon.raw_body.trim();
    let statute = if body.is_empty() {
        canon.invariant.trim()
    } else {
        body
    };
    let _ = writeln!(out, "\n{statute}\n");
}
