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

//! Per-case and per-slice prompt assembly and reason-first JSON Schema builder for `admit` (`docs/architecture/nodes/admit.md` §4).

use super::exhibits::{append_candidate_exhibit_section, non_blank, truncate_chars};
use crate::canon::Canon;
use crate::caseload::{CaseloadDiscovery, CaseloadIntake};
use crate::discover::WorkspaceSnapshot;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fmt::Write as _;

const MAX_OVERVIEW_PR_BODY_CHARS: usize = 1_024;
const MAX_OVERVIEW_ISSUE_BODY_CHARS: usize = 512;
const MAX_OVERVIEW_LINKED_ISSUES: usize = 5;
const MAX_OVERVIEW_MANIFEST_PATHS: usize = 200;

pub(super) const ADMIT_SYSTEM_INSTRUCTION: &str = "\
You are the Clerk of the Court for Canon Clerk performing Micro Evidentiary Admissibility Triage (`admit`).
Given a set of candidate exhibits from an in-flight filing (modified file diffs, target workspace files, and inspected pull request metadata planes such as `pr_title`, `pr_body`, `commit_messages`, or `linked_issues`) followed by ONE active Case's Governing Canon, determine for EACH candidate exhibit independently whether it is admissible as direct, relevant evidence prior to judicial adjudication (`audit`).

Evaluation Rules:
1. Exact Single-Item Domain Indirection: For each entry in `exhibits`, set `file_path` to ONE exact candidate exhibit identifier copied verbatim from `### Candidate Exhibit:` / `## Required file_path Identifiers`. NEVER comma-separate, group, or combine multiple file paths into a single `file_path` string—every candidate exhibit MUST have its own separate JSON object in `exhibits`.
2. Construct-Grounded Contrastive Scratchpad (Reason First):
   - In `admissibility_summary`, explicitly name the construct class and artifact stratum governed by the canon (e.g., CLI option definitions/help headings, CLI `--json` or `stdout`/`stderr` output rendering, CLI usage error hints/env-var fallbacks, POSIX credential permissions, `README.md` feature docs, or `fix:` defect PRs) AND state whether THIS individual exhibit itself defines, implements, modifies, or directly evidences an instance of that construct.
   - NEVER write a generic summary of a file's role in the PR without testing whether it touches an instance of the canon's governed construct.
