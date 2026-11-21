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

//! Declarative workspace policy configuration (`<scope>/.canons/config.yaml`) parser,
//! validator, and nearest-scope-first lexical cascade evaluator (`SPEC.md` §7).

use super::DiscoverError;
use super::glob_matcher::{GlobPattern, normalize_workspace_path, scope_relative_path};
use crate::canon::{CANON_FILE_EXT, Canon, CanonScope, CanonSourceKind, resolve_source_info};
use serde_yaml::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Canonical filename for workspace policy configuration files (`SPEC.md` §7.1).
const POLICY_CONFIG_FILENAME: &str = "config.yaml";

const RUNTIME_CREDENTIAL_KEYS: &[&str] = &[
    "providers",
    "defaults",
    "nodes",
    "apiKey",
    "api_key",
    "gemini_api_key",
    "typesafe_api_key",
    "baseUrl",
    "base_url",
    "model",
    "models",
    "credentials",
    "thinking_level",
    "reasoning_budget",
];

const DEFERRED_TOP_LEVEL_KEYS: &[&str] = &["validate", "packs"];

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

fn parse_policy_config(path: &str, raw_yaml: &str) -> Result<CompiledPolicyConfig, DiscoverError> {
    if raw_yaml.trim().is_empty() {
        return Ok(CompiledPolicyConfig { rules: Vec::new() });
    }

    let value: Value = serde_yaml::from_str(raw_yaml)
        .map_err(|err| invalid_config(path, format!("Malformed YAML: {err}")))?;

    if value.is_null() {
        return Ok(CompiledPolicyConfig { rules: Vec::new() });
    }

    let Some(root) = value.as_mapping() else {
        return Err(invalid_config(
            path,
            "Root YAML document must be a mapping (e.g., `rules: [...]`).",
        ));
    };

    for key in root.keys() {
        let Some(key_str) = key.as_str() else {
            return Err(invalid_config(
                path,
                "Top-level keys in `.canons/config.yaml` must be strings.",
            ));
        };
        if key_str != "rules" {
            return Err(unknown_key_error(path, key_str, "top-level"));
        }
    }

    let Some(rules_val) = root.get(Value::String("rules".to_string())) else {
        return Ok(CompiledPolicyConfig { rules: Vec::new() });
    };

    if rules_val.is_null() {
        return Ok(CompiledPolicyConfig { rules: Vec::new() });
    }

    let Some(rules_seq) = rules_val.as_sequence() else {
        return Err(invalid_config(
            path,
            "`rules` must be a YAML sequence of rule mappings.",
        ));
    };

    let mut rules = Vec::with_capacity(rules_seq.len());
    for (idx, item) in rules_seq.iter().enumerate() {
        rules.push(parse_policy_rule(path, idx, item)?);
    }

    Ok(CompiledPolicyConfig { rules })
}

fn parse_policy_rule(
    path: &str,
    idx: usize,
    item: &Value,
) -> Result<CompiledPolicyRule, DiscoverError> {
    let Some(mapping) = item.as_mapping() else {
        return Err(invalid_config(
            path,
            format!("`rules[{idx}]` must be a YAML mapping specifying `allow` or `deny`."),
        ));
    };

    for key in mapping.keys() {
        let Some(key_str) = key.as_str() else {
            return Err(invalid_config(
                path,
                format!("`rules[{idx}]` keys must be strings."),
            ));
        };
        if !matches!(key_str, "allow" | "deny" | "exhibits") {
            return Err(unknown_key_error(path, key_str, &format!("`rules[{idx}]`")));
        }
    }

    let allow_val = mapping.get(Value::String("allow".to_string()));
    let deny_val = mapping.get(Value::String("deny".to_string()));

    let (action, selector_val) = match (allow_val, deny_val) {
        (Some(v), None) => (PolicyAction::Allow, v),
        (None, Some(v)) => (PolicyAction::Deny, v),
        (Some(_), Some(_)) => {
            return Err(invalid_config(
                path,
                format!("`rules[{idx}]` cannot specify both `allow` and `deny`."),
            ));
        }
        (None, None) => {
            return Err(invalid_config(
                path,
                format!("`rules[{idx}]` must specify either `allow` or `deny`."),
            ));
        }
    };

    let Some(selector_str) = selector_val.as_str() else {
        return Err(invalid_config(
            path,
            format!("`rules[{idx}]` action selector must be a non-empty string."),
        ));
    };
    let selector = parse_canon_selector(path, idx, selector_str)?;

    let exhibits = match mapping.get(Value::String("exhibits".to_string())) {
        None => None,
        Some(ex_val) => Some(parse_exhibits(path, idx, ex_val)?),
    };

    Ok(CompiledPolicyRule {
        action,
        selector,
        exhibits,
    })
}

fn parse_canon_selector(
    path: &str,
    idx: usize,
    raw: &str,
) -> Result<PolicyCanonSelector, DiscoverError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(invalid_config(
            path,
            format!("`rules[{idx}]` canon selector must not be empty."),
        ));
    }

    if trimmed == "all" {
        return Ok(PolicyCanonSelector::All);
    }

    if let Some(rest) = trimmed.strip_prefix("builtin:") {
        let glob = parse_selector_glob(path, idx, "builtin:", rest)?;
        return Ok(PolicyCanonSelector::Builtin(glob));
    }

    if let Some(rest) = trimmed.strip_prefix("parents:") {
        let glob = parse_selector_glob(path, idx, "parents:", rest)?;
        return Ok(PolicyCanonSelector::Parents(glob));
    }

    if trimmed.starts_with("tag:") || trimmed.starts_with("inspect:") {
        return Err(invalid_config(
            path,
            format!(
                "`rules[{idx}]` selector `{trimmed}` uses a deferred v1 prefix (`tag:`/`inspect:` are not supported in v1)."
            ),
        ));
    }

    let glob = parse_selector_glob(path, idx, "", trimmed)?;
    Ok(PolicyCanonSelector::Local(glob))
}

