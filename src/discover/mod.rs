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

//! Core domain execution engine for the `discover` Caseload DAG stage (`docs/architecture/nodes/discover.md`).

pub mod glob_matcher;
pub mod workspace;

pub use glob_matcher::{
    DEFAULT_CANON_GLOB, DEFAULT_IGNORES, GlobPattern, GlobSyntaxError, is_ignored_path,
    is_wildcard_pattern, normalize_workspace_path, scope_relative_path,
};
pub use workspace::{WorkspaceSnapshot, parse_canon_or_fallback};

use crate::canon::{Canon, CanonScope, InspectToken};
use crate::caseload::{
    ActiveExhibits, Caseload, CaseloadDiscovery, CaseloadIntake, DiscoveryMode, FileChangeStatus,
    IntakeScope,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;
use workspace::compile_canon_filter_globs;

/// Domain input parameters for executing the `discover` pipeline stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiscoverOptions {
    /// Whether to bypass path trigger intersection and promote all discoverable canons (`--all-canons`).
    pub all_canons: bool,

    /// Optional explicit canon file paths or glob patterns restricting candidate canons (`--canon <path|glob>`).
    pub canon_patterns: Vec<String>,
}

/// Domain errors produced during `discover` execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoverError {
    /// Invoked without `caseload.intake` and without `all_canons: true`.
    MissingInputSource,
    /// Invalid glob pattern syntax in `--canon` or target directives.
    InvalidGlob(GlobSyntaxError),
    /// Target or canon path attempts directory traversal (`..`) escaping the workspace root.
    PathEscapesWorkspace { path: String },
    /// Workspace filesystem traversal or file read error.
    Io { path: String, message: String },
}

impl fmt::Display for DiscoverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingInputSource => write!(
                f,
                "No filing input source or `--all-canons` scope provided for `discover`.\n  \
                 Hint: Pass `--all-canons` to enumerate all repository canons, \
                 pipe a diff (`git diff origin/main | canon-clerk discover --diff -`), \
                 specify target paths (`canon-clerk discover src/lib.rs`), \
                 or pass `--caseload <PATH>`."
            ),
            Self::InvalidGlob(err) => write!(
                f,
                "{err}\n  Hint: Check for unclosed `[` brackets or `{{` braces in glob patterns."
            ),
            Self::PathEscapesWorkspace { path } => write!(
                f,
                "Path `{path}` escapes the workspace root directory.\n  \
                 Hint: All target and canon paths must reside within the repository workspace."
            ),
            Self::Io { path, message } => write!(f, "Failed to read `{path}`: {message}"),
        }
    }
}

impl std::error::Error for DiscoverError {}

impl From<GlobSyntaxError> for DiscoverError {
    fn from(err: GlobSyntaxError) -> Self {
        Self::InvalidGlob(err)
    }
}

struct EvaluatedCanonMatch {
    matched_files: Vec<String>,
    inspected_metadata: Vec<String>,
    active_pr_title: bool,
    active_pr_body: bool,
    active_linked_issues: bool,
}

/// Executes the pure `discover` domain stage against an in-memory [`WorkspaceSnapshot`].
pub fn execute_discover(
    options: &DiscoverOptions,
    caseload: Caseload,
    workspace: &WorkspaceSnapshot,
) -> Result<Caseload, DiscoverError> {
    if caseload.discovery.is_some() && !options.all_canons && options.canon_patterns.is_empty() {
        return Ok(caseload);
    }

    if caseload.intake.is_none() && !options.all_canons {
        return Err(DiscoverError::MissingInputSource);
    }

    let filtered_canons = filter_canons(&workspace.canons, &options.canon_patterns)?;
    let target_files = match &caseload.intake {
        Some(intake) => materialize_target_files(intake, workspace)?,
        None => Vec::new(),
    };

    if options.all_canons {
        let candidate_canons: Vec<String> = filtered_canons.keys().cloned().collect();
        let mut triggers_join = BTreeMap::new();
        for canon_ref in &candidate_canons {
            triggers_join.insert(canon_ref.clone(), Vec::new());
        }
        let discovery = CaseloadDiscovery {
            mode: DiscoveryMode::AllCanons,
            target_files: target_files.clone(),
            candidate_canons,
            active_exhibits: ActiveExhibits {
                files: target_files,
                ..ActiveExhibits::default()
            },
            triggers_join,
        };
        return Ok(caseload.with_discovery(discovery));
    }

    let intake = caseload.intake.clone().unwrap_or_default();
    let target_file_tree = build_target_file_tree(&intake, &target_files, workspace);

    let mut candidate_canons = Vec::new();
    let mut active_files_set = BTreeSet::new();
    let mut active_pr_title = false;
    let mut active_pr_body = false;
    let mut active_linked_issues = false;
    let mut triggers_join = BTreeMap::new();

    for (canon_ref, canon) in filtered_canons {
        if let Some(eval) =
            evaluate_candidate_canon(canon, &intake, &target_files, &target_file_tree)
        {
            candidate_canons.push(canon_ref.clone());
            for f in &eval.matched_files {
                active_files_set.insert(f.clone());
            }
            active_pr_title |= eval.active_pr_title;
            active_pr_body |= eval.active_pr_body;
            active_linked_issues |= eval.active_linked_issues;

            let mut join_items = eval.matched_files;
            join_items.extend(eval.inspected_metadata);
            triggers_join.insert(canon_ref.clone(), join_items);
        }
    }

    let active_exhibits = ActiveExhibits {
        files: active_files_set.into_iter().collect(),
        pr_title: active_pr_title.then_some(true),
        pr_body: active_pr_body.then_some(true),
        commit_messages: None,
        linked_issues: active_linked_issues.then_some(true),
    };

    let discovery = CaseloadDiscovery {
        mode: DiscoveryMode::Triggered,
        target_files,
        candidate_canons,
        active_exhibits,
        triggers_join,
    };

    Ok(caseload.with_discovery(discovery))
}