3. Independent Per-Exhibit Evaluation, Stratum Discipline & Anti-Momentum Scoring (`[0.0, 1.0]`):
   - Break Autoregressive Streaks (Zero Momentum): Evaluate Exhibit #k fresh against the Governing Canon regardless of whether the preceding 10+ exhibits were excluded (`<= 0.25`) or admitted (`>= 0.75`). Never carry high or low scores forward by streak momentum.
   - Universal Construct Coverage (Every Matching Instance Counts): Most canons govern a general class of constructs across the repository (such as ALL CLI subcommands, flags, help screens, stdin operands, error remediations, or output formatters; ALL domain entities; ALL credential handlers)—never assume a canon refers to some other hypothetical command or module. When a new or modified feature spans a multi-file package (e.g., `src/cli/<cmd>/{mod.rs, guards.rs, render.rs, stages.rs}`), admit (`0.75 - 0.95`) whichever specific file(s) actually implement the governed concern (such as `mod.rs` for CLI args/flags/help/stdin, `guards.rs` for CLI error hints or env-var fallbacks, and `render.rs`/`mod.rs` for `--json` or `stdout`/`stderr` stream separation).
   - Cross-Layer Exclusion (No PR-Wide Halo): Exclude (`0.05 - 0.25`) files belonging to other architectural layers in the same PR that do not instantiate the governed construct—for example, exclude internal domain engines, data-plane models, JSON decoders, and benchmark harnesses when evaluating a `cli-*` canon, and exclude CLI adapters when evaluating an internal domain-entity, validator, or parser canon.
   - Code Stratum vs. Documentation Stratum: When the canon governs source-code or runtime constructs (CLI flags/arguments/help/streams/exit codes, domain entities, validators, parsers, credentials, or test suites), prose Markdown documentation and architecture specs (`*.md`) that merely describe options or behavior do NOT implement the code construct and MUST be scored `0.05 - 0.25` (Excluded)—never admit docs as \"contextual support\". Admit `*.md` files (`0.75 - 0.95`) ONLY when the canon itself governs documentation or specification files (`README.md`, `AGENTS.md`, `llms.txt`, `SPEC.md`, `docs/architecture/`).
   - Contingent Commit-Type & Construct Preconditions: Inspect exact Conventional Commit prefixes carefully (`feat(...)` is NOT `fix(...)` or `refactor(...)`). When a canon applies ONLY to a specific commit/PR type or construct (e.g., `fix` PRs/commits repairing defects, `refactor` PRs, live external service tests, or POSIX signal traps) and that precondition is NOT met (for example, `pr_title` and `commit_messages` contain only `feat(...)` on a `fix`-only canon), score EVERY candidate exhibit—INCLUDING `pr_title`, `pr_body`, `commit_messages`, and `linked_issues` themselves—`0.05 - 0.25` (Excluded) so the inapplicable case is dismissed with zero admitted exhibits.
   - NEVER Exclude Because of Compliance: When an individual exhibit DOES directly define, implement, or modify an instance of the governed construct in the governed stratum and cleanly obeys the rule, you MUST score `0.75 - 0.95` (Admitted) so `audit` receives affirmative evidence of compliance.
   - Omission & Companion-Artifact Rules: When a canon's triggering precondition IS met (e.g., a `feat(...)` PR/commit required to update `README.md`, or a PR required to link its motivating issue) and the required companion artifact is absent from the diff, admit (`0.75 - 0.95`) the exhibits evidencing the triggering user-facing change (`pr_title`, `pr_body`, `commit_messages`, `linked_issues`, and primary user-facing entrypoint files), while excluding unrelated internal helper modules.
   - Metadata Plane Hygiene: Admit `pr_title`, `pr_body`, `commit_messages`, or `linked_issues` (`0.75 - 0.95`) ONLY when the canon's precondition IS met AND the canon governs commits/PRs/workflow scope or declares `Exception` criteria grounded in PR/commit/issue justification; exclude (`0.05 - 0.25`) metadata planes on self-contained source-code canons.
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
                "description": "SINGLE exact candidate exhibit identifier copied verbatim from `## Required file_path Identifiers` (either one repository-relative file path or one metadata plane token such as `pr_title`, `pr_body`, `commit_messages`, `linked_issues`). NEVER comma-separate or combine multiple paths into one string."
            },
            "admissibility_summary": {
                "type": "string",
                "description": "Construct-grounded rationale naming the canon's governed construct class and stratum, then stating whether THIS individual exhibit defines, implements, modifies, or directly evidences an instance of that construct (never write a generic file summary or carry forward prior exclusions/admissions by momentum)."
            },
            "admissibility_score": {
                "type": "number",
                "minimum": 0.0,
                "maximum": 1.0,
                "description": "Continuous evidentiary relevance score in [0.0, 1.0]: >= 0.75 when this specific exhibit implements, modifies, or directly evidences an instance of the governed construct in the governed stratum; <= 0.25 when the construct is absent from this exhibit, belongs to another stratum/layer, or a commit-type precondition is unmet."
            }
        },
        "propertyOrdering": ["file_path", "admissibility_summary", "admissibility_score"],
        "required": ["file_path", "admissibility_summary", "admissibility_score"]
    })
}

/// Assembles the isolated micro-triage user prompt for a single active case (`canon_ref`)
/// and its resolved `candidate_exhibits`.
///
/// Leads with the case-invariant `## Filing Overview` and `## Candidate Exhibits` sections so
/// cases sharing identical exhibit sets share an exact byte-for-byte prefix for provider prefix
/// caching, then places `## Active Case Governing Canon` at peak recency immediately before
/// `## Required file_path Identifiers`.
pub(super) fn build_admit_case_user_prompt(
    canon_ref: &str,
    canon: &Canon,
    candidate_exhibits: &[String],
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
    workspace: &WorkspaceSnapshot,
) -> String {
    let mut out = String::with_capacity(1280 + candidate_exhibits.len() * 512);

    append_filing_overview(&mut out, intake, discovery);

    let _ = writeln!(
        out,
        "## Candidate Exhibits ({})\n",
        candidate_exhibits.len()
    );
    for exhibit_id in candidate_exhibits {
        append_candidate_exhibit_section(&mut out, exhibit_id, intake, workspace);
    }

    append_governing_canon_section(&mut out, canon_ref, canon);
    append_evaluation_directive_and_identifiers(&mut out, canon_ref, candidate_exhibits);

    out
}

/// Assembles an isolated micro-triage user prompt for one slice of a multi-slice active case,
/// starting from the canon-invariant `shared_slice_prefix` (`## Filing Overview` + `## Candidate Exhibits`)
/// so sibling canons sharing identical exhibit sets retain byte-identical prefixes per slice.
pub(super) fn build_admit_slice_user_prompt(
    shared_slice_prefix: &str,
    canon_ref: &str,
    canon: &Canon,
    slice_exhibits: &[String],
) -> String {
    let mut out = String::with_capacity(shared_slice_prefix.len() + 1280);
    out.push_str(shared_slice_prefix);
    append_governing_canon_section(&mut out, canon_ref, canon);
    append_evaluation_directive_and_identifiers(&mut out, canon_ref, slice_exhibits);
    out
}

