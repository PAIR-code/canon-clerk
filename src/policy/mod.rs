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

//! Cross-cutting declarative workspace policy chains (`<scope>/.canons/policy-chains.yaml`)
//! discovery, fail-fast schema validation, and nearest-scope-first lexical cascade primitives
//! (`SPEC.md` §7, `docs/architecture/policy.md`).

mod parse;
mod survey;

use survey::CompiledSurveyContextRule;
pub use survey::DEFAULT_SURVEY_CONTEXT_PATHS;

use crate::canon::{Canon, CanonScope, CanonSourceKind, resolve_source_info};
use crate::discover::glob_matcher::{
    GlobPattern, is_ignored_path, normalize_workspace_path, scope_relative_path,
};
use parse::parse_policy_file;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

/// Canonical filename for declarative workspace policy chain files (`SPEC.md` §7.1).
pub const POLICY_CHAINS_FILENAME: &str = "policy-chains.yaml";

/// Error returned while discovering, parsing, or validating `<scope>/.canons/policy-chains.yaml` files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyError {
    /// Malformed or schema-invalid `<scope>/.canons/policy-chains.yaml` policy file (`SPEC.md` §7.2).
    InvalidPolicyFile {
        /// Workspace-relative POSIX path of the offending policy file.
        path: String,
        /// Actionable human-readable validation failure reason.
        message: String,
    },
    /// Filesystem I/O error encountered while traversing or reading trusted policy files on disk.
    Io {
        /// Workspace-relative path or workspace root where I/O failed.
        path: String,
        /// Underlying OS error description.
        message: String,
    },
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPolicyFile { path, message } => {
                write!(f, "Invalid workspace policy file `{path}`: {message}")
            }
            Self::Io { path, message } => {
                write!(
                    f,
                    "Failed to read workspace policy path `{path}`: {message}"
                )
            }
        }
    }
}

impl std::error::Error for PolicyError {}

/// Classification of a workspace-relative path with respect to `.canons/policy-chains.yaml` discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyFilePathKind {
    /// Direct child `<scope>/.canons/policy-chains.yaml` (with `scope: None` for root `.canons/policy-chains.yaml`).
    Direct {
        /// Enclosing monorepo scope prefix, or `None` for the repository root.
        scope: Option<String>,
    },
    /// Nested `policy-chains.yaml` inside a `.canons/` subdirectory (e.g., `.canons/security/policy-chains.yaml`),
    /// which must be ignored with a non-fatal diagnostic warning (`SPEC.md` §7.1).
    Nested,
    /// Unrelated file (including alternate filenames such as `.canons/config.yaml`, `config.yml`, or `policy-chains.yml`).
    Other,
}

/// Classifies a workspace-relative POSIX path as a direct policy file, an ignored nested policy file,
/// or an unrelated file (`SPEC.md` §7.1).
pub fn classify_policy_file_path(rel_path: &str) -> PolicyFilePathKind {
    let Ok(normalized) = normalize_workspace_path(rel_path) else {
        return PolicyFilePathKind::Other;
    };
    let segments: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
    if segments.last().copied() != Some(POLICY_CHAINS_FILENAME) {
        return PolicyFilePathKind::Other;
    }

    let Some(first_canons_idx) = segments.iter().position(|&seg| seg == ".canons") else {
        return PolicyFilePathKind::Other;
    };

    if first_canons_idx + 2 == segments.len() {
        let scope = if first_canons_idx == 0 {
            None
        } else {
            Some(segments[..first_canons_idx].join("/"))
        };
        PolicyFilePathKind::Direct { scope }
    } else {
        PolicyFilePathKind::Nested
    }
}

/// Returns `true` if `rel_path` is a canonical direct-child `<scope>/.canons/policy-chains.yaml` path.
pub fn is_direct_policy_file_path(rel_path: &str) -> bool {
    matches!(
        classify_policy_file_path(rel_path),
        PolicyFilePathKind::Direct { .. }
    )
}

