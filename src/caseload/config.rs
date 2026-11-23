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

//! Data plane models for the `.config` namespace (`docs/architecture/nodes/configure.md` §3).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

/// Canonical default provider slug (`docs/architecture/nodes/configure.md` §4).
pub const DEFAULT_PROVIDER: &str = "google";

/// Canonical provider slug for TypeSafe AI decision models (`typesafe:jev-*`).
pub const TYPESAFE_PROVIDER: &str = "typesafe";

/// Canonical provider slug for built-in local System One daemons (`local:kev-4b`, etc.).
pub const LOCAL_PROVIDER: &str = "local";

/// Default canonical Google `Discriminator` model specifier (`docs/architecture/nodes/configure.md` §4).
pub const DEFAULT_GOOGLE_DISCRIMINATOR_MODEL: &str = "google:gemini-decision-1";

/// Default canonical Google `Generator` model specifier (`docs/architecture/nodes/configure.md` §4).
pub const DEFAULT_GOOGLE_GENERATOR_MODEL: &str = "google:gemini-4-carbon";

/// Canonical Google `Generator` model specifier frequently used when overriding screening stages.
pub const DEFAULT_GENERATIVE_SCREENING_MODEL: &str = "google:gemini-4-hydrogen";

/// Default canonical TypeSafe AI `Discriminator` model specifier (`docs/architecture/nodes/configure.md` §4).
pub const DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL: &str = "typesafe:jev-latest";

/// Default canonical built-in local `Discriminator` model specifier (`docs/architecture/nodes/configure.md` §4).
pub const DEFAULT_LOCAL_DISCRIMINATOR_MODEL: &str = "local:kev-4b";

/// Explicit minimal/zero reasoning budget signal for generative screening/vetting stages (`apprise`, `docket`, `admit`, `vet`).
pub const DEFAULT_SCREENING_REASONING_BUDGET: Option<u32> = Some(0);

/// Default qualitative thinking level applied to `audit` trials when neither `thinking_level` nor `reasoning_budget` is configured.
pub const DEFAULT_AUDIT_THINKING_LEVEL: ThinkingLevel = ThinkingLevel::Low;

/// Default qualitative thinking level applied to `probe` health checks on Gemini 4 models when unspecified.
pub const DEFAULT_PROBE_THINKING_LEVEL: ThinkingLevel = ThinkingLevel::Low;

/// Heuristic evaluation DAG node governed by per-node model and operational cascades (`configure.md` §3–4).
///
/// Variant ordering matches canonical DAG stage progression (`apprise` → `docket` → `admit` → `vet` → `audit`).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum HeuristicNode {
    /// Prospective statutory notice (`apprise`).
    Apprise,
    /// Macro subject-matter jurisdiction triage (`docket`).
    Docket,
    /// Micro evidentiary relevance triage (`admit`).
    Admit,
    /// Prima facie compliance vetting (`vet`).
    Vet,
    /// Substantive judicial adjudication (`audit`).
    Audit,
}

impl HeuristicNode {
    /// All five heuristic DAG nodes in canonical stage progression order.
    pub const ALL: [Self; 5] = [
        Self::Apprise,
        Self::Docket,
        Self::Admit,
        Self::Vet,
        Self::Audit,
    ];

    /// Returns the canonical `snake_case` node identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Apprise => "apprise",
            Self::Docket => "docket",
            Self::Admit => "admit",
            Self::Vet => "vet",
            Self::Audit => "audit",
        }
    }

    /// Returns the stage-prefixed CLI model flag (`"--apprise-model"`, etc.).
    pub const fn cli_model_flag(self) -> &'static str {
        match self {
            Self::Apprise => "--apprise-model",
            Self::Docket => "--docket-model",
            Self::Admit => "--admit-model",
            Self::Vet => "--vet-model",
            Self::Audit => "--audit-model",
        }
    }

    /// Returns the stage-prefixed model environment variable (`"CANON_CLERK_APPRISE_MODEL"`, etc.).
    pub const fn env_model_var(self) -> &'static str {
        match self {
            Self::Apprise => "CANON_CLERK_APPRISE_MODEL",
            Self::Docket => "CANON_CLERK_DOCKET_MODEL",
            Self::Admit => "CANON_CLERK_ADMIT_MODEL",
            Self::Vet => "CANON_CLERK_VET_MODEL",
            Self::Audit => "CANON_CLERK_AUDIT_MODEL",
        }
    }

    /// Returns `true` when this node supports candidate-canon `batchSize` configuration (`apprise`, `docket`).
    pub const fn supports_batch_size(self) -> bool {
        matches!(self, Self::Apprise | Self::Docket)
    }
}

