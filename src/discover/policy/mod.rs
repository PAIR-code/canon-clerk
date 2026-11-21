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

//! Declarative workspace policy configuration (`<scope>/.canons/config.yaml`)
//! classification and nearest-scope-first lexical cascade evaluator (`SPEC.md` §7).

mod parse;

use super::DiscoverError;
use super::glob_matcher::{GlobPattern, normalize_workspace_path, scope_relative_path};
use crate::canon::{Canon, CanonScope, CanonSourceKind, resolve_source_info};
use parse::parse_policy_config;
use std::collections::{BTreeMap, BTreeSet};

/// Canonical filename for workspace policy configuration files (`SPEC.md` §7.1).
const POLICY_CONFIG_FILENAME: &str = "config.yaml";

/// Classification of a workspace-relative path with respect to `.canons/config.yaml` discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PolicyConfigPathKind {
    /// Direct child `<scope>/.canons/config.yaml` (with `scope: None` for root `.canons/config.yaml`).
    Direct {
        /// Enclosing monorepo scope prefix, or `None` for the repository root.
        scope: Option<String>,
    },
    /// Nested `config.yaml` inside a `.canons/` subdirectory (e.g., `.canons/security/config.yaml`),
    /// which must be ignored with a non-fatal diagnostic warning (`SPEC.md` §7.1).
    Nested,
    /// Unrelated file (including alternate filenames such as `config.yml`).
    Other,
}

/// Classifies a workspace-relative POSIX path as a direct policy config, an ignored nested config,
/// or an unrelated file (`SPEC.md` §7.1).
pub(super) fn classify_policy_config_path(rel_path: &str) -> PolicyConfigPathKind {
    let Ok(normalized) = normalize_workspace_path(rel_path) else {
        return PolicyConfigPathKind::Other;
    };
    let segments: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
    if segments.last().copied() != Some(POLICY_CONFIG_FILENAME) {
        return PolicyConfigPathKind::Other;
    }

    let Some(first_canons_idx) = segments.iter().position(|&seg| seg == ".canons") else {
        return PolicyConfigPathKind::Other;
    };

    if first_canons_idx + 2 == segments.len() {
        let scope = if first_canons_idx == 0 {
            None
        } else {
            Some(segments[..first_canons_idx].join("/"))
        };
        PolicyConfigPathKind::Direct { scope }
    } else {
        PolicyConfigPathKind::Nested
    }
}

/// Returns all ignored nested `config.yaml` paths inside `.canons/` subdirectories in sorted order.
pub(super) fn detect_nested_policy_configs(existing_files: &BTreeSet<String>) -> Vec<String> {
    existing_files
        .iter()
        .filter(|path| {
            matches!(
                classify_policy_config_path(path),
                PolicyConfigPathKind::Nested
            )
        })
        .cloned()
        .collect()
}