/// Returns all ignored nested `policy-chains.yaml` paths inside `.canons/` subdirectories in sorted order.
pub fn detect_nested_policy_files(existing_files: &BTreeSet<String>) -> Vec<String> {
    existing_files
        .iter()
        .filter(|path| matches!(classify_policy_file_path(path), PolicyFilePathKind::Nested))
        .cloned()
        .collect()
}

/// Raw policy files and ignored nested paths discovered from a trusted workspace checkout on disk.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiscoveredPolicyFiles {
    /// Raw UTF-8 YAML contents of direct `<scope>/.canons/policy-chains.yaml` files keyed by workspace-relative POSIX path.
    pub files: BTreeMap<String, String>,
    /// Stably sorted workspace-relative paths of ignored nested `policy-chains.yaml` files inside `.canons/` subdirectories.
    pub nested_paths: Vec<String>,
}

/// Discovers and reads all direct `<scope>/.canons/policy-chains.yaml` files strictly within `workspace_root`
/// on disk (trusted-checkout boundary; never ascends above `workspace_root`).
///
/// # Errors
///
/// Returns [`PolicyError::Io`] if `workspace_root` cannot be read or if reading any discovered
/// direct `<scope>/.canons/policy-chains.yaml` file as UTF-8 fails.
pub fn discover_policy_files_on_disk(
    workspace_root: &Path,
) -> Result<DiscoveredPolicyFiles, PolicyError> {
    let mut discovered = DiscoveredPolicyFiles::default();
    walk_policy_files(workspace_root, workspace_root, &mut discovered)?;
    discovered.nested_paths.sort();
    Ok(discovered)
}

fn walk_policy_files(
    root: &Path,
    current_dir: &Path,
    out: &mut DiscoveredPolicyFiles,
) -> Result<(), PolicyError> {
    let entries = match fs::read_dir(current_dir) {
        Ok(iter) => iter,
        Err(err) => {
            if current_dir == root {
                return Err(PolicyError::Io {
                    path: current_dir.display().to_string(),
                    message: err.to_string(),
                });
            }
            return Ok(());
        }
    };

    let mut sorted_entries: Vec<_> = entries.filter_map(Result::ok).collect();
    sorted_entries.sort_by_key(|e| e.file_name());

    for entry in sorted_entries {
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let rel_posix = rel.to_string_lossy().replace('\\', "/");
        if rel_posix.is_empty() || is_ignored_path(&rel_posix) {
            continue;
        }

        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            walk_policy_files(root, &path, out)?;
        } else if metadata.is_file() {
            match classify_policy_file_path(&rel_posix) {
                PolicyFilePathKind::Direct { .. } => {
                    let raw_yaml = fs::read_to_string(&path).map_err(|err| PolicyError::Io {
                        path: rel_posix.clone(),
                        message: err.to_string(),
                    })?;
                    out.files.insert(rel_posix, raw_yaml);
                }
                PolicyFilePathKind::Nested => {
                    out.nested_paths.push(rel_posix);
                }
                PolicyFilePathKind::Other => {}
            }
        }
    }

    Ok(())
}

/// Effective action of an ordered workspace policy chain rule (`SPEC.md` §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyAction {
    /// Explicitly permit the matching item.
    Allow,
    /// Explicitly exclude the matching item.
    Deny,
}

/// Evaluates a nearest-scope-first lexical cascade across `scopes`, stopping at the first
/// scope whose evaluator returns `Some(PolicyAction)`.
fn evaluate_scope_cascade<C, F>(
    configs_by_scope: &BTreeMap<Option<String>, C>,
    scopes: &[Option<String>],
    mut evaluate_scope: F,
) -> Option<PolicyAction>
where
    F: FnMut(Option<&str>, &C) -> Option<PolicyAction>,
{
    for scope in scopes {
        let Some(config) = configs_by_scope.get(scope) else {
            continue;
        };
        if let Some(action) = evaluate_scope(scope.as_deref(), config) {
            return Some(action);
        }
    }
    None
}

