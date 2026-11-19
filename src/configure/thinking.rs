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

//! Extended-thinking parsing, per-layer mutual exclusivity, and per-node cascade resolution (`configure.md` §3–4).

use crate::caseload::{HeuristicNode, ResolvedNodeConfig, ThinkingLevel};
use crate::configure::{
    ConfigureError, ConfigureOptions, ENV_REASONING_BUDGET, ENV_THINKING_LEVEL, ModelCapability,
    StoredConfig,
};

pub(super) type ThinkingPair = (Option<ThinkingLevel>, Option<u32>);

/// Pre-validated global thinking layers (`CLI` ≻ `Env` and `config.json defaults`).
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct GlobalThinkingLayers {
    pub cli_or_env: Option<ThinkingPair>,
    pub stored_defaults: Option<ThinkingPair>,
}

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

fn parse_stored_pair(
    raw_level: Option<&str>,
    budget: Option<u32>,
    source_label: &str,
) -> Result<Option<ThinkingPair>, ConfigureError> {
    let trimmed_lvl = raw_level.map(str::trim).filter(|s| !s.is_empty());
    match (trimmed_lvl, budget) {
        (Some(_), Some(_)) => Err(ConfigureError::ConflictingThinkingOptions {
            source: source_label.to_string(),
        }),
        (Some(lvl), None) => Ok(Some((Some(parse_thinking_level(lvl)?), None))),
        (None, Some(bud)) => Ok(Some((None, Some(bud)))),
        (None, None) => Ok(None),
    }
}

/// Validates global CLI, environment, and `defaults` extended-thinking layers up front.
pub(super) fn resolve_global_thinking_layers(
    options: &ConfigureOptions,
    stored: Option<&StoredConfig>,
) -> Result<GlobalThinkingLayers, ConfigureError> {
    let cli_pair = match (options.thinking_level, options.reasoning_budget) {
        (Some(_), Some(_)) => {
            return Err(ConfigureError::ConflictingThinkingOptions {
                source: "CLI flags (`--thinking-level` and `--reasoning-budget`)".to_string(),
            });
        }
        (Some(level), None) => Some((Some(level), None)),
        (None, Some(budget)) => Some((None, Some(budget))),
        (None, None) => None,
    };

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

    let env_pair = match (env_level, env_budget) {
        (Some(_), Some(_)) => {
            return Err(ConfigureError::ConflictingThinkingOptions {
                source: format!(
                    "environment variables (`{ENV_THINKING_LEVEL}` and `{ENV_REASONING_BUDGET}`)"
                ),
            });
        }
        (Some(raw_lvl), None) => Some((Some(parse_thinking_level(raw_lvl)?), None)),
        (None, Some(raw_bud)) => Some((None, Some(parse_reasoning_budget(raw_bud)?))),
        (None, None) => None,
    };

    let stored_defaults = match stored {
        Some(cfg) => parse_stored_pair(
            cfg.defaults.thinking_level.as_deref(),
            cfg.defaults.reasoning_budget,
            "configuration file (`defaults.thinkingLevel` and `defaults.reasoningBudget`)",
        )?,
        None => None,
    };

    Ok(GlobalThinkingLayers {
        cli_or_env: cli_pair.or(env_pair),
        stored_defaults,
    })
}

/// Resolves `(thinking_level, reasoning_budget)` for a single `node` and its resolved `model`:
/// - `Discriminator` models reject explicit per-node `nodes.<node>.{thinkingLevel, reasoningBudget}`
///   with exit `2` while leaving shared/ambient thinking unset (`(None, None)`).
/// - `Generator` models cascade: `CLI` ≻ `Env` ≻ `nodes.<node>.*` ≻ `defaults.*` ≻ `existing_node` ≻ `(None, None)`.
pub(super) fn resolve_node_thinking_options(
    node: HeuristicNode,
    model: &str,
    capability: ModelCapability,
    global: GlobalThinkingLayers,
    stored: Option<&StoredConfig>,
    existing_node: Option<&ResolvedNodeConfig>,
) -> Result<ThinkingPair, ConfigureError> {
    let node_stored = match stored.and_then(|s| s.node(node)) {
        Some(n) => parse_stored_pair(
            n.thinking_level.as_deref(),
            n.reasoning_budget,
            &format!(
                "configuration file (`nodes.{node}.thinkingLevel` and `nodes.{node}.reasoningBudget`)"
            ),
        )?,
        None => None,
    };

    if capability == ModelCapability::Discriminator {
        if node_stored.is_some() {
            return Err(ConfigureError::DiscriminativeNodeThinkingUnsupported {
                node,
                specifier: model.to_string(),
            });
        }
        return Ok((None, None));
    }

    if let Some(pair) = global.cli_or_env {
        return Ok(pair);
    }
    if let Some(pair) = node_stored {
        return Ok(pair);
    }
    if let Some(pair) = global.stored_defaults {
        return Ok(pair);
    }
    if let Some(existing) = existing_node {
        if existing.thinking_level.is_some() && existing.reasoning_budget.is_some() {
            return Err(ConfigureError::ConflictingThinkingOptions {
                source: format!(
                    "upstream Caseload `.config.nodes.{node}` (`thinking_level` and `reasoning_budget`)"
                ),
            });
        }
        return Ok((existing.thinking_level, existing.reasoning_budget));
    }

    Ok((None, None))
}
