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

//! Thinking level and reasoning budget parsing, precedence cascade, and mutual-exclusivity checks.

use crate::caseload::{CaseloadConfig, ThinkingLevel};
use crate::configure::{
    ConfigureError, ConfigureOptions, ENV_REASONING_BUDGET, ENV_THINKING_LEVEL, StoredConfig,
};

/// Parses a raw `--thinking-level` / `CANON_CLERK_THINKING_LEVEL` / `thinkingLevel` string.
pub fn parse_thinking_level(raw: &str) -> Result<ThinkingLevel, ConfigureError> {
    raw.parse::<ThinkingLevel>()
        .map_err(|_| ConfigureError::InvalidThinkingLevel {
            raw: raw.to_string(),
        })
}

fn parse_reasoning_budget(raw: &str) -> Result<u32, ConfigureError> {
    raw.trim()
        .parse::<u32>()
        .map_err(|_| ConfigureError::InvalidReasoningBudget {
            raw: raw.to_string(),
        })
}

/// Resolves mutually exclusive `(thinking_level, reasoning_budget)` across CLI overrides,
/// environment variables, user `config.json`, and upstream `caseload.config`.
pub(super) fn resolve_thinking_options(
    options: &ConfigureOptions,
    stored: Option<&StoredConfig>,
    existing_config: Option<&CaseloadConfig>,
) -> Result<(Option<ThinkingLevel>, Option<u32>), ConfigureError> {
    match (options.thinking_level, options.reasoning_budget) {
        (Some(_), Some(_)) => {
            return Err(ConfigureError::ConflictingThinkingOptions {
                source: "CLI flags (`--thinking-level` and `--reasoning-budget`)".to_string(),
            });
        }
        (Some(level), None) => return Ok((Some(level), None)),
        (None, Some(budget)) => return Ok((None, Some(budget))),
        (None, None) => {}
    }

    let env_level = options
        .env
        .get(ENV_THINKING_LEVEL)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let env_budget = options
        .env
        .get(ENV_REASONING_BUDGET)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    match (env_level, env_budget) {
        (Some(_), Some(_)) => {
            return Err(ConfigureError::ConflictingThinkingOptions {
                source: format!(
                    "environment variables (`{ENV_THINKING_LEVEL}` and `{ENV_REASONING_BUDGET}`)"
                ),
            });
        }
        (Some(raw_lvl), None) => return Ok((Some(parse_thinking_level(raw_lvl)?), None)),
        (None, Some(raw_bud)) => return Ok((None, Some(parse_reasoning_budget(raw_bud)?))),
        (None, None) => {}
    }

    let stored_level = stored
        .and_then(|s| s.thinking_level.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let stored_budget = stored.and_then(|s| s.reasoning_budget);

    match (stored_level, stored_budget) {
        (Some(_), Some(_)) => {
            return Err(ConfigureError::ConflictingThinkingOptions {
                source: "configuration file (`thinkingLevel` and `reasoningBudget`)".to_string(),
            });
        }
        (Some(raw_lvl), None) => return Ok((Some(parse_thinking_level(raw_lvl)?), None)),
        (None, Some(budget)) => return Ok((None, Some(budget))),
        (None, None) => {}
    }

    if let Some(existing) = existing_config {
        if existing.thinking_level.is_some() && existing.reasoning_budget.is_some() {
            return Err(ConfigureError::ConflictingThinkingOptions {
                source: "upstream Caseload `.config` (`thinking_level` and `reasoning_budget`)"
                    .to_string(),
            });
        }
        return Ok((existing.thinking_level, existing.reasoning_budget));
    }

    Ok((None, None))
}