impl fmt::Display for HeuristicNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for HeuristicNode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "apprise" => Ok(Self::Apprise),
            "docket" => Ok(Self::Docket),
            "admit" => Ok(Self::Admit),
            "vet" => Ok(Self::Vet),
            "audit" => Ok(Self::Audit),
            _ => Err(s.to_string()),
        }
    }
}

/// Provider-neutral qualitative reasoning effort tier (`docs/architecture/nodes/configure.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel {
    /// Minimal reasoning effort (maps to `"LOW"` on Gemini 4 models that lack a `"MINIMAL"` tier).
    Minimal,
    /// Low reasoning effort prioritizing fast turnaround and token economy (default for `audit` and `probe`).
    Low,
    /// Balanced reasoning depth and latency.
    Medium,
    /// Deep extended reasoning effort for intricate statutory analysis.
    High,
}

impl ThinkingLevel {
    /// Returns the canonical lowercase identifier (`"minimal"`, `"low"`, `"medium"`, `"high"`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    /// Returns the uppercase Proto3 enum wire string (`"MINIMAL"`, `"LOW"`, `"MEDIUM"`, `"HIGH"`).
    pub const fn gemini_wire_str(self) -> &'static str {
        match self {
            Self::Minimal => "MINIMAL",
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
        }
    }
}

impl fmt::Display for ThinkingLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ThinkingLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "minimal" => Ok(Self::Minimal),
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            _ => Err(s.to_string()),
        }
    }
}

/// Resolved runtime model and extended-thinking binding for a single [`HeuristicNode`] (`configure.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ResolvedNodeConfig {
    /// Canonical `<provider>:<model>` identifier bound to this node.
    pub model: String,
    /// Optional qualitative reasoning effort tier resolved for a `Generator`-bound node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    /// Optional token ceiling resolved for a `Generator`-bound node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_budget: Option<u32>,
}

impl ResolvedNodeConfig {
    /// Constructs a [`ResolvedNodeConfig`] with no explicit thinking overrides.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            thinking_level: None,
            reasoning_budget: None,
        }
    }

    /// Constructs a [`ResolvedNodeConfig`] with explicit optional thinking controls.
    pub fn with_thinking(
        model: impl Into<String>,
        thinking_level: Option<ThinkingLevel>,
        reasoning_budget: Option<u32>,
    ) -> Self {
        Self {
            model: model.into(),
            thinking_level,
            reasoning_budget,
        }
    }

    /// Returns effective `(thinking_level, reasoning_budget)` for generative screening/vetting stages,
    /// defaulting to minimal/zero reasoning ([`DEFAULT_SCREENING_REASONING_BUDGET`]) when unset.
    pub const fn effective_screening_thinking(&self) -> (Option<ThinkingLevel>, Option<u32>) {
        match (self.thinking_level, self.reasoning_budget) {
            (Some(level), _) => (Some(level), None),
            (None, Some(budget)) => (None, Some(budget)),
            (None, None) => (None, DEFAULT_SCREENING_REASONING_BUDGET),
        }
    }

    /// Returns effective `(thinking_level, reasoning_budget)` for generative `audit` trials,
    /// defaulting to [`DEFAULT_AUDIT_THINKING_LEVEL`] (`"low"`) when unset.
    pub const fn effective_audit_thinking(&self) -> (Option<ThinkingLevel>, Option<u32>) {
        match (self.thinking_level, self.reasoning_budget) {
            (Some(level), _) => (Some(level), None),
            (None, Some(budget)) => (None, Some(budget)),
            (None, None) => (Some(DEFAULT_AUDIT_THINKING_LEVEL), None),
        }
    }
}

fn default_provider() -> String {
    DEFAULT_PROVIDER.to_string()
}

/// Normalized runtime provider and per-node configuration captured during `configure` (`configure.md` §3).
/// Raw API keys are intentionally omitted to uphold the Credential Non-Persistence Invariant.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CaseloadConfig {
    /// Resolved active default provider slug (`"google"` or `"typesafe"`).
    #[serde(default = "default_provider")]
    pub provider: String,
    /// Resolved per-node `<provider>:<model>` and optional thinking bindings in canonical DAG order.
    #[serde(default)]
    pub nodes: BTreeMap<HeuristicNode, ResolvedNodeConfig>,
}