fn append_evaluation_directive_and_identifiers(
    out: &mut String,
    canon_ref: &str,
    required_exhibits: &[String],
) {
    let _ = writeln!(
        out,
        "## Evaluation Directive for `{canon_ref}`\n\n\
         Assess each exhibit listed below independently against `{canon_ref}` without carrying \
         forward exclusion or admission streaks from preceding exhibits:\n\
         - Score `0.75 - 0.95` (Admitted) for every exhibit in the governed stratum that defines, \
           implements, modifies, or directly evidences an instance of this canon's governed \
           construct (including compliant code and newly added subcommands/modules).\n\
         - Score `0.05 - 0.25` (Excluded) for prose documentation on source-code canons, \
           orthogonal architectural layers, and ALL exhibits (including `pr_title`) when a \
           commit-type precondition (such as `fix:` vs `feat:`) is unmet.\n"
    );

    let _ = writeln!(
        out,
        "## Required `file_path` Identifiers ({})",
        required_exhibits.len()
    );
    for (idx, exhibit_id) in required_exhibits.iter().enumerate() {
        let _ = writeln!(out, "{}. `{exhibit_id}`", idx + 1);
    }
}

pub(super) fn append_filing_overview(
    out: &mut String,
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
) {
    let _ = writeln!(out, "## Filing Overview");
    let _ = writeln!(out, "- Intake Scope: {}", intake.scope.as_str());
    let _ = writeln!(out, "- Discovery Mode: {}", discovery.mode.as_str());
    if let Some(title) = non_blank(intake.pr_title.as_deref()) {
        let _ = writeln!(out, "- PR Title: {title}");
    }
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

/// Builds the global `## Filing Overview` shared across every slice of a multi-slice case,
/// preserving `PR Title`, bounded `PR Body` and `Linked Issues` metadata context, and the
/// numbered manifest of all filing file paths in scope so contingent commit-type preconditions
/// and companion-artifact omission checks behave identically across disjoint slices.
pub(super) fn build_sliced_filing_overview(
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
    max_overview_chars: usize,
) -> String {
    let mut out = String::with_capacity(1536);
    append_filing_overview(&mut out, intake, discovery);

    if let Some(body) = non_blank(intake.pr_body.as_deref()) {
        let bounded = truncate_chars(body, MAX_OVERVIEW_PR_BODY_CHARS, "PR body overview");
        let _ = writeln!(out, "### Pull Request Description Context\n{bounded}\n");
    }

    if let Some(issues) = intake.linked_issues.as_deref().filter(|v| !v.is_empty()) {
        let _ = writeln!(out, "### Linked Issues Context");
        for issue in issues.iter().take(MAX_OVERVIEW_LINKED_ISSUES) {
            let title = non_blank(issue.title.as_deref()).unwrap_or("(untitled)");
            let _ = writeln!(out, "- Issue #{}: {title}", issue.number);
            if let Some(body) = non_blank(issue.body.as_deref()) {
                let bounded = truncate_chars(body, MAX_OVERVIEW_ISSUE_BODY_CHARS, "issue overview");
                let _ = writeln!(out, "  {bounded}");
            }
        }
        if issues.len() > MAX_OVERVIEW_LINKED_ISSUES {
            let _ = writeln!(
                out,
                "- ... (+{} additional linked issue(s))",
                issues.len() - MAX_OVERVIEW_LINKED_ISSUES
            );
        }
        out.push('\n');
    }

    if let Some(commits) = intake.commit_messages.as_deref().filter(|v| !v.is_empty()) {
        let _ = writeln!(out, "### Commit Messages Context ({})", commits.len());
        crate::audit::metadata::append_commit_messages_block(&mut out, commits, "", 15, 1_536);
        out.push('\n');
    }

    let filing_paths: BTreeSet<&str> = if !intake.diffs.is_empty() {
        intake.diffs.keys().map(String::as_str).collect()
    } else {
        discovery.target_files.iter().map(String::as_str).collect()
    };
    if !filing_paths.is_empty() {
        let _ = writeln!(
            out,
            "### All Filing File Paths in Scope ({})",
            filing_paths.len()
        );
        for (idx, path) in filing_paths
            .iter()
            .take(MAX_OVERVIEW_MANIFEST_PATHS)
            .enumerate()
        {
            let _ = writeln!(out, "{}. `{path}`", idx + 1);
        }
        if filing_paths.len() > MAX_OVERVIEW_MANIFEST_PATHS {
            let _ = writeln!(
                out,
                "... (+{} additional filing file path(s))",
                filing_paths.len() - MAX_OVERVIEW_MANIFEST_PATHS
            );
        }
        out.push('\n');
    }

    let cap = max_overview_chars.max(256);
    if out.len() > cap
        && let Some((byte_idx, _)) = out.char_indices().nth(cap)
    {
        out.truncate(byte_idx);
        out.push_str("\n... [filing overview truncated]\n\n");
    }

    out
}

pub(super) fn append_governing_canon_section(out: &mut String, canon_ref: &str, canon: &Canon) {
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
