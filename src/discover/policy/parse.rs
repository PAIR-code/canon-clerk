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

//! Fail-fast YAML schema parser and validator for `<scope>/.canons/config.yaml` (`SPEC.md` §7.2).

use super::{CompiledPolicyConfig, CompiledPolicyRule, PolicyAction, PolicyCanonSelector};
use crate::canon::CANON_FILE_EXT;
use crate::discover::DiscoverError;
use crate::discover::glob_matcher::GlobPattern;
use serde_yaml::Value;

const POLICY_CHAIN_KEY: &str = "canon-policy-chain";

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

pub(super) fn parse_policy_config(
    path: &str,
    raw_yaml: &str,
) -> Result<CompiledPolicyConfig, DiscoverError> {
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
            "Root YAML document must be a mapping (e.g., `canon-policy-chain: [...]`).",
        ));
    };

    for key in root.keys() {
        let Some(key_str) = key.as_str() else {
            return Err(invalid_config(
                path,
                "Top-level keys in `.canons/config.yaml` must be strings.",
            ));
        };
        if key_str != POLICY_CHAIN_KEY {
            return Err(unknown_key_error(path, key_str, "top-level"));
        }
    }

    let Some(rules_val) = root.get(Value::String(POLICY_CHAIN_KEY.to_string())) else {
        return Ok(CompiledPolicyConfig { rules: Vec::new() });
    };

    if rules_val.is_null() {
        return Ok(CompiledPolicyConfig { rules: Vec::new() });
    }

    let Some(rules_seq) = rules_val.as_sequence() else {
        return Err(invalid_config(
            path,
            "`canon-policy-chain` must be a YAML sequence of policy entry mappings.",
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
            format!(
                "`{POLICY_CHAIN_KEY}[{idx}]` must be a YAML mapping specifying `allow` or `deny`."
            ),
        ));
    };

    for key in mapping.keys() {
        let Some(key_str) = key.as_str() else {
            return Err(invalid_config(
                path,
                format!("`{POLICY_CHAIN_KEY}[{idx}]` keys must be strings."),
            ));
        };
        if !matches!(key_str, "allow" | "deny" | "exhibits") {
            return Err(unknown_key_error(
                path,
                key_str,
                &format!("`{POLICY_CHAIN_KEY}[{idx}]`"),
            ));
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
                format!("`{POLICY_CHAIN_KEY}[{idx}]` cannot specify both `allow` and `deny`."),
            ));
        }
        (None, None) => {
            return Err(invalid_config(
                path,
                format!("`{POLICY_CHAIN_KEY}[{idx}]` must specify either `allow` or `deny`."),
            ));
        }
    };

    let Some(selector_str) = selector_val.as_str() else {
        return Err(invalid_config(
            path,
            format!("`{POLICY_CHAIN_KEY}[{idx}]` action selector must be a non-empty string."),
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
            format!("`{POLICY_CHAIN_KEY}[{idx}]` canon selector must not be empty."),
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
                "`{POLICY_CHAIN_KEY}[{idx}]` selector `{trimmed}` uses a deferred v1 prefix (`tag:`/`inspect:` are not supported in v1)."
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
            format!(
                "`{POLICY_CHAIN_KEY}[{idx}]` selector `{prefix}{raw_pattern}` must not be empty."
            ),
        ));
    }
    validate_no_traversal_or_absolute(
        path,
        &format!("`{POLICY_CHAIN_KEY}[{idx}]` selector"),
        trimmed,
    )?;

    let stripped = strip_trailing_canon_ext(trimmed);
    if stripped.trim().is_empty() {
        return Err(invalid_config(
            path,
            format!(
                "`{POLICY_CHAIN_KEY}[{idx}]` selector `{prefix}{raw_pattern}` is empty after extension normalization."
            ),
        ));
    }

    GlobPattern::parse(stripped).map_err(|err| {
        invalid_config(
            path,
            format!("`{POLICY_CHAIN_KEY}[{idx}]` selector `{prefix}{raw_pattern}`: {err}"),
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
                        format!(
                            "`{POLICY_CHAIN_KEY}[{idx}].exhibits[{item_idx}]` must be a glob string."
                        ),
                    ));
                };
                items.push(s);
            }
            items
        }
        _ => {
            return Err(invalid_config(
                path,
                format!(
                    "`{POLICY_CHAIN_KEY}[{idx}].exhibits` must be a glob string or list of glob strings."
                ),
            ));
        }
    };

    let mut globs = Vec::with_capacity(raw_items.len());
    for raw in raw_items {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(invalid_config(
                path,
                format!("`{POLICY_CHAIN_KEY}[{idx}].exhibits` contains an empty glob pattern."),
            ));
        }
        if trimmed.starts_with("plane=") {
            return Err(invalid_config(
                path,
                format!(
                    "`{POLICY_CHAIN_KEY}[{idx}].exhibits` entry `{trimmed}` uses a deferred fielded exhibit query (not supported in v1)."
                ),
            ));
        }
        validate_no_traversal_or_absolute(
            path,
            &format!("`{POLICY_CHAIN_KEY}[{idx}].exhibits`"),
            trimmed,
        )?;
        let glob = GlobPattern::parse(trimmed).map_err(|err| {
            invalid_config(
                path,
                format!("`{POLICY_CHAIN_KEY}[{idx}].exhibits` pattern `{raw}`: {err}"),
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
