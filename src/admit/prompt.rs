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
Given a single active Case (one governing engineering canon from the active docket) and a list of candidate exhibits (modified file diffs, target workspace files, and inspected pull request metadata planes such as `pr_title`, `pr_body`, or `linked_issues`), determine for EACH candidate exhibit in isolation whether it is admissible as direct, relevant evidence prior to judicial adjudication (`audit`).

Evaluation Rules:
1. Exact Single-Item Domain Indirection: For each entry in `exhibits`, set `file_path` to ONE exact candidate exhibit identifier copied verbatim from `### Candidate Exhibit:` / `## Required file_path Identifiers`. NEVER comma-separate, group, or combine multiple file paths into a single `file_path` string—every candidate exhibit MUST have its own separate JSON object in `exhibits`.
2. Construct-Grounded Contrastive Scratchpad (Reason First):
   - In `admissibility_summary`, you MUST explicitly name the specific construct and artifact stratum governed by the canon (e.g., boolean CLI option definitions in argument-parser code, `--help` usage examples, POSIX credential permission checks, `README.md` feature documentation, or `fix:` pull requests) AND state whether THIS individual exhibit itself defines, modifies, or directly evidences that construct.
   - NEVER write a generic description of what a file does in the PR without testing whether it instantiates or modifies the canon's governed construct.
3. Per-Exhibit Isolation, Stratum Discipline & Anti-Halo Scoring (`[0.0, 1.0]`):
   - Evaluate every exhibit strictly in isolation. Pull requests routinely touch 15-30 files across specs, domain logic, data models, CLI adapters, and tests where ONLY 0 to 3 exhibits actually touch a canon's specific construct. NEVER assign uniform high scores across all files in a PR just because the PR as a whole relates to the topic.
   - Code Stratum vs. Documentation Stratum: When the canon governs source-code or runtime constructs (such as CLI parser flags/arguments/help/streams/exit codes, domain entities, validators, parsers, credentials, or test suites), prose Markdown documentation and architecture specs (`*.md`) that merely describe or document options/behavior do NOT implement the code construct and MUST be scored `0.05 - 0.25` (Excluded)—never admit docs merely as \"contextual support\". Admit `*.md` files (`0.75 - 0.95`) ONLY when the canon itself governs documentation or specification files (`README.md`, `AGENTS.md`, `llms.txt`, `SPEC.md`, `docs/architecture/`).
   - Sibling-Module Exclusion: Within source code, exclude (`0.05 - 0.25`) sibling modules in the same PR (such as pure data-plane models, JSON decoders, domain engines, or barrel `lib.rs` re-exports) when they do not themselves define, parse, or touch the canon's governed construct.
   - Contingent Trigger & Commit-Type Preconditions: When a canon applies conditionally to a specific commit/PR type or construct (for example, `fix` PRs/commits repairing defects, `refactor` PRs, live external service tests, or POSIX signal traps) and the candidate exhibits show that precondition is NOT met (e.g., `pr_title` is `feat(...)` rather than `fix(...)`), score ALL candidate exhibits `0.05 - 0.25` (Excluded) so the inapplicable case is dismissed without trial.
   - NEVER Exclude Because of Compliance: When an individual exhibit DOES directly define or modify the governed construct in the governed stratum and cleanly obeys the rule, you MUST score `0.75 - 0.95` (Admitted) so `audit` receives affirmative evidence of compliance.
   - Omission & Companion-Artifact Rules: When a canon's triggering precondition IS met (e.g., a `feat` PR required to update `README.md`, or a PR required to link its motivating issue) and the required companion artifact is absent from the diff, admit (`0.75 - 0.95`) the specific exhibits evidencing the triggering user-facing change (`pr_title`, `pr_body`, `linked_issues`, and primary entrypoint files), while still excluding unrelated internal helper modules.
   - Metadata Plane Hygiene: Admit `pr_title`, `pr_body`, or `linked_issues` (`0.75 - 0.95`) ONLY when the canon governs commits/PRs/workflow scope or declares `Exception` criteria grounded in PR/issue justification; exclude (`0.05 - 0.25`) metadata planes when the canon governs self-contained source-code structure, naming, or syntax.
   - Avoid ambiguous boundary scores in `0.35 - 0.65`: commit decisively to `>= 0.75` when directly relevant or `<= 0.25` when absent/orthogonal.
4. Completeness & 1-to-1 Cardinality: Return an `exhibits` array whose length equals the exact count in `## Required file_path Identifiers`, with one separate object per identifier in the exact order listed—never group multiple identifiers into one `file_path`, never omit an identifier, and never emit duplicates or unknown paths.";

/// Builds a constant-size domain-indirected, reason-first JSON Schema enforcing
/// `file_path`, `admissibility_summary`, then `admissibility_score` (`docs/architecture/nodes/admit.md` §4.4).
pub(super) fn build_admit_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "exhibits": {
                "type": "array",
                "description": "Per-exhibit admissibility assessments evaluated independently against the governing canon's specific construct and stratum.",
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
                "description": "SINGLE exact candidate exhibit identifier copied verbatim from `## Required file_path Identifiers` (either one repository-relative file path or one metadata plane token such as `pr_title`, `pr_body`, `linked_issues`). NEVER comma-separate or combine multiple paths into one string."
            },
            "admissibility_summary": {
                "type": "string",
                "description": "Construct-grounded rationale naming the canon's specific governed construct/stratum and stating whether THIS individual exhibit directly defines, modifies, or evidences it (never write a generic file summary or admit docs/sibling modules as general PR context)."
            },
            "admissibility_score": {
                "type": "number",
                "minimum": 0.0,
                "maximum": 1.0,
                "description": "Continuous evidentiary relevance score in [0.0, 1.0]: >= 0.75 when this specific exhibit directly touches or evidences the governed construct; <= 0.25 when the construct is absent from this exhibit or a commit-type precondition is unmet."
            }
        },
        "propertyOrdering": ["file_path", "admissibility_summary", "admissibility_score"],
        "required": ["file_path", "admissibility_summary", "admissibility_score"]
    })
}

/// Assembles the isolated micro-triage user prompt for a single active case (`canon_ref`)
/// and its resolved `candidate_exhibits`, leading with the governing canon so attention
/// anchors on the exact statutory construct and stratum before scanning exhibits.
pub(super) fn build_admit_case_user_prompt(
    canon_ref: &str,
    canon: &Canon,
    candidate_exhibits: &[String],
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
    workspace: &WorkspaceSnapshot,
) -> String {
    let mut out = String::with_capacity(1280 + candidate_exhibits.len() * 512);

    append_governing_canon_section(&mut out, canon_ref, canon);
    append_filing_overview(&mut out, intake, discovery);

    let _ = writeln!(
        out,
        "## Candidate Exhibits ({})\n\n\
         Evaluate each exhibit below strictly on its own contents against `{canon_ref}`. \
         Exclude (`<= 0.25`) prose documentation, sibling modules, and metadata planes that do not \
         directly define, modify, or evidence this canon's specific construct.\n",
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
        let _ = writeln!(
            out,
            "- Total Modified Files in Filing: {}",
            intake.diffs.len()
        );
    } else if !discovery.target_files.is_empty() {
        let _ = writeln!(
            out,
            "- Total Target Files in Scope: {}",
            discovery.target_files.len()
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
    let _ = writeln!(out, "- Governing Invariant: {}", canon.invariant.trim());
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
