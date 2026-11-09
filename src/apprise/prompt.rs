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

//! Aggregate prompt assembly and reason-first JSON Schema builder for `apprise` (`docs/architecture/nodes/apprise.md` §4).

use crate::canon::Canon;
use crate::caseload::{CaseloadDiscovery, CaseloadIntake};
use crate::discover::WorkspaceSnapshot;
use serde_json::{Map, Value, json};
use std::fmt::Write as _;

pub(super) const APPRISE_SYSTEM_INSTRUCTION: &str = "\
You are the Clerk of the Court for Canon Clerk exercising Statutory Apprisal Authority (Jurisdictio Notificatoria).
Given a prospective architectural design intent, target scope context, and a set of candidate engineering canons, determine whether each candidate canon has a colorable claim of prospective subject-matter jurisdiction over the planned work before implementation begins.

Evaluation Rules:
1. Reason First: For every candidate canon, write a concise, concrete `apprisal_summary` explaining why the canon does or does not govern the planned design intent and target scope BEFORE assigning `apprisal_score`.
2. Positive Polarity Scoring (`[0.0, 1.0]`):
   - Score >= 0.5 when the planned work touches, introduces, or modifies concerns governed by the canon's invariant or exceptions.
   - Score < 0.5 when the canon is orthogonal or inapplicable to the planned work.
3. Completeness: Return an assessment for every candidate canon identifier.";

/// Builds a strict reason-first JSON Schema enforcing `apprisal_summary` before `apprisal_score`
/// for every candidate canon (`docs/architecture/nodes/apprise.md` §4.3).
pub(super) fn build_apprise_schema(candidate_canons: &[String]) -> Value {
    let mut assessment_props = Map::new();
    for canon_ref in candidate_canons {
        assessment_props.insert(canon_ref.clone(), single_canon_assessment_schema());
    }

    let ordered_keys: Vec<Value> = candidate_canons
        .iter()
        .map(|k| Value::String(k.clone()))
        .collect();

    json!({
        "type": "object",
        "properties": {
            "assessments": {
                "type": "object",
                "properties": Value::Object(assessment_props),
                "propertyOrdering": ordered_keys,
                "required": candidate_canons,
            }
        },
        "propertyOrdering": ["assessments"],
        "required": ["assessments"]
    })
}

fn single_canon_assessment_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "apprisal_summary": {
                "type": "string",
                "description": "Concise rationale explaining why this canon does or does not claim prospective jurisdiction over the planned design intent and target scope."
            },
            "apprisal_score": {
                "type": "number",
                "minimum": 0.0,
                "maximum": 1.0,
                "description": "Continuous prospective applicability score in [0.0, 1.0], where >= 0.5 indicates the canon governs the planned work."
            }
        },
        "propertyOrdering": ["apprisal_summary", "apprisal_score"],
        "required": ["apprisal_summary", "apprisal_score"]
    })
}

/// Assembles the single-turn aggregate user prompt presenting prospective design intent,
/// target scope context, and all candidate canons.
pub(super) fn build_apprise_user_prompt(
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
    workspace: &WorkspaceSnapshot,
) -> Result<String, String> {
    let mut out = String::new();
    let intent = intake.intent.as_deref().unwrap_or_default().trim();

    let _ = writeln!(out, "## Prospective Design Intent\n\n{intent}\n");
    append_scope_section(&mut out, intake, discovery);

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

fn append_scope_section(out: &mut String, intake: &CaseloadIntake, discovery: &CaseloadDiscovery) {
    let _ = writeln!(out, "## Target & Discovery Scope");
    let _ = writeln!(out, "- Intake Scope: {}", intake.scope.as_str());
    let _ = writeln!(out, "- Discovery Mode: {}", discovery.mode.as_str());
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
    if let Some(title) = intake.pr_title.as_deref().filter(|s| !s.trim().is_empty()) {
        let _ = writeln!(out, "- PR Title: {}", title.trim());
    }
    if let Some(body) = intake.pr_body.as_deref().filter(|s| !s.trim().is_empty()) {
        let _ = writeln!(out, "- PR Description: {}", body.trim());
    }
    out.push('\n');
}

fn append_canon_section(out: &mut String, canon_ref: &str, canon: &Canon, matched: &[String]) {
    let _ = writeln!(out, "### Canon: `{canon_ref}`");
    let _ = writeln!(out, "- ID: `{}`", canon.id);
    let _ = writeln!(out, "- Title: {}", canon.title);
    if !matched.is_empty() {
        let _ = writeln!(out, "- Triggered Scope: {}", matched.join(", "));
    }
    let body = canon.raw_body.trim();
    let statute = if body.is_empty() {
        canon.invariant.trim()
    } else {
        body
    };
    let _ = writeln!(out, "\n{statute}\n");
}
