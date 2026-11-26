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

//! Per-canon precondition verification, trigger intersection, workspace policy filtering,
//! and bipartite `inspect:` plane evaluation for `discover`.

use super::glob_matcher::{GlobPattern, scope_relative_path};
use crate::canon::{Canon, CanonScope, InspectToken};
use crate::caseload::CaseloadIntake;
use crate::policy::WorkspacePolicySet;
use std::collections::BTreeSet;

pub(super) struct EvaluatedCanonMatch {
    pub(super) matched_files: Vec<String>,
    pub(super) inspected_metadata: Vec<String>,
    pub(super) active_pr_title: bool,
    pub(super) active_pr_body: bool,
    pub(super) active_commit_messages: bool,
    pub(super) active_linked_issues: bool,
}

pub(super) fn evaluate_candidate_canon(
    canon_ref: &str,
    canon: &Canon,
    intake: &CaseloadIntake,
    target_files: &[String],
    target_file_tree: &BTreeSet<String>,
    policy: Option<&WorkspacePolicySet>,
) -> Option<EvaluatedCanonMatch> {
    let canon_scope = canon.scope.as_deref().map(|s| CanonScope(s.to_string()));

    let in_scope_targets: Vec<&String> = target_files
        .iter()
        .filter(|f| scope_relative_path(f, canon_scope.as_ref()).is_some())
        .collect();

    if canon_scope.is_some() && in_scope_targets.is_empty() {
        return None;
    }

    if !verify_exists_preconditions(canon, canon_scope.as_ref(), target_file_tree) {
        return None;
    }

    let matched_files: Vec<String> =
        match_canon_triggers(canon, canon_scope.as_ref(), &in_scope_targets)
            .into_iter()
            .filter(|file| policy.is_none_or(|p| p.allows_file_exhibit(canon_ref, canon, file)))
            .collect();

    let allow_unconditional = policy.is_none_or(|p| p.allows_unconditional(canon_ref, canon));
    if matched_files.is_empty() && !allow_unconditional {
        return None;
    }

    evaluate_bipartite_inspect(canon, intake, target_files, matched_files)
}

fn verify_exists_preconditions(
    canon: &Canon,
    canon_scope: Option<&CanonScope>,
    target_file_tree: &BTreeSet<String>,
) -> bool {
    if canon.exists.is_empty() {
        return true;
    }

    for pattern in &canon.exists {
        let Ok(glob) = GlobPattern::parse(pattern) else {
            return false;
        };
        let matched = target_file_tree.iter().any(|file| {
            let Some(rel_in_scope) = scope_relative_path(file, canon_scope) else {
                return false;
            };
            glob.is_match(file) || glob.is_match(&rel_in_scope)
        });
        if !matched {
            return false;
        }
    }
    true
}

fn match_canon_triggers(
    canon: &Canon,
    canon_scope: Option<&CanonScope>,
    in_scope_targets: &[&String],
) -> Vec<String> {
    let trigger_globs: Vec<GlobPattern> = canon
        .triggers
        .iter()
        .filter_map(|t| GlobPattern::parse(t).ok())
        .collect();

    let mut matched = Vec::new();
    for &file in in_scope_targets {
        let Some(rel_in_scope) = scope_relative_path(file, canon_scope) else {
            continue;
        };
        let is_triggered = trigger_globs.is_empty()
            || trigger_globs
                .iter()
                .any(|g| g.is_match(file) || g.is_match(&rel_in_scope));
        if is_triggered {
            matched.push(file.clone());
        }
    }
    matched
}

fn evaluate_bipartite_inspect(
    canon: &Canon,
    intake: &CaseloadIntake,
    target_files: &[String],
    matched_files: Vec<String>,
) -> Option<EvaluatedCanonMatch> {
    let has_diff_match = !matched_files.is_empty();
    let has_pr_title = intake.pr_title.is_some();
    let has_pr_body = intake.pr_body.is_some();
    let has_commit_messages = intake
        .commit_messages
        .as_ref()
        .is_some_and(|v| !v.is_empty());
    let has_linked_issues = intake.linked_issues.as_ref().is_some_and(|v| !v.is_empty());

    let inspects_diff = canon.inspect.iter().any(|p| p.token == InspectToken::Diff);
    let has_explicit_triggers = canon
        .raw_frontmatter
        .as_ref()
        .is_some_and(|fm| fm.contains_key("triggers"));

    if !inspects_diff && has_explicit_triggers && !target_files.is_empty() && !has_diff_match {
        return None;
    }

    let retained_files = if inspects_diff {
        matched_files
    } else {
        Vec::new()
    };

    let mut inspected_metadata = Vec::new();
    let mut active_pr_title = false;
    let mut active_pr_body = false;
    let mut active_commit_messages = false;
    let mut active_linked_issues = false;

    for plane in &canon.inspect {
        let available = match plane.token {
            InspectToken::Diff => has_diff_match,
            InspectToken::PrTitle => has_pr_title,
            InspectToken::PrBody => has_pr_body,
            InspectToken::CommitMessages => has_commit_messages,
            InspectToken::LinkedIssues => has_linked_issues,
        };

        if !plane.optional && !available {
            return None;
        }

        if available && plane.token != InspectToken::Diff {
            inspected_metadata.push(plane.token.as_str().to_string());
            match plane.token {
                InspectToken::PrTitle => active_pr_title = true,
                InspectToken::PrBody => active_pr_body = true,
                InspectToken::CommitMessages => active_commit_messages = true,
                InspectToken::LinkedIssues => active_linked_issues = true,
                InspectToken::Diff => {}
            }
        }
    }

    if retained_files.is_empty() && inspected_metadata.is_empty() {
        return None;
    }

    Some(EvaluatedCanonMatch {
        matched_files: retained_files,
        inspected_metadata,
        active_pr_title,
        active_pr_body,
        active_commit_messages,
        active_linked_issues,
    })
}
