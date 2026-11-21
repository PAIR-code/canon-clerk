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

mod evaluate;
pub mod glob_matcher;
mod policy;
pub mod workspace;

pub use glob_matcher::{
    DEFAULT_CANON_GLOB, DEFAULT_IGNORES, GlobPattern, GlobSyntaxError, is_ignored_path,
    is_wildcard_pattern, normalize_workspace_path, scope_relative_path,
};
pub use workspace::{WorkspaceSnapshot, parse_canon_or_fallback};

use crate::canon::{Canon, CanonSourceKind};
use crate::caseload::{
    ActiveExhibits, Caseload, CaseloadDiscovery, CaseloadIntake, DiscoveryMode, FileChangeStatus,
    IntakeScope,
};
use evaluate::evaluate_candidate_canon;
use policy::{WorkspacePolicySet, detect_nested_policy_configs};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;
use workspace::compile_canon_filter_globs;

/// Domain input parameters for executing the `discover` pipeline stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoverOptions {
    /// Whether to bypass path trigger intersection and promote all discoverable canons (`--all-canons`).
    pub all_canons: bool,

    /// Optional explicit canon file paths or glob patterns restricting candidate canons (`--canon <path|glob>`).
    pub canon_patterns: Vec<String>,

    /// Whether compile-time embedded `builtin:*` standard canons are included during discovery (`--builtins` / `--no-builtins`; default: `true`).
    pub include_builtins: bool,
}