fn parse_selector_glob(
    path: &str,
    idx: usize,
    prefix: &str,
    raw_pattern: &str,
) -> Result<GlobPattern, DiscoverError> {
    let trimmed = raw_pattern.trim();
    if trimmed.is_empty() {
        return Err(invalid_config(
            path,
            format!("`rules[{idx}]` selector `{prefix}{raw_pattern}` must not be empty."),
        ));
    }
    validate_no_traversal_or_absolute(path, &format!("`rules[{idx}]` selector"), trimmed)?;

    let stripped = strip_trailing_canon_ext(trimmed);
    if stripped.trim().is_empty() {
        return Err(invalid_config(
            path,
            format!(
                "`rules[{idx}]` selector `{prefix}{raw_pattern}` is empty after extension normalization."
            ),
        ));
    }

    GlobPattern::parse(stripped).map_err(|err| {
        invalid_config(
            path,
            format!("`rules[{idx}]` selector `{prefix}{raw_pattern}`: {err}"),
        )
    })
}

fn parse_exhibits(
    path: &str,
    idx: usize,
    value: &Value,
) -> Result<Vec<GlobPattern>, DiscoverError> {
    let raw_items: Vec<&str> = match value {
        Value::String(s) => vec![s.as_str()],
        Value::Sequence(seq) => {
            let mut items = Vec::with_capacity(seq.len());
            for (item_idx, elem) in seq.iter().enumerate() {
                let Some(s) = elem.as_str() else {
                    return Err(invalid_config(
                        path,
                        format!("`rules[{idx}].exhibits[{item_idx}]` must be a glob string."),
                    ));
                };
                items.push(s);
            }
            items
        }
        _ => {
            return Err(invalid_config(
                path,
                format!("`rules[{idx}].exhibits` must be a glob string or list of glob strings."),
            ));
        }
    };

    let mut globs = Vec::with_capacity(raw_items.len());
    for raw in raw_items {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(invalid_config(
                path,
                format!("`rules[{idx}].exhibits` contains an empty glob pattern."),
            ));
        }
        if trimmed.starts_with("plane=") {
            return Err(invalid_config(
                path,
                format!(
                    "`rules[{idx}].exhibits` entry `{trimmed}` uses a deferred fielded exhibit query (not supported in v1)."
                ),
            ));
        }
        validate_no_traversal_or_absolute(path, &format!("`rules[{idx}].exhibits`"), trimmed)?;
        let glob = GlobPattern::parse(trimmed).map_err(|err| {
            invalid_config(
                path,
                format!("`rules[{idx}].exhibits` pattern `{raw}`: {err}"),
            )
        })?;
        globs.push(glob);
    }

    Ok(globs)
}

fn validate_no_traversal_or_absolute(
    path: &str,
    context: &str,
    raw_pattern: &str,
) -> Result<(), DiscoverError> {
    let posix = raw_pattern.replace('\\', "/");
    if posix.starts_with('/') {
        return Err(invalid_config(
            path,
            format!(
                "{context} `{raw_pattern}` must be relative to its enclosing scope (leading `/` is forbidden)."
            ),
        ));
    }
    if posix.split('/').any(|seg| seg == "..") {
        return Err(invalid_config(
            path,
            format!("{context} `{raw_pattern}` must not contain directory traversal (`../`)."),
        ));
    }
    Ok(())
}

fn strip_trailing_canon_ext(pattern: &str) -> &str {
    if pattern.len() >= CANON_FILE_EXT.len()
        && pattern[pattern.len() - CANON_FILE_EXT.len()..].eq_ignore_ascii_case(CANON_FILE_EXT)
    {
        &pattern[..pattern.len() - CANON_FILE_EXT.len()]
    } else {
        pattern
    }
}

fn unknown_key_error(path: &str, key: &str, location: &str) -> DiscoverError {
    if RUNTIME_CREDENTIAL_KEYS.contains(&key) {
        invalid_config(
            path,
            format!(
                "Forbidden runtime credential/provider key `{key}` in {location}. \
                 Runtime provider bindings and API keys must reside outside the workspace \
                 in `~/.config/canon-clerk/config.json`."
            ),
        )
    } else if DEFERRED_TOP_LEVEL_KEYS.contains(&key) {
        invalid_config(
            path,
            format!(
                "Unsupported {location} key `{key}` (`{key}:` is deferred beyond v1 and cannot be used in `.canons/config.yaml`)."
            ),
        )
    } else {
        invalid_config(
            path,
            format!("Unrecognized {location} key `{key}` in `.canons/config.yaml`."),
        )
    }
}

fn invalid_config(path: &str, message: impl Into<String>) -> DiscoverError {
    DiscoverError::InvalidPolicyConfig {
        path: path.to_string(),
        message: message.into(),
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