/// Materializes a [`WorkspaceSnapshot`] from `workspace_root` on disk and executes [`execute_discover`].
pub fn execute_discover_on_disk(
    options: &DiscoverOptions,
    caseload: Caseload,
    workspace_root: &Path,
) -> Result<Caseload, DiscoverError> {
    if caseload.discovery.is_some() && !options.all_canons && options.canon_patterns.is_empty() {
        return Ok(caseload);
    }
    if caseload.intake.is_none() && !options.all_canons {
        return Err(DiscoverError::MissingInputSource);
    }
    let snapshot = WorkspaceSnapshot::from_disk(workspace_root, &options.canon_patterns)?;
    execute_discover(options, caseload, &snapshot)
}

fn filter_canons<'a>(
    canons: &'a BTreeMap<String, Canon>,
    canon_patterns: &[String],
) -> Result<BTreeMap<String, &'a Canon>, DiscoverError> {
    let matchers = compile_canon_filter_globs(canon_patterns)?;
    let mut out = BTreeMap::new();
    for (key, canon) in canons {
        if matchers.is_empty() || matchers.iter().any(|m| m.is_match(key)) {
            out.insert(key.clone(), canon);
        }
    }
    Ok(out)
}

fn materialize_target_files(
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
) -> Result<Vec<String>, DiscoverError> {
    let mut resolved = BTreeSet::new();

    for path in intake.diffs.keys() {
        let normalized = normalize_workspace_path(path)
            .map_err(|_| DiscoverError::PathEscapesWorkspace { path: path.clone() })?;
        if normalized != "." {
            resolved.insert(normalized);
        }
    }

    if intake.scope == IntakeScope::AllTargets {
        for file in &workspace.existing_files {
            resolved.insert(file.clone());
        }
    }

    for directive in &intake.target_paths {
        if is_wildcard_pattern(directive) {
            if directive.replace('\\', "/").split('/').any(|s| s == "..") {
                return Err(DiscoverError::PathEscapesWorkspace {
                    path: directive.clone(),
                });
            }
            let glob = GlobPattern::parse(directive)?;
            for file in &workspace.existing_files {
                if glob.is_match(file) {
                    resolved.insert(file.clone());
                }
            }
        } else {
            let normalized = normalize_workspace_path(directive).map_err(|_| {
                DiscoverError::PathEscapesWorkspace {
                    path: directive.clone(),
                }
            })?;
            if normalized == "." {
                for file in &workspace.existing_files {
                    resolved.insert(file.clone());
                }
            } else {
                let dir_prefix = format!("{normalized}/");
                let dir_matches: Vec<&String> = workspace
                    .existing_files
                    .iter()
                    .filter(|f| f.starts_with(&dir_prefix))
                    .collect();
                if !dir_matches.is_empty() {
                    for f in dir_matches {
                        resolved.insert(f.clone());
                    }
                } else {
                    resolved.insert(normalized);
                }
            }
        }
    }

    Ok(resolved.into_iter().collect())
}