/// Effective action of a workspace policy rule (`SPEC.md` §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PolicyAction {
    /// Explicitly permit the matching canon/exhibit pair.
    Allow,
    /// Explicitly exclude the matching canon/exhibit pair.
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PolicyCanonSelector {
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
struct CompiledPolicyRule {
    action: PolicyAction,
    selector: PolicyCanonSelector,
    exhibits: Option<Vec<GlobPattern>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompiledPolicyConfig {
    rules: Vec<CompiledPolicyRule>,
}

/// Compiled set of all discovered `<scope>/.canons/config.yaml` policy files in a workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct WorkspacePolicySet {
    config_files: Vec<String>,
    configs_by_scope: BTreeMap<Option<String>, CompiledPolicyConfig>,
}

impl WorkspacePolicySet {
    /// Parses, validates, and compiles all discovered `<scope>/.canons/config.yaml` files.
    ///
    /// Fails fast with [`DiscoverError::InvalidPolicyConfig`] on any schema, YAML, glob,
    /// traversal, or credential-isolation violation (`SPEC.md` §7.2).
    pub(super) fn compile(raw_configs: &BTreeMap<String, String>) -> Result<Self, DiscoverError> {
        let mut config_files = Vec::new();
        let mut configs_by_scope = BTreeMap::new();

        for (path, raw_yaml) in raw_configs {
            let PolicyConfigPathKind::Direct { scope } = classify_policy_config_path(path) else {
                continue;
            };
            let compiled = parse_policy_config(path, raw_yaml)?;
            config_files.push(path.clone());
            configs_by_scope.insert(scope, compiled);
        }

        config_files.sort();
        Ok(Self {
            config_files,
            configs_by_scope,
        })
    }

    /// Returns stably sorted workspace-relative paths of all compiled `<scope>/.canons/config.yaml` files.
    pub(super) fn config_files(&self) -> &[String] {
        &self.config_files
    }

    /// Evaluates whether `(canon, file_path)` is allowed under the nearest-scope-first lexical cascade
    /// (`[child <scope>/.canons/config.yaml -> parent ... -> root .canons/config.yaml -> implicit allow]`).
    pub(super) fn allows_file_exhibit(
        &self,
        canon_ref: &str,
        canon: &Canon,
        file_path: &str,
    ) -> bool {
        if self.configs_by_scope.is_empty() {
            return true;
        }
        let scopes = file_scope_chain(file_path);
        for scope in scopes {
            let Some(config) = self.configs_by_scope.get(&scope) else {
                continue;
            };
            let canon_scope_obj = scope.as_deref().map(|s| CanonScope(s.to_string()));
            let Some(rel_file) = scope_relative_path(file_path, canon_scope_obj.as_ref()) else {
                continue;
            };
            for rule in &config.rules {
                if !rule.selector.matches(canon_ref, canon, scope.as_deref()) {
                    continue;
                }
                let exhibits_match = match &rule.exhibits {
                    None => true,
                    Some(globs) => globs.iter().any(|g| g.is_match(&rel_file)),
                };
                if exhibits_match {
                    return rule.action == PolicyAction::Allow;
                }
            }
        }
        true
    }

    /// Evaluates unconditional policy rules (omitted `exhibits:`) anchored at the canon's own scope
    /// (`[<canon-scope>/.canons/config.yaml -> ancestor ... -> root .canons/config.yaml -> implicit allow]`),
    /// used for metadata-only filings and `--all-canons` (`SPEC.md` §7.3).
    pub(super) fn allows_unconditional(&self, canon_ref: &str, canon: &Canon) -> bool {
        if self.configs_by_scope.is_empty() {
            return true;
        }
        let anchor = canon_scope(canon_ref, canon);
        let scopes = scope_ancestor_chain(anchor.as_deref());
        for scope in scopes {
            let Some(config) = self.configs_by_scope.get(&scope) else {
                continue;
            };
            for rule in &config.rules {
                if rule.exhibits.is_some() {
                    continue;
                }
                if rule.selector.matches(canon_ref, canon, scope.as_deref()) {
                    return rule.action == PolicyAction::Allow;
                }
            }
        }
        true
    }
}

fn is_builtin_canon(canon_ref: &str, canon: &Canon) -> bool {
    canon.source_kind == Some(CanonSourceKind::Builtin) || canon_ref.starts_with("builtin:")
}

fn is_workspace_canon(canon_ref: &str, canon: &Canon) -> bool {
    match canon.source_kind {
        Some(CanonSourceKind::Workspace) => true,
        Some(CanonSourceKind::Builtin | CanonSourceKind::Pack(_)) => false,
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

fn file_scope_chain(file_path: &str) -> Vec<Option<String>> {
    let clean = file_path.trim().replace('\\', "/");
    let trimmed = clean.trim_start_matches("./").trim_matches('/');
    let parent = trimmed.rsplit_once('/').map(|(dir, _)| dir);
    scope_ancestor_chain(parent)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_policy_config_paths() {
        assert_eq!(
            classify_policy_config_path(".canons/config.yaml"),
            PolicyConfigPathKind::Direct { scope: None }
        );
        assert_eq!(
            classify_policy_config_path("packages/ui/.canons/config.yaml"),
            PolicyConfigPathKind::Direct {
                scope: Some("packages/ui".to_string())
            }
        );
        assert_eq!(
            classify_policy_config_path(".canons/security/config.yaml"),
            PolicyConfigPathKind::Nested
        );
        assert_eq!(
            classify_policy_config_path("packages/ui/.canons/sub/config.yaml"),
            PolicyConfigPathKind::Nested
        );
        assert_eq!(
            classify_policy_config_path(".canons/config.yml"),
            PolicyConfigPathKind::Other
        );
        assert_eq!(
            classify_policy_config_path("config.yaml"),
            PolicyConfigPathKind::Other
        );
    }

    #[test]
    fn test_empty_and_empty_rules_are_valid_noops() {
        for raw in ["", "   \n# comment\n", "rules: []", "rules:"] {
            let mut configs = BTreeMap::new();
            configs.insert(".canons/config.yaml".to_string(), raw.to_string());
            let set = WorkspacePolicySet::compile(&configs).expect("valid noop");
            assert_eq!(set.config_files(), &[".canons/config.yaml".to_string()]);
        }
    }

    #[test]
    fn test_schema_validation_rejects_malformed_unknown_credential_and_traversal() {
        let invalid_cases = [
            ("malformed yaml", "rules:\n  - deny: [unclosed"),
            ("non-mapping root", "- deny: all"),
            ("unknown top-level key", "unknown_key: true"),
            ("credential key providers", "providers:\n  gemini: {}"),
            ("credential key apiKey", "apiKey: secret"),
            ("credential key defaults", "defaults:\n  model: x"),
            ("credential key nodes", "nodes:\n  audit: {}"),
            ("deferred key validate", "validate:\n  rules: {}"),
            ("deferred key packs", "packs:\n  foo: bar"),
            ("missing allow/deny", "rules:\n  - exhibits: 'src/**'"),
            (
                "conflicting allow and deny",
                "rules:\n  - allow: all\n    deny: all",
            ),
            ("unknown rule key", "rules:\n  - deny: all\n    extra: 1"),
            ("empty selector", "rules:\n  - deny: ''"),
            ("empty builtin selector", "rules:\n  - deny: 'builtin:'"),
            ("empty parents selector", "rules:\n  - deny: 'parents:'"),
            ("deferred tag selector", "rules:\n  - deny: 'tag:security'"),
            (
                "deferred inspect selector",
                "rules:\n  - deny: 'inspect:pr_title'",
            ),
            ("traversal in selector", "rules:\n  - deny: '../foo'"),
            (
                "traversal in parents selector",
                "rules:\n  - deny: 'parents:../foo'",
            ),
            ("invalid glob in selector", "rules:\n  - deny: 'foo/[bad'"),
            (
                "empty exhibit glob",
                "rules:\n  - deny: all\n    exhibits: ''",
            ),
            (
                "traversal in exhibits",
                "rules:\n  - deny: all\n    exhibits: '../outside/**'",
            ),
            (
                "invalid glob in exhibits",
                "rules:\n  - deny: all\n    exhibits: 'src/{a,b'",
            ),
            (
                "deferred fielded exhibit",
                "rules:\n  - deny: all\n    exhibits: 'plane=commit_messages'",
            ),
        ];

        for (label, raw) in invalid_cases {
            let mut configs = BTreeMap::new();
            configs.insert(".canons/config.yaml".to_string(), raw.to_string());
            let err = WorkspacePolicySet::compile(&configs)
                .expect_err(&format!("case `{label}` must fail"));
            assert!(
                matches!(err, DiscoverError::InvalidPolicyConfig { .. }),
                "case `{label}` produced unexpected error: {err:?}"
            );
        }
    }
}