impl CaseloadConfig {
    /// Constructs a [`CaseloadConfig`] populated for a single [`HeuristicNode`].
    pub fn single_node(
        provider: impl Into<String>,
        node: HeuristicNode,
        cfg: ResolvedNodeConfig,
    ) -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(node, cfg);
        Self {
            provider: provider.into(),
            nodes,
        }
    }

    /// Inserts or replaces the [`ResolvedNodeConfig`] for `node` and returns the updated config.
    pub fn with_node(mut self, node: HeuristicNode, cfg: ResolvedNodeConfig) -> Self {
        self.nodes.insert(node, cfg);
        self
    }

    /// Inserts or replaces the model specifier for `node` (with default thinking) and returns the updated config.
    pub fn with_node_model(self, node: HeuristicNode, model: impl Into<String>) -> Self {
        self.with_node(node, ResolvedNodeConfig::new(model))
    }

    /// Returns the [`ResolvedNodeConfig`] bound to `node`, if provisioned.
    pub fn node(&self, node: HeuristicNode) -> Option<&ResolvedNodeConfig> {
        self.nodes.get(&node)
    }

    /// Returns the canonical `<provider>:<model>` specifier bound to `node`, if provisioned.
    pub fn model_for(&self, node: HeuristicNode) -> Option<&str> {
        self.nodes.get(&node).map(|c| c.model.as_str())
    }
}

impl Default for CaseloadConfig {
    fn default() -> Self {
        let mut nodes = BTreeMap::new();
        for node in [
            HeuristicNode::Apprise,
            HeuristicNode::Docket,
            HeuristicNode::Admit,
            HeuristicNode::Vet,
        ] {
            nodes.insert(
                node,
                ResolvedNodeConfig::new(DEFAULT_GOOGLE_DISCRIMINATOR_MODEL),
            );
        }
        nodes.insert(
            HeuristicNode::Audit,
            ResolvedNodeConfig::new(DEFAULT_GOOGLE_GENERATOR_MODEL),
        );
        Self {
            provider: DEFAULT_PROVIDER.to_string(),
            nodes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::Caseload;

    mod serialization_and_ordering {
        use super::*;

        #[test]
        fn test_caseload_config_canonical_node_order_and_optional_thinking_omission() {
            let default_cfg = CaseloadConfig::default();
            let json = serde_json::to_string(&default_cfg).expect("serialize default config");
            assert_eq!(
                json,
                r#"{"provider":"google","nodes":{"apprise":{"model":"google:gemini-decision-1"},"docket":{"model":"google:gemini-decision-1"},"admit":{"model":"google:gemini-decision-1"},"vet":{"model":"google:gemini-decision-1"},"audit":{"model":"google:gemini-4-carbon"}}}"#
            );
            assert!(!json.contains("reasoning_budget"));
            assert!(!json.contains("thinking_level"));

            let with_budget = CaseloadConfig::default().with_node(
                HeuristicNode::Audit,
                ResolvedNodeConfig::with_thinking(DEFAULT_GOOGLE_GENERATOR_MODEL, None, Some(4096)),
            );
            let caseload = Caseload::new().with_config(with_budget);
            let encoded = serde_json::to_string(&caseload).expect("serialize caseload");
            let decoded: Caseload = serde_json::from_str(&encoded).expect("deserialize caseload");
            assert_eq!(decoded, caseload);
            assert_eq!(
                decoded
                    .config
                    .as_ref()
                    .and_then(|c| c.node(HeuristicNode::Audit))
                    .and_then(|n| n.reasoning_budget),
                Some(4096)
            );
        }
    }

    mod enums_and_helpers {
        use super::*;

        #[test]
        fn test_heuristic_node_and_thinking_level_parsing() {
            for (raw, expected) in [
                ("apprise", HeuristicNode::Apprise),
                ("DOCKET", HeuristicNode::Docket),
                ("  admit ", HeuristicNode::Admit),
                ("vet", HeuristicNode::Vet),
                ("audit", HeuristicNode::Audit),
            ] {
                assert_eq!(raw.parse::<HeuristicNode>().expect("valid node"), expected);
            }
            assert!("screener".parse::<HeuristicNode>().is_err());

            for (raw, expected, wire) in [
                ("minimal", ThinkingLevel::Minimal, "MINIMAL"),
                ("  LOW ", ThinkingLevel::Low, "LOW"),
                ("Medium", ThinkingLevel::Medium, "MEDIUM"),
                ("HIGH", ThinkingLevel::High, "HIGH"),
            ] {
                let parsed: ThinkingLevel = raw.parse().expect("valid thinking level");
                assert_eq!(parsed, expected);
                assert_eq!(parsed.gemini_wire_str(), wire);
            }
            assert!("extreme".parse::<ThinkingLevel>().is_err());
        }
    }
}