/// Returns the ordered nearest-scope-first ancestor scope chain for a workspace-relative file path:
/// `[deepest_dir, parent_dir, ..., None (repo root)]`.
fn file_scope_chain(file_path: &str) -> Vec<Option<String>> {
    let clean = file_path.trim().replace('\\', "/");
    let trimmed = clean.trim_start_matches("./").trim_matches('/');
    let parent = trimmed.rsplit_once('/').map(|(dir, _)| dir);
    scope_ancestor_chain(parent)
}

/// Returns the ordered nearest-scope-first ancestor scope chain starting at `start_scope`
/// and terminating at `None` (repository root).
fn scope_ancestor_chain(start_scope: Option<&str>) -> Vec<Option<String>> {
    let mut chain = Vec::new();
    let mut current = start_scope
        .map(|s| s.trim_matches('/'))
        .filter(|s| !s.is_empty() && *s != ".");
    while let Some(dir) = current {
        chain.push(Some(dir.to_string()));
        current = dir.rsplit_once('/').map(|(parent, _)| parent);
    }
    chain.push(None);
    chain
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PolicyCanonSelector {
    All,
    Builtin(GlobPattern),
    Parents(GlobPattern),
    Local(GlobPattern),
}

impl PolicyCanonSelector {
    fn matches(&self, canon_ref: &str, canon: &Canon, config_scope: Option<&str>) -> bool {
        match self {
            Self::All => true,
            Self::Builtin(glob) => {
                if !is_builtin_canon(canon_ref, canon) {
                    return false;
                }
                canon_diagnostic_path(canon_ref, canon).is_some_and(|path| glob.is_match(&path))
            }
            Self::Local(glob) => {
                if !is_workspace_canon(canon_ref, canon) {
                    return false;
                }
                if canon_scope(canon_ref, canon).as_deref() != config_scope {
                    return false;
                }
                canon_diagnostic_path(canon_ref, canon).is_some_and(|path| glob.is_match(&path))
            }
            Self::Parents(glob) => {
                let Some(child_scope) = config_scope else {
                    return false;
                };
                if !is_workspace_canon(canon_ref, canon) {
                    return false;
                }
                let c_scope = canon_scope(canon_ref, canon);
                if !is_strict_ancestor_scope(c_scope.as_deref(), child_scope) {
                    return false;
                }
                canon_diagnostic_path(canon_ref, canon).is_some_and(|path| glob.is_match(&path))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledCanonPolicyRule {
    pub(crate) action: PolicyAction,
    pub(crate) selector: PolicyCanonSelector,
    pub(crate) exhibits: Option<Vec<GlobPattern>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CompiledPolicyFile {
    pub(crate) canon_policy_chain: Vec<CompiledCanonPolicyRule>,
    pub(crate) survey_context_chain: Vec<CompiledSurveyContextRule>,
}

/// Compiled set of all discovered `<scope>/.canons/policy-chains.yaml` policy files in a workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspacePolicySet {
    policy_files: Vec<String>,
    files_by_scope: BTreeMap<Option<String>, CompiledPolicyFile>,
}

impl WorkspacePolicySet {
    /// Parses, validates, and compiles all discovered `<scope>/.canons/policy-chains.yaml` files.
    ///
    /// Fails fast with [`PolicyError::InvalidPolicyFile`] on any schema, YAML, glob,
    /// traversal, or runtime-isolation violation (`SPEC.md` §7.2).
    pub fn compile(raw_files: &BTreeMap<String, String>) -> Result<Self, PolicyError> {
        let mut policy_files = Vec::new();
        let mut files_by_scope = BTreeMap::new();

        for (path, raw_yaml) in raw_files {
            let PolicyFilePathKind::Direct { scope } = classify_policy_file_path(path) else {
                continue;
            };
            let compiled = parse_policy_file(path, raw_yaml)?;
            policy_files.push(path.clone());
            files_by_scope.insert(scope, compiled);
        }

        policy_files.sort();
        Ok(Self {
            policy_files,
            files_by_scope,
        })
    }

    /// Returns stably sorted workspace-relative paths of all compiled `<scope>/.canons/policy-chains.yaml` files.
    pub fn policy_files(&self) -> &[String] {
        &self.policy_files
    }

    /// Evaluates whether `(canon, file_path)` is allowed under the nearest-scope-first lexical cascade
    /// (`[child <scope>/.canons/policy-chains.yaml -> parent ... -> root .canons/policy-chains.yaml -> implicit allow]`).
    pub fn allows_file_exhibit(&self, canon_ref: &str, canon: &Canon, file_path: &str) -> bool {
        if is_inline_canon(canon_ref, canon) || self.files_by_scope.is_empty() {
            return true;
        }
        let scopes = file_scope_chain(file_path);
        evaluate_scope_cascade(&self.files_by_scope, &scopes, |scope, compiled| {
            let canon_scope_obj = scope.map(|s| CanonScope(s.to_string()));
            let rel_file = scope_relative_path(file_path, canon_scope_obj.as_ref())?;
            for rule in &compiled.canon_policy_chain {
                if !rule.selector.matches(canon_ref, canon, scope) {
                    continue;
                }
                let exhibits_match = match &rule.exhibits {
                    None => true,
                    Some(globs) => globs.iter().any(|g| g.is_match(&rel_file)),
                };
                if exhibits_match {
                    return Some(rule.action);
                }
            }
            None
        })
        .is_none_or(|action| action == PolicyAction::Allow)
    }

    /// Evaluates unconditional `canon-policy-chain:` rules (omitted `exhibits:`) anchored at the canon's own scope
    /// (`[<canon-scope>/.canons/policy-chains.yaml -> ancestor ... -> root .canons/policy-chains.yaml -> implicit allow]`),
    /// used for metadata-only filings and `--all-canons` (`SPEC.md` §7.3).
    pub fn allows_unconditional(&self, canon_ref: &str, canon: &Canon) -> bool {
        if is_inline_canon(canon_ref, canon) || self.files_by_scope.is_empty() {
            return true;
        }
        let anchor = canon_scope(canon_ref, canon);
        let scopes = scope_ancestor_chain(anchor.as_deref());
        evaluate_scope_cascade(&self.files_by_scope, &scopes, |scope, compiled| {
            for rule in &compiled.canon_policy_chain {
                if rule.exhibits.is_some() {
                    continue;
                }
                if rule.selector.matches(canon_ref, canon, scope) {
                    return Some(rule.action);
                }
            }
            None
        })
        .is_none_or(|action| action == PolicyAction::Allow)
    }
    /// Evaluates whether a workspace-relative `file_path` is included by `survey-context-chain:`
    /// under the nearest-scope-first lexical cascade:
    /// `[child <scope>/.canons/policy-chains.yaml -> parent ... -> root .canons/policy-chains.yaml -> built-in root default allowlist -> implicit deny]`
    /// (`SPEC.md` §7.3).
    pub fn allows_survey_file(&self, file_path: impl AsRef<Path>) -> bool {
        let raw_path = file_path.as_ref().to_string_lossy();
        let Ok(normalized) = normalize_workspace_path(&raw_path) else {
            return false;
        };
        if normalized == "." {
            return false;
        }
        let scopes = file_scope_chain(&normalized);
        match evaluate_scope_cascade(&self.files_by_scope, &scopes, |scope, compiled| {
            let canon_scope_obj = scope.map(|s| CanonScope(s.to_string()));
            let rel_file = scope_relative_path(&normalized, canon_scope_obj.as_ref())?;
            for rule in &compiled.survey_context_chain {
                if rule.selector.matches(&rel_file) {
                    return Some(rule.action);
                }
            }
            None
        }) {
            Some(PolicyAction::Allow) => true,
            Some(PolicyAction::Deny) => false,
            None => DEFAULT_SURVEY_CONTEXT_PATHS.contains(&normalized.as_str()),
        }
    }
}

fn is_builtin_canon(canon_ref: &str, canon: &Canon) -> bool {
    canon.source_kind == Some(CanonSourceKind::Builtin) || canon_ref.starts_with("builtin:")
}

fn is_inline_canon(canon_ref: &str, canon: &Canon) -> bool {
    canon.source_kind == Some(CanonSourceKind::Inline) || canon_ref.starts_with("inline:")
}

fn is_workspace_canon(canon_ref: &str, canon: &Canon) -> bool {
    match canon.source_kind {
        Some(CanonSourceKind::Workspace) => true,
        Some(CanonSourceKind::Builtin | CanonSourceKind::Inline | CanonSourceKind::Pack(_)) => {
            false
        }
        None => resolve_source_info(canon_ref)
            .is_some_and(|info| info.source_kind == CanonSourceKind::Workspace),
    }
}

fn canon_scope(canon_ref: &str, canon: &Canon) -> Option<String> {
    if canon.scope.is_some() {
        return canon.scope.clone();
    }
    resolve_source_info(canon_ref).and_then(|info| info.scope.map(|s| s.0))
}

fn canon_diagnostic_path(canon_ref: &str, canon: &Canon) -> Option<String> {
    if let Some(diag) = canon.diagnostic_path.as_deref()
        && !diag.is_empty()
    {
        return Some(diag.to_string());
    }
    resolve_source_info(canon_ref)
        .map(|info| info.diagnostic_path)
        .filter(|p| !p.is_empty())
}

fn is_strict_ancestor_scope(ancestor: Option<&str>, child: &str) -> bool {
    match ancestor {
        None => true,
        Some(anc) => {
            let clean_anc = anc.trim_matches('/');
            let clean_child = child.trim_matches('/');
            clean_child.starts_with(clean_anc)
                && clean_child.as_bytes().get(clean_anc.len()) == Some(&b'/')
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_classify_policy_file_paths() {
        assert_eq!(
            classify_policy_file_path(".canons/policy-chains.yaml"),
            PolicyFilePathKind::Direct { scope: None }
        );
        assert_eq!(
            classify_policy_file_path("packages/ui/.canons/policy-chains.yaml"),
            PolicyFilePathKind::Direct {
                scope: Some("packages/ui".to_string())
            }
        );
        assert_eq!(
            classify_policy_file_path(".canons/security/policy-chains.yaml"),
            PolicyFilePathKind::Nested
        );
        assert_eq!(
            classify_policy_file_path("packages/ui/.canons/sub/policy-chains.yaml"),
            PolicyFilePathKind::Nested
        );
        // Alternate filenames must not be loaded
        assert_eq!(
            classify_policy_file_path(".canons/config.yaml"),
            PolicyFilePathKind::Other
        );
        assert_eq!(
            classify_policy_file_path(".canons/config.yml"),
            PolicyFilePathKind::Other
        );
        assert_eq!(
            classify_policy_file_path(".canons/policy-chains.yml"),
            PolicyFilePathKind::Other
        );
        assert_eq!(
            classify_policy_file_path(".canon-clerk.yaml"),
            PolicyFilePathKind::Other
        );
        assert_eq!(
            classify_policy_file_path("policy-chains.yaml"),
            PolicyFilePathKind::Other
        );
    }

    #[test]
    fn test_empty_and_empty_rules_are_valid_noops() {
        for raw in [
            "",
            "   \n# comment\n",
            "canon-policy-chain: []",
            "canon-policy-chain:",
        ] {
            let mut files = BTreeMap::new();
            files.insert(".canons/policy-chains.yaml".to_string(), raw.to_string());
            let set = WorkspacePolicySet::compile(&files).expect("valid noop");
            assert_eq!(
                set.policy_files(),
                &[".canons/policy-chains.yaml".to_string()]
            );
        }
    }

    #[test]
    fn test_schema_validation_rejects_malformed_unknown_credential_budget_and_traversal() {
        let invalid_cases = [
            ("malformed yaml", "canon-policy-chain:\n  - deny: [unclosed"),
            ("non-mapping root", "- deny: all"),
            ("unknown top-level key", "unknown_key: true"),
            ("legacy rules key rejected", "rules:\n  - deny: all"),
            ("credential key providers", "providers:\n  gemini: {}"),
            ("credential key apiKey", "apiKey: secret"),
            ("credential key defaults", "defaults:\n  model: x"),
            ("credential key nodes", "nodes:\n  audit: {}"),
            ("budget key reasoning_budget", "reasoning_budget: 1024"),
            ("budget key max_file_chars", "max_file_chars: 4000"),
            ("budget key max_total_chars", "max_total_chars: 12000"),
            ("budget key diff_head_lines", "diff_head_lines: 80"),
            ("deferred key validate", "validate:\n  rules: {}"),
            ("deferred key packs", "packs:\n  foo: bar"),
            (
                "missing allow/deny",
                "canon-policy-chain:\n  - exhibits: 'src/**'",
            ),
            (
                "conflicting allow and deny",
                "canon-policy-chain:\n  - allow: all\n    deny: all",
            ),
            (
                "unknown rule key",
                "canon-policy-chain:\n  - deny: all\n    extra: 1",
            ),
            ("empty selector", "canon-policy-chain:\n  - deny: ''"),
            (
                "empty builtin selector",
                "canon-policy-chain:\n  - deny: 'builtin:'",
            ),
            (
                "empty parents selector",
                "canon-policy-chain:\n  - deny: 'parents:'",
            ),
            (
                "deferred tag selector",
                "canon-policy-chain:\n  - deny: 'tag:security'",
            ),
            (
                "deferred inspect selector",
                "canon-policy-chain:\n  - deny: 'inspect:pr_title'",
            ),
            (
                "traversal in selector",
                "canon-policy-chain:\n  - deny: '../foo'",
            ),
            (
                "traversal in parents selector",
                "canon-policy-chain:\n  - deny: 'parents:../foo'",
            ),
            (
                "invalid glob in selector",
                "canon-policy-chain:\n  - deny: 'foo/[bad'",
            ),
            (
                "empty exhibit glob",
                "canon-policy-chain:\n  - deny: all\n    exhibits: ''",
            ),
            (
                "traversal in exhibits",
                "canon-policy-chain:\n  - deny: all\n    exhibits: '../outside/**'",
            ),
            (
                "invalid glob in exhibits",
                "canon-policy-chain:\n  - deny: all\n    exhibits: 'src/{a,b'",
            ),
            (
                "deferred fielded exhibit",
                "canon-policy-chain:\n  - deny: all\n    exhibits: 'plane=commit_messages'",
            ),
        ];

        for (label, raw) in invalid_cases {
            let mut files = BTreeMap::new();
            files.insert(".canons/policy-chains.yaml".to_string(), raw.to_string());
            let err = WorkspacePolicySet::compile(&files)
                .expect_err(&format!("case `{label}` must fail"));
            assert!(
                matches!(err, PolicyError::InvalidPolicyFile { .. }),
                "case `{label}` produced unexpected error: {err:?}"
            );
        }
    }

    #[test]
    fn test_discover_policy_files_on_disk_loads_direct_and_records_nested() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("canon_clerk_policy_disk_{nanos}"));
        fs::create_dir_all(root.join(".canons/security")).unwrap();
        fs::create_dir_all(root.join("packages/ui/.canons")).unwrap();
        fs::write(
            root.join(".canons/policy-chains.yaml"),
            "canon-policy-chain: []\n",
        )
        .unwrap();
        fs::write(
            root.join(".canons/config.yaml"),
            "canon-policy-chain:\n  - deny: all\n",
        )
        .unwrap();
        fs::write(
            root.join(".canons/security/policy-chains.yaml"),
            "canon-policy-chain:\n  - deny: all\n",
        )
        .unwrap();
        fs::write(
            root.join("packages/ui/.canons/policy-chains.yaml"),
            "canon-policy-chain:\n  - allow: all\n",
        )
        .unwrap();

        let discovered = discover_policy_files_on_disk(&root).expect("discover on disk");
        let _ = fs::remove_dir_all(&root);

        let keys: Vec<String> = discovered.files.keys().cloned().collect();
        assert_eq!(
            keys,
            vec![
                ".canons/policy-chains.yaml".to_string(),
                "packages/ui/.canons/policy-chains.yaml".to_string(),
            ]
        );
        assert_eq!(
            discovered.nested_paths,
            vec![".canons/security/policy-chains.yaml".to_string()]
        );
    }

    #[test]
    fn test_survey_context_chain_defaults_and_nested_cascading() {
        let empty = WorkspacePolicySet::default();
        assert!(empty.allows_survey_file("README.md"));
        assert!(empty.allows_survey_file("Cargo.toml"));
        assert!(empty.allows_survey_file("llms.txt"));
        assert!(!empty.allows_survey_file("src/lib.rs"));
        assert!(!empty.allows_survey_file("packages/ui/README.md"));

        let mut raw = BTreeMap::new();
        raw.insert(
            ".canons/policy-chains.yaml".to_string(),
            r#"
survey-context-chain:
  - deny: "Cargo.toml"
  - allow: "docs/architecture.md"
  - allow: "packages/*/README.md"
"#
            .to_string(),
        );
        raw.insert(
            "packages/vendored/.canons/policy-chains.yaml".to_string(),
            r#"
survey-context-chain:
  - deny: all
"#
            .to_string(),
        );
        raw.insert(
            "packages/ui/.canons/policy-chains.yaml".to_string(),
            r#"
survey-context-chain:
  - allow: "DESIGN.md"
"#
            .to_string(),
        );

        let policy = WorkspacePolicySet::compile(&raw).expect("compile survey policy");
        // Root defaults fall through when not matched
        assert!(policy.allows_survey_file("README.md"));
        // Root deny overrides built-in default
        assert!(!policy.allows_survey_file("Cargo.toml"));
        // Root allow adds custom paths
        assert!(policy.allows_survey_file("docs/architecture.md"));
        assert!(policy.allows_survey_file("packages/ui/README.md"));
        // Child allow adds scope-relative path and falls through to root for others
        assert!(policy.allows_survey_file("packages/ui/DESIGN.md"));
        // Child `- deny: all` isolates subtree from parent allow
        assert!(!policy.allows_survey_file("packages/vendored/README.md"));
    }

    #[test]
    fn test_survey_context_chain_validation_rejects_invalid_rules() {
        let invalid_cases = [
            ("non-sequence", "survey-context-chain: true"),
            ("missing action", "survey-context-chain:\n  - {}"),
            (
                "conflicting allow and deny",
                "survey-context-chain:\n  - allow: all\n    deny: all",
            ),
            (
                "disallowed exhibits key",
                "survey-context-chain:\n  - allow: 'README.md'\n    exhibits: 'src/**'",
            ),
            ("empty selector", "survey-context-chain:\n  - allow: ''"),
            (
                "traversal selector",
                "survey-context-chain:\n  - allow: '../README.md'",
            ),
            (
                "absolute selector",
                "survey-context-chain:\n  - allow: '/etc/passwd'",
            ),
            (
                "invalid glob",
                "survey-context-chain:\n  - allow: 'docs/[bad'",
            ),
        ];

        for (label, raw) in invalid_cases {
            let mut files = BTreeMap::new();
            files.insert(".canons/policy-chains.yaml".to_string(), raw.to_string());
            let err = WorkspacePolicySet::compile(&files)
                .expect_err(&format!("case `{label}` must fail"));
            assert!(
                matches!(err, PolicyError::InvalidPolicyFile { .. }),
                "case `{label}` produced unexpected error: {err:?}"
            );
        }
    }
}