fn build_target_file_tree(
    intake: &CaseloadIntake,
    target_files: &[String],
    workspace: &WorkspaceSnapshot,
) -> BTreeSet<String> {
    let mut tree = workspace.existing_files.clone();
    for f in target_files {
        tree.insert(f.clone());
    }
    for (path, artifact) in &intake.diffs {
        if artifact.status == FileChangeStatus::Deleted {
            tree.remove(path);
        } else {
            tree.insert(path.clone());
        }
    }
    tree
}

fn evaluate_candidate_canon(
    canon: &Canon,
    intake: &CaseloadIntake,
    target_files: &[String],
    target_file_tree: &BTreeSet<String>,
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

    let matched_files = match_canon_triggers(canon, canon_scope.as_ref(), &in_scope_targets);
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
    let mut active_linked_issues = false;

    for plane in &canon.inspect {
        let available = match plane.token {
            InspectToken::Diff => has_diff_match,
            InspectToken::PrTitle => has_pr_title,
            InspectToken::PrBody => has_pr_body,
            InspectToken::CommitMessages => false,
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
                InspectToken::LinkedIssues => active_linked_issues = true,
                InspectToken::Diff | InspectToken::CommitMessages => {}
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
        active_linked_issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::caseload::{FileArtifact, LinkedIssueContext};

    fn sample_workspace() -> WorkspaceSnapshot {
        WorkspaceSnapshot::new()
            .with_file("src/lib.rs")
            .with_file("src/cli/mod.rs")
            .with_file("packages/ui/package.json")
            .with_file("packages/ui/src/button.tsx")
            .with_file("docs/README.md")
            .with_canon_markdown(
                "---\ntriggers:\n  - \"src/**/*.rs\"\n---\n# Rust Code Rule\n\nRust code MUST obey invariants.",
                &ParseCanonOptions::with_source_ref(".canons/rust/rust-code-rule.md"),
            )
            .with_canon_markdown(
                "---\ntriggers:\n  - \"src/**/*.tsx\"\nexists:\n  - \"package.json\"\n---\n# UI Button Rule\n\nUI components MUST use tokens.",
                &ParseCanonOptions::with_source_ref("packages/ui/.canons/ui-button-rule.md"),
            )
            .with_canon_markdown(
                "---\ninspect:\n  - pr_title\n---\n# PR Title Rule\n\nPR titles MUST use Conventional Commits.",
                &ParseCanonOptions::with_source_ref(".canons/git/pr-title-rule.md"),
            )
    }

    mod source_and_fast_forward {
        use super::*;

        #[test]
        fn test_missing_intake_without_all_canons_fails() {
            let ws = sample_workspace();
            let err = execute_discover(&DiscoverOptions::default(), Caseload::new(), &ws)
                .expect_err("naked discover must fail");
            assert_eq!(err, DiscoverError::MissingInputSource);
            assert!(err.to_string().contains("Hint:"));
        }

        #[test]
        fn test_fast_forwards_when_discovery_already_present() {
            let ws = sample_workspace();
            let existing = Caseload::new().with_discovery(CaseloadDiscovery {
                mode: DiscoveryMode::Triggered,
                target_files: vec!["src/lib.rs".to_string()],
                candidate_canons: vec![".canons/rust/rust-code-rule.md".to_string()],
                active_exhibits: ActiveExhibits::default(),
                triggers_join: BTreeMap::new(),
            });

            let result = execute_discover(&DiscoverOptions::default(), existing.clone(), &ws)
                .expect("fast-forward must succeed");
            assert_eq!(result, existing);
        }
    }

    mod exhibit_materialization {
        use super::*;

        #[test]
        fn test_materializes_directory_and_glob_target_directives() {
            let ws = sample_workspace();
            let intake = CaseloadIntake {
                target_paths: vec!["src".to_string(), "packages/ui/**/*.tsx".to_string()],
                ..CaseloadIntake::default()
            };
            let caseload = Caseload::new().with_intake(intake);
            let enriched =
                execute_discover(&DiscoverOptions::default(), caseload, &ws).expect("discover");
            let disc = enriched.discovery.expect("discovery populated");

            assert_eq!(
                disc.target_files,
                vec![
                    "packages/ui/src/button.tsx".to_string(),
                    "src/cli/mod.rs".to_string(),
                    "src/lib.rs".to_string(),
                ]
            );
        }

        #[test]
        fn test_rejects_target_path_escaping_workspace() {
            let ws = sample_workspace();
            let intake = CaseloadIntake {
                target_paths: vec!["../outside.rs".to_string()],
                ..CaseloadIntake::default()
            };
            let caseload = Caseload::new().with_intake(intake);
            let err = execute_discover(&DiscoverOptions::default(), caseload, &ws)
                .expect_err("traversal must fail");
            assert!(matches!(err, DiscoverError::PathEscapesWorkspace { .. }));
        }
    }

    mod plenary_all_canons {
        use super::*;

        #[test]
        fn test_all_canons_promotes_entire_corpus_without_intake() {
            let ws = sample_workspace();
            let opts = DiscoverOptions {
                all_canons: true,
                canon_patterns: Vec::new(),
            };
            let enriched = execute_discover(&opts, Caseload::new(), &ws).expect("all-canons");
            let disc = enriched.discovery.expect("discovery populated");

            assert_eq!(disc.mode, DiscoveryMode::AllCanons);
            assert_eq!(
                disc.candidate_canons,
                vec![
                    ".canons/git/pr-title-rule.md".to_string(),
                    ".canons/rust/rust-code-rule.md".to_string(),
                    "packages/ui/.canons/ui-button-rule.md".to_string(),
                ]
            );
        }
    }

    mod scope_and_exists_preconditions {
        use super::*;

        #[test]
        fn test_scoped_canon_only_activates_on_files_inside_scope_with_met_exists() {
            let ws = sample_workspace();
            let intake = CaseloadIntake {
                target_paths: vec!["src/lib.rs".to_string()],
                ..CaseloadIntake::default()
            };
            let enriched = execute_discover(
                &DiscoverOptions::default(),
                Caseload::new().with_intake(intake),
                &ws,
            )
            .expect("discover");
            let disc = enriched.discovery.expect("discovery");

            assert_eq!(
                disc.candidate_canons,
                vec![".canons/rust/rust-code-rule.md".to_string()]
            );

            let ui_intake = CaseloadIntake {
                target_paths: vec!["packages/ui/src/button.tsx".to_string()],
                ..CaseloadIntake::default()
            };
            let ui_enriched = execute_discover(
                &DiscoverOptions::default(),
                Caseload::new().with_intake(ui_intake),
                &ws,
            )
            .expect("discover ui");
            let ui_disc = ui_enriched.discovery.expect("ui discovery");
            assert_eq!(
                ui_disc.candidate_canons,
                vec!["packages/ui/.canons/ui-button-rule.md".to_string()]
            );
        }

        #[test]
        fn test_prunes_canon_when_exists_precondition_deleted_in_diff() {
            let ws = sample_workspace();
            let mut diffs = BTreeMap::new();
            diffs.insert(
                "packages/ui/package.json".to_string(),
                FileArtifact {
                    path: "packages/ui/package.json".to_string(),
                    status: FileChangeStatus::Deleted,
                    previous_path: None,
                    lines_added: 0,
                    lines_deleted: 10,
                    patch: None,
                    patch_omission_reason: None,
                    content: None,
                    content_omission_reason: None,
                },
            );
            let intake = CaseloadIntake {
                target_paths: vec!["packages/ui/src/button.tsx".to_string()],
                diffs,
                ..CaseloadIntake::default()
            };
            let enriched = execute_discover(
                &DiscoverOptions::default(),
                Caseload::new().with_intake(intake),
                &ws,
            )
            .expect("discover with deleted exists file");
            let disc = enriched.discovery.expect("discovery");

            assert!(disc.candidate_canons.is_empty());
        }
    }

    mod bipartite_inspect_and_mutual_pruning {
        use super::*;

        #[test]
        fn test_mutual_pruning_drops_uninspected_files_and_metadata() {
            let ws = sample_workspace();
            let intake = CaseloadIntake {
                pr_title: Some("feat: update rust and docs".to_string()),
                linked_issues: Some(vec![LinkedIssueContext {
                    number: 1,
                    title: None,
                    body: None,
                    url: None,
                }]),
                target_paths: vec!["src/lib.rs".to_string(), "docs/README.md".to_string()],
                ..CaseloadIntake::default()
            };
            let enriched = execute_discover(
                &DiscoverOptions::default(),
                Caseload::new().with_intake(intake),
                &ws,
            )
            .expect("discover");
            let disc = enriched.discovery.expect("discovery");

            // docs/README.md is not matched by any canon, so it is pruned from active_exhibits.files
            assert_eq!(disc.active_exhibits.files, vec!["src/lib.rs".to_string()]);
            assert_eq!(disc.active_exhibits.pr_title, Some(true));
            // linked_issues is not inspected by any surviving canon, so it is pruned (None)
            assert_eq!(disc.active_exhibits.linked_issues, None);
        }
    }
}