impl Default for DiscoverOptions {
    fn default() -> Self {
        Self {
            all_canons: false,
            canon_patterns: Vec::new(),
            include_builtins: true,
        }
    }
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
    /// Malformed or schema-invalid `<scope>/.canons/config.yaml` policy file (`SPEC.md` §7.2).
    InvalidPolicyConfig { path: String, message: String },
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
            Self::InvalidPolicyConfig { path, message } => write!(
                f,
                "Invalid workspace policy configuration `{path}`: {message}\n  \
                 Hint: See SPEC.md §7 for the `.canons/config.yaml` schema."
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

/// Executes the pure `discover` domain stage against an in-memory [`WorkspaceSnapshot`].
pub fn execute_discover(
    options: &DiscoverOptions,
    caseload: Caseload,
    workspace: &WorkspaceSnapshot,
) -> Result<Caseload, DiscoverError> {
    if caseload.discovery.is_some()
        && !options.all_canons
        && options.canon_patterns.is_empty()
        && options.include_builtins
    {
        return Ok(caseload);
    }

    if caseload.intake.is_none() && !options.all_canons {
        return Err(DiscoverError::MissingInputSource);
    }

    for nested_path in detect_nested_policy_configs(&workspace.existing_files) {
        tracing::warn!(
            path = %nested_path,
            "Ignoring nested `config.yaml` inside `.canons/` subdirectory; place workspace policy at `<scope>/.canons/config.yaml`"
        );
    }

    let policy_set = WorkspacePolicySet::compile(&workspace.policy_configs)?;
    let config_files = policy_set.config_files().to_vec();
    let active_policy = options.canon_patterns.is_empty().then_some(&policy_set);

    let filtered_canons = filter_canons(
        &workspace.canons,
        &options.canon_patterns,
        options.include_builtins,
    )?;
    let target_files = match &caseload.intake {
        Some(intake) => materialize_target_files(intake, workspace)?,
        None => Vec::new(),
    };

    if options.all_canons {
        let candidate_canons: Vec<String> = filtered_canons
            .iter()
            .filter(|(canon_ref, canon)| {
                active_policy.is_none_or(|p| p.allows_unconditional(canon_ref, canon))
            })
            .map(|(canon_ref, _)| canon_ref.clone())
            .collect();
        let mut triggers_join = BTreeMap::new();
        for canon_ref in &candidate_canons {
            triggers_join.insert(canon_ref.clone(), Vec::new());
        }
        let discovery = CaseloadDiscovery {
            mode: DiscoveryMode::AllCanons,
            target_files: target_files.clone(),
            config_files,
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
        if let Some(eval) = evaluate_candidate_canon(
            &canon_ref,
            canon,
            &intake,
            &target_files,
            &target_file_tree,
            active_policy,
        ) {
            candidate_canons.push(canon_ref.clone());
            for f in &eval.matched_files {
                active_files_set.insert(f.clone());
            }
            active_pr_title |= eval.active_pr_title;
            active_pr_body |= eval.active_pr_body;
            active_linked_issues |= eval.active_linked_issues;

            let mut join_items = eval.matched_files;
            join_items.extend(eval.inspected_metadata);
            triggers_join.insert(canon_ref, join_items);
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
        config_files,
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
    if caseload.discovery.is_some()
        && !options.all_canons
        && options.canon_patterns.is_empty()
        && options.include_builtins
    {
        return Ok(caseload);
    }
    if caseload.intake.is_none() && !options.all_canons {
        return Err(DiscoverError::MissingInputSource);
    }
    let snapshot = WorkspaceSnapshot::from_disk_with_builtins(
        workspace_root,
        &options.canon_patterns,
        options.include_builtins,
    )?;
    execute_discover(options, caseload, &snapshot)
}

fn filter_canons<'a>(
    canons: &'a BTreeMap<String, Canon>,
    canon_patterns: &[String],
    include_builtins: bool,
) -> Result<BTreeMap<String, &'a Canon>, DiscoverError> {
    let matchers = compile_canon_filter_globs(canon_patterns)?;
    let mut out = BTreeMap::new();
    for (key, canon) in canons {
        if !include_builtins && canon.source_kind == Some(CanonSourceKind::Builtin) {
            continue;
        }
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
                config_files: Vec::new(),
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
                ..DiscoverOptions::default()
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

    mod builtin_sourcing_and_isolation {
        use super::*;

        #[test]
        fn test_builtins_do_not_pollute_existing_files_or_all_targets() {
            let ws = WorkspaceSnapshot::new()
                .with_file("src/lib.rs")
                .with_builtins();
            assert_eq!(
                ws.existing_files,
                BTreeSet::from(["src/lib.rs".to_string()])
            );
            assert!(
                ws.canons
                    .contains_key("builtin:cli/cli-flags-must-use-positive-polarity")
            );

            let intake = CaseloadIntake {
                scope: IntakeScope::AllTargets,
                ..CaseloadIntake::default()
            };
            let enriched = execute_discover(
                &DiscoverOptions::default(),
                Caseload::new().with_intake(intake),
                &ws,
            )
            .expect("discover --all-targets with builtins");
            let disc = enriched.discovery.expect("discovery");
            assert_eq!(disc.target_files, vec!["src/lib.rs".to_string()]);
            assert!(
                disc.target_files
                    .iter()
                    .all(|path| !path.starts_with("builtin:"))
            );
        }

        #[test]
        fn test_include_builtins_false_excludes_embedded_canons_while_keeping_local() {
            let ws = WorkspaceSnapshot::new()
                .with_file("src/lib.rs")
                .with_canon_markdown(
                    "---\ntriggers:\n  - \"src/**/*.rs\"\n---\nLocal rule MUST hold.\n",
                    &ParseCanonOptions::with_source_ref(".canons/local-rule.md"),
                )
                .with_builtins();

            let opts = DiscoverOptions {
                all_canons: true,
                include_builtins: false,
                ..DiscoverOptions::default()
            };
            let enriched = execute_discover(&opts, Caseload::new(), &ws).expect("no-builtins");
            let disc = enriched.discovery.expect("discovery");
            assert_eq!(
                disc.candidate_canons,
                vec![".canons/local-rule.md".to_string()]
            );
        }

        #[test]
        fn test_builtin_canon_filter_globs_support_exact_prefix_and_wildcard_selectors() {
            let ws = WorkspaceSnapshot::new().with_builtins();

            // 1. Exact ref (without .md)
            let exact = execute_discover(
                &DiscoverOptions {
                    all_canons: true,
                    canon_patterns: vec![
                        "builtin:cli/cli-flags-must-use-positive-polarity".to_string(),
                    ],
                    include_builtins: true,
                },
                Caseload::new(),
                &ws,
            )
            .expect("exact builtin ref");
            assert_eq!(
                exact.discovery.expect("disc").candidate_canons,
                vec!["builtin:cli/cli-flags-must-use-positive-polarity".to_string()]
            );

            // 2. Exact ref with optional .canon.md suffix
            let exact_canon_md = execute_discover(
                &DiscoverOptions {
                    all_canons: true,
                    canon_patterns: vec![
                        "builtin:cli/cli-flags-must-use-positive-polarity.canon.md".to_string(),
                    ],
                    include_builtins: true,
                },
                Caseload::new(),
                &ws,
            )
            .expect("exact builtin ref with .canon.md");
            assert_eq!(
                exact_canon_md.discovery.expect("disc").candidate_canons,
                vec!["builtin:cli/cli-flags-must-use-positive-polarity".to_string()]
            );

            // 2b. Plain .md suffix is strictly NOT stripped for builtin refs
            let plain_md = execute_discover(
                &DiscoverOptions {
                    all_canons: true,
                    canon_patterns: vec![
                        "builtin:cli/cli-flags-must-use-positive-polarity.md".to_string(),
                    ],
                    include_builtins: true,
                },
                Caseload::new(),
                &ws,
            )
            .expect("plain .md builtin ref");
            assert!(
                plain_md
                    .discovery
                    .expect("disc")
                    .candidate_canons
                    .is_empty()
            );

            // 3. Domain prefix (`builtin:cli`)
            let domain = execute_discover(
                &DiscoverOptions {
                    all_canons: true,
                    canon_patterns: vec!["builtin:cli".to_string()],
                    include_builtins: true,
                },
                Caseload::new(),
                &ws,
            )
            .expect("domain prefix");
            let domain_canons = domain.discovery.expect("disc").candidate_canons;
            assert!(!domain_canons.is_empty());
            assert!(domain_canons.iter().all(|c| c.starts_with("builtin:cli/")));

            // 4. Glob (`builtin:security/**`)
            let security = execute_discover(
                &DiscoverOptions {
                    all_canons: true,
                    canon_patterns: vec!["builtin:security/**".to_string()],
                    include_builtins: true,
                },
                Caseload::new(),
                &ws,
            )
            .expect("security glob");
            let sec_canons = security.discovery.expect("disc").candidate_canons;
            assert!(!sec_canons.is_empty());
            assert!(
                sec_canons
                    .iter()
                    .all(|c| c.starts_with("builtin:security/"))
            );

            // 5. Wildcard (`builtin:**`)
            let all_b = execute_discover(
                &DiscoverOptions {
                    all_canons: true,
                    canon_patterns: vec!["builtin:**".to_string()],
                    include_builtins: true,
                },
                Caseload::new(),
                &ws,
            )
            .expect("builtin wildcard");
            assert_eq!(
                all_b.discovery.expect("disc").candidate_canons.len(),
                crate::builtin::builtin_canons().len()
            );
        }

        #[test]
        fn test_canon_authoring_triggers_match_builtins_and_local_canons_but_not_readme() {
            let ws = WorkspaceSnapshot::new()
                .with_file(".canons/README.md")
                .with_file(".canons/local-rule.canon.md")
                .with_file("builtins/cli/example-rule.canon.md")
                .with_builtins();

            // 1. Modifying builtins/**/*.canon.md triggers builtin:canon-authoring/*
            let builtin_edit = Caseload::new().with_intake(CaseloadIntake {
                target_paths: vec!["builtins/cli/example-rule.canon.md".to_string()],
                ..CaseloadIntake::default()
            });
            let disc_builtin = execute_discover(
                &DiscoverOptions {
                    canon_patterns: vec!["builtin:canon-authoring".to_string()],
                    include_builtins: true,
                    ..DiscoverOptions::default()
                },
                builtin_edit,
                &ws,
            )
            .expect("discover builtins canon edit")
            .discovery
            .expect("discovery");
            assert!(
                disc_builtin
                    .candidate_canons
                    .contains(&"builtin:canon-authoring/canons-must-be-atomic".to_string())
            );

            // 2. Modifying .canons/*.canon.md triggers builtin:canon-authoring/*
            let local_edit = Caseload::new().with_intake(CaseloadIntake {
                target_paths: vec![".canons/local-rule.canon.md".to_string()],
                ..CaseloadIntake::default()
            });
            let disc_local = execute_discover(
                &DiscoverOptions {
                    canon_patterns: vec!["builtin:canon-authoring".to_string()],
                    include_builtins: true,
                    ..DiscoverOptions::default()
                },
                local_edit,
                &ws,
            )
            .expect("discover local canon edit")
            .discovery
            .expect("discovery");
            assert!(
                disc_local
                    .candidate_canons
                    .contains(&"builtin:canon-authoring/canons-must-be-atomic".to_string())
            );

            // 3. Modifying non-canon .canons/README.md does NOT trigger builtin:canon-authoring/*
            let readme_edit = Caseload::new().with_intake(CaseloadIntake {
                target_paths: vec![".canons/README.md".to_string()],
                ..CaseloadIntake::default()
            });
            let disc_readme = execute_discover(
                &DiscoverOptions {
                    canon_patterns: vec!["builtin:canon-authoring".to_string()],
                    include_builtins: true,
                    ..DiscoverOptions::default()
                },
                readme_edit,
                &ws,
            )
            .expect("discover .canons/README.md edit")
            .discovery
            .expect("discovery");
            assert!(disc_readme.candidate_canons.is_empty());
        }
    }

    mod workspace_policy_config {
        use super::*;
        use crate::caseload::WorkspaceState;

        fn monorepo_workspace() -> WorkspaceSnapshot {
            WorkspaceSnapshot::new()
                .with_file("src/lib.rs")
                .with_file("src/legacy/old.rs")
                .with_file("packages/ui/src/button.tsx")
                .with_file("packages/ui/src/generated.tsx")
                .with_file("packages/ui/vendor/lib.tsx")
                .with_file("packages/ui/nested/deep.tsx")
                .with_canon_markdown(
                    "---\ntriggers:\n  - \"**/*.{rs,tsx}\"\n---\n# Root Arch Rule\n\nRoot arch rule MUST hold.",
                    &ParseCanonOptions::with_source_ref(".canons/architecture/foo.canon.md"),
                )
                .with_canon_markdown(
                    "---\ntriggers:\n  - \"**/*.{rs,tsx}\"\n---\n# Root Style Rule\n\nRoot style rule MUST hold.",
                    &ParseCanonOptions::with_source_ref(".canons/style/bar.canon.md"),
                )
                .with_canon_markdown(
                    "---\ninspect:\n  - pr_title\n---\n# PR Title Policy Rule\n\nPR titles MUST be concise.",
                    &ParseCanonOptions::with_source_ref(".canons/git/pr-title.canon.md"),
                )
                .with_canon_markdown(
                    "---\ntriggers:\n  - \"**/*.tsx\"\n---\n# UI Local Rule\n\nUI components MUST use tokens.",
                    &ParseCanonOptions::with_source_ref("packages/ui/.canons/ui/tokens.canon.md"),
                )
                .with_canon_markdown(
                    "---\ntriggers:\n  - \"**/*.tsx\"\n---\n# Nested Local Rule\n\nNested code MUST be pure.",
                    &ParseCanonOptions::with_source_ref(
                        "packages/ui/nested/.canons/pure.canon.md",
                    ),
                )
        }

        #[test]
        fn test_ordered_first_match_wins_and_default_allow() {
            let ws = monorepo_workspace().with_policy_config(
                ".canons/config.yaml",
                "rules:\n  - allow: \"architecture/foo\"\n  - deny: \"architecture/**\"\n",
            );
            let intake = CaseloadIntake {
                target_paths: vec!["src/lib.rs".to_string()],
                ..CaseloadIntake::default()
            };
            let disc = execute_discover(
                &DiscoverOptions {
                    include_builtins: false,
                    ..DiscoverOptions::default()
                },
                Caseload::new().with_intake(intake),
                &ws,
            )
            .expect("discover")
            .discovery
            .expect("discovery");

            assert_eq!(disc.config_files, vec![".canons/config.yaml".to_string()]);
            assert_eq!(
                disc.candidate_canons,
                vec![
                    ".canons/architecture/foo.canon.md".to_string(),
                    ".canons/style/bar.canon.md".to_string(),
                ]
            );
        }

        #[test]
        fn test_nearest_scope_cascade_fallthrough_and_subtree_isolation() {
            let ws = monorepo_workspace()
                .with_policy_config(
                    ".canons/config.yaml",
                    "rules:\n  - deny: \"style/bar\"\n",
                )
                .with_policy_config(
                    "packages/ui/.canons/config.yaml",
                    "rules:\n  - allow: \"parents:style/bar\"\n  - deny: \"parents:architecture/foo\"\n",
                )
                .with_policy_config(
                    "packages/ui/vendor/.canons/config.yaml",
                    "rules:\n  - deny: all\n",
                );

            // 1. In packages/ui/src/button.tsx: child re-allows parents:style/bar, denies parents:architecture/foo,
            //    and allows its own ui/tokens by default.
            let ui_disc = execute_discover(
                &DiscoverOptions {
                    include_builtins: false,
                    ..DiscoverOptions::default()
                },
                Caseload::new().with_intake(CaseloadIntake {
                    target_paths: vec!["packages/ui/src/button.tsx".to_string()],
                    ..CaseloadIntake::default()
                }),
                &ws,
            )
            .expect("ui discover")
            .discovery
            .expect("discovery");
            assert_eq!(
                ui_disc.config_files,
                vec![
                    ".canons/config.yaml".to_string(),
                    "packages/ui/.canons/config.yaml".to_string(),
                    "packages/ui/vendor/.canons/config.yaml".to_string(),
                ]
            );
            assert_eq!(
                ui_disc.candidate_canons,
                vec![
                    ".canons/style/bar.canon.md".to_string(),
                    "packages/ui/.canons/ui/tokens.canon.md".to_string(),
                ]
            );

            // 2. In packages/ui/vendor/lib.tsx: `- deny: all` isolates the subtree completely.
            let vendor_disc = execute_discover(
                &DiscoverOptions {
                    include_builtins: false,
                    ..DiscoverOptions::default()
                },
                Caseload::new().with_intake(CaseloadIntake {
                    target_paths: vec!["packages/ui/vendor/lib.tsx".to_string()],
                    ..CaseloadIntake::default()
                }),
                &ws,
            )
            .expect("vendor discover")
            .discovery
            .expect("discovery");
            assert!(vendor_disc.candidate_canons.is_empty());
        }

        #[test]
        fn test_parents_selector_multilevel_and_root_noop() {
            let ws = monorepo_workspace()
                .with_policy_config(
                    ".canons/config.yaml",
                    // `parents:` in root config matches zero canons
                    "rules:\n  - deny: \"parents:**\"\n",
                )
                .with_policy_config(
                    "packages/ui/nested/.canons/config.yaml",
                    // Disables both root `architecture/foo` and intermediate `ui/tokens`
                    "rules:\n  - deny: \"parents:architecture/foo\"\n  - deny: \"parents:ui/*\"\n",
                );

            let nested_disc = execute_discover(
                &DiscoverOptions {
                    include_builtins: false,
                    ..DiscoverOptions::default()
                },
                Caseload::new().with_intake(CaseloadIntake {
                    target_paths: vec!["packages/ui/nested/deep.tsx".to_string()],
                    ..CaseloadIntake::default()
                }),
                &ws,
            )
            .expect("nested discover")
            .discovery
            .expect("discovery");
            assert_eq!(
                nested_disc.candidate_canons,
                vec![
                    ".canons/style/bar.canon.md".to_string(),
                    "packages/ui/nested/.canons/pure.canon.md".to_string(),
                ]
            );

            // Root `parents:**` did not deny root canons on `src/lib.rs`
            let root_disc = execute_discover(
                &DiscoverOptions {
                    include_builtins: false,
                    ..DiscoverOptions::default()
                },
                Caseload::new().with_intake(CaseloadIntake {
                    target_paths: vec!["src/lib.rs".to_string()],
                    ..CaseloadIntake::default()
                }),
                &ws,
            )
            .expect("root discover")
            .discovery
            .expect("discovery");
            assert_eq!(
                root_disc.candidate_canons,
                vec![
                    ".canons/architecture/foo.canon.md".to_string(),
                    ".canons/style/bar.canon.md".to_string(),
                ]
            );
        }

        #[test]
        fn test_canon_md_extension_normalization_and_exhibits_filtering() {
            let ws = monorepo_workspace().with_policy_config(
                ".canons/config.yaml",
                "rules:\n  - deny: \"architecture/foo.canon.md\"\n    exhibits: \"src/legacy/**\"\n  - deny: \"style/*.canon.md\"\n    exhibits: [\"src/legacy/**\", \"src/lib.rs\"]\n",
            );

            let disc = execute_discover(
                &DiscoverOptions {
                    include_builtins: false,
                    ..DiscoverOptions::default()
                },
                Caseload::new().with_intake(CaseloadIntake {
                    target_paths: vec!["src/lib.rs".to_string(), "src/legacy/old.rs".to_string()],
                    ..CaseloadIntake::default()
                }),
                &ws,
            )
            .expect("discover")
            .discovery
            .expect("discovery");

            // architecture/foo survives on src/lib.rs (only src/legacy/old.rs was filtered out)
            // style/bar has zero surviving exhibits and is dropped
            assert_eq!(
                disc.candidate_canons,
                vec![".canons/architecture/foo.canon.md".to_string()]
            );
            assert_eq!(
                disc.triggers_join.get(".canons/architecture/foo.canon.md"),
                Some(&vec!["src/lib.rs".to_string()])
            );
            assert_eq!(disc.active_exhibits.files, vec!["src/lib.rs".to_string()]);
        }

        #[test]
        fn test_all_canons_and_metadata_only_respect_unconditional_rules_only() {
            let ws = monorepo_workspace()
                .with_policy_config(
                    ".canons/config.yaml",
                    "rules:\n  - deny: \"architecture/foo\"\n    exhibits: \"src/**\"\n  - deny: \"style/bar\"\n  - deny: \"git/pr-title\"\n    exhibits: \"src/**\"\n",
                )
                .with_policy_config(
                    "packages/ui/.canons/config.yaml",
                    "rules:\n  - deny: \"ui/tokens\"\n",
                );

            // --all-canons drops unconditional denies (`style/bar`, `ui/tokens`) but keeps rules with `exhibits:`
            let all_disc = execute_discover(
                &DiscoverOptions {
                    all_canons: true,
                    include_builtins: false,
                    ..DiscoverOptions::default()
                },
                Caseload::new(),
                &ws,
            )
            .expect("all-canons")
            .discovery
            .expect("discovery");
            assert_eq!(
                all_disc.candidate_canons,
                vec![
                    ".canons/architecture/foo.canon.md".to_string(),
                    ".canons/git/pr-title.canon.md".to_string(),
                    "packages/ui/nested/.canons/pure.canon.md".to_string(),
                ]
            );

            // Metadata-only filing is unaffected by `deny: git/pr-title` with `exhibits: src/**`
            let meta_disc = execute_discover(
                &DiscoverOptions {
                    include_builtins: false,
                    ..DiscoverOptions::default()
                },
                Caseload::new().with_intake(CaseloadIntake {
                    pr_title: Some("feat: add policy".to_string()),
                    ..CaseloadIntake::default()
                }),
                &ws,
            )
            .expect("meta discover")
            .discovery
            .expect("discovery");
            assert_eq!(
                meta_disc.candidate_canons,
                vec![".canons/git/pr-title.canon.md".to_string()]
            );

            // Unconditional deny drops metadata-only canon
            let ws_denied_meta = monorepo_workspace().with_policy_config(
                ".canons/config.yaml",
                "rules:\n  - deny: \"git/pr-title\"\n",
            );
            let denied_meta_disc = execute_discover(
                &DiscoverOptions {
                    include_builtins: false,
                    ..DiscoverOptions::default()
                },
                Caseload::new().with_intake(CaseloadIntake {
                    pr_title: Some("feat: add policy".to_string()),
                    ..CaseloadIntake::default()
                }),
                &ws_denied_meta,
            )
            .expect("denied meta discover")
            .discovery
            .expect("discovery");
            assert!(denied_meta_disc.candidate_canons.is_empty());
        }

        #[test]
        fn test_cli_precedence_canon_no_builtins_and_builtins() {
            let ws = monorepo_workspace().with_builtins().with_policy_config(
                ".canons/config.yaml",
                "rules:\n  - allow: \"builtin:cli/**\"\n  - deny: \"builtin:**\"\n  - deny: \"architecture/foo\"\n",
            );

            // 1. --builtins (include_builtins: true) respects config.yaml deny for non-cli builtins
            let default_disc = execute_discover(
                &DiscoverOptions {
                    all_canons: true,
                    include_builtins: true,
                    ..DiscoverOptions::default()
                },
                Caseload::new(),
                &ws,
            )
            .expect("default builtins")
            .discovery
            .expect("discovery");
            assert!(
                default_disc
                    .candidate_canons
                    .iter()
                    .any(|c| c.starts_with("builtin:cli/"))
            );
            assert!(
                !default_disc
                    .candidate_canons
                    .iter()
                    .any(|c| c.starts_with("builtin:canon-authoring/"))
            );
            assert!(
                !default_disc
                    .candidate_canons
                    .contains(&".canons/architecture/foo.canon.md".to_string())
            );

            // 2. --no-builtins excludes all builtins despite `allow: builtin:cli/**`
            let no_builtins_disc = execute_discover(
                &DiscoverOptions {
                    all_canons: true,
                    include_builtins: false,
                    ..DiscoverOptions::default()
                },
                Caseload::new(),
                &ws,
            )
            .expect("no-builtins")
            .discovery
            .expect("discovery");
            assert!(
                !no_builtins_disc
                    .candidate_canons
                    .iter()
                    .any(|c| c.starts_with("builtin:"))
            );

            // 3. Explicit --canon overrides config.yaml deny rules
            let explicit_disc = execute_discover(
                &DiscoverOptions {
                    all_canons: true,
                    canon_patterns: vec![
                        ".canons/architecture/foo.canon.md".to_string(),
                        "builtin:canon-authoring/canons-must-be-atomic".to_string(),
                    ],
                    include_builtins: true,
                },
                Caseload::new(),
                &ws,
            )
            .expect("explicit --canon")
            .discovery
            .expect("discovery");
            assert_eq!(
                explicit_disc.candidate_canons,
                vec![
                    ".canons/architecture/foo.canon.md".to_string(),
                    "builtin:canon-authoring/canons-must-be-atomic".to_string(),
                ]
            );
        }

        #[test]
        fn test_nested_config_ignored_and_workspace_state_base_ignores_diff_policy_injection() {
            let ws = monorepo_workspace()
                .with_file(".canons/security/config.yaml")
                .with_policy_config(".canons/config.yaml", "rules: []\n");

            // Untrusted PR diff attempts to inject `- deny: all` into `.canons/config.yaml` under `--workspace-state base`
            let mut diffs = BTreeMap::new();
            diffs.insert(
                ".canons/config.yaml".to_string(),
                FileArtifact {
                    path: ".canons/config.yaml".to_string(),
                    status: FileChangeStatus::Modified,
                    previous_path: None,
                    lines_added: 2,
                    lines_deleted: 1,
                    patch: Some("@@ -1 +1,2 @@\n-rules: []\n+rules:\n+  - deny: all\n".to_string()),
                    patch_omission_reason: None,
                    content: Some("rules:\n  - deny: all\n".to_string()),
                    content_omission_reason: None,
                },
            );
            let intake = CaseloadIntake {
                workspace_state: WorkspaceState::Base,
                target_paths: vec!["src/lib.rs".to_string()],
                diffs,
                ..CaseloadIntake::default()
            };

            let disc = execute_discover(
                &DiscoverOptions {
                    include_builtins: false,
                    ..DiscoverOptions::default()
                },
                Caseload::new().with_intake(intake),
                &ws,
            )
            .expect("discover base state")
            .discovery
            .expect("discovery");

            // Nested config is NOT in config_files; untrusted diff injection did NOT deny root canons
            assert_eq!(disc.config_files, vec![".canons/config.yaml".to_string()]);
            assert_eq!(
                disc.candidate_canons,
                vec![
                    ".canons/architecture/foo.canon.md".to_string(),
                    ".canons/style/bar.canon.md".to_string(),
                ]
            );
        }
    }
}
