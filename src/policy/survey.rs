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

//! `survey-context-chain:` selector types and fail-fast YAML parser (`SPEC.md` §7.2).

use super::parse::{invalid_policy, unknown_key_error, validate_no_traversal_or_absolute};
use super::{PolicyAction, PolicyError};
use crate::discover::glob_matcher::GlobPattern;
use serde_yaml::Value;

pub(super) const SURVEY_CONTEXT_CHAIN_KEY: &str = "survey-context-chain";

/// Built-in repository-root default allowlist for `survey-context-chain:` fall-through (`SPEC.md` §7.2).
pub const DEFAULT_SURVEY_CONTEXT_PATHS: &[&str] = &[
    "README.md",
    "README.txt",
    "README",
    "llms.txt",
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "go.mod",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SurveyContextSelector {
    All,
    Glob(GlobPattern),
}

impl SurveyContextSelector {
    pub(crate) fn matches(&self, scope_rel_path: &str) -> bool {
        match self {
            Self::All => true,
            Self::Glob(glob) => glob.is_match(scope_rel_path),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledSurveyContextRule {
    pub(crate) action: PolicyAction,
    pub(crate) selector: SurveyContextSelector,
}

pub(super) fn parse_survey_context_chain(
    path: &str,
    rules_val: Option<&Value>,
) -> Result<Vec<CompiledSurveyContextRule>, PolicyError> {
    let Some(val) = rules_val else {
        return Ok(Vec::new());
    };
    if val.is_null() {
        return Ok(Vec::new());
    }
    let Some(rules_seq) = val.as_sequence() else {
        return Err(invalid_policy(
            path,
            "`survey-context-chain` must be a YAML sequence of policy entry mappings.",
        ));
    };

    let mut rules = Vec::with_capacity(rules_seq.len());
    for (idx, item) in rules_seq.iter().enumerate() {
        rules.push(parse_survey_context_rule(path, idx, item)?);
    }
    Ok(rules)
}

fn parse_survey_context_rule(
    path: &str,
    idx: usize,
    item: &Value,
) -> Result<CompiledSurveyContextRule, PolicyError> {
    let Some(mapping) = item.as_mapping() else {
        return Err(invalid_policy(
            path,
            format!(
                "`{SURVEY_CONTEXT_CHAIN_KEY}[{idx}]` must be a YAML mapping specifying `allow` or `deny`."
            ),
        ));
    };

    for key in mapping.keys() {
        let Some(key_str) = key.as_str() else {
            return Err(invalid_policy(
                path,
                format!("`{SURVEY_CONTEXT_CHAIN_KEY}[{idx}]` keys must be strings."),
            ));
        };
        if !matches!(key_str, "allow" | "deny") {
            return Err(unknown_key_error(
                path,
                key_str,
                &format!("`{SURVEY_CONTEXT_CHAIN_KEY}[{idx}]`"),
            ));
        }
    }

    let allow_val = mapping.get(Value::String("allow".to_string()));
    let deny_val = mapping.get(Value::String("deny".to_string()));

    let (action, selector_val) = match (allow_val, deny_val) {
        (Some(v), None) => (PolicyAction::Allow, v),
        (None, Some(v)) => (PolicyAction::Deny, v),
        (Some(_), Some(_)) => {
            return Err(invalid_policy(
                path,
                format!(
                    "`{SURVEY_CONTEXT_CHAIN_KEY}[{idx}]` cannot specify both `allow` and `deny`."
                ),
            ));
        }
        (None, None) => {
            return Err(invalid_policy(
                path,
                format!(
                    "`{SURVEY_CONTEXT_CHAIN_KEY}[{idx}]` must specify either `allow` or `deny`."
                ),
            ));
        }
    };

    let Some(selector_str) = selector_val.as_str() else {
        return Err(invalid_policy(
            path,
            format!(
                "`{SURVEY_CONTEXT_CHAIN_KEY}[{idx}]` action selector must be a non-empty string."
            ),
        ));
    };
    let trimmed = selector_str.trim();
    if trimmed.is_empty() {
        return Err(invalid_policy(
            path,
            format!("`{SURVEY_CONTEXT_CHAIN_KEY}[{idx}]` path selector must not be empty."),
        ));
    }

    let selector = if trimmed == "all" {
        SurveyContextSelector::All
    } else {
        validate_no_traversal_or_absolute(
            path,
            &format!("`{SURVEY_CONTEXT_CHAIN_KEY}[{idx}]` selector"),
            trimmed,
        )?;
        let glob = GlobPattern::parse(trimmed).map_err(|err| {
            invalid_policy(
                path,
                format!("`{SURVEY_CONTEXT_CHAIN_KEY}[{idx}]` selector `{selector_str}`: {err}"),
            )
        })?;
        SurveyContextSelector::Glob(glob)
    };

    Ok(CompiledSurveyContextRule { action, selector })
}
