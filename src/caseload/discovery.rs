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

//! Data plane models for the `.discovery` namespace (`docs/architecture/nodes/discover.md` §3).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Discovery evaluation mode recorded during the `discover` stage (`docs/architecture/nodes/discover.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiscoveryMode {
    /// Evaluates candidate canons whose preconditions, triggers, and inspect planes match the filing.
    #[default]
    #[serde(rename = "triggered")]
    Triggered,
    /// Promotes all discoverable workspace canons into `candidate_canons` (`--all-canons`).
    #[serde(rename = "all-canons")]
    AllCanons,
}

impl DiscoveryMode {
    /// Returns the canonical wire representation (`"triggered"` or `"all-canons"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Triggered => "triggered",
            Self::AllCanons => "all-canons",
        }
    }
}

/// Materialized active exhibits retained after mutual pruning with candidate canons (`docs/architecture/nodes/discover.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct ActiveExhibits {
    /// Stably sorted repository-relative POSIX file paths triggered or inspected by at least one candidate canon.
    #[serde(default)]
    pub files: Vec<String>,
    /// Whether `pr_title` is present and inspected by at least one candidate canon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_title: Option<bool>,
    /// Whether `pr_body` is present and inspected by at least one candidate canon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_body: Option<bool>,
    /// Whether `commit_messages` is present and inspected by at least one candidate canon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_messages: Option<bool>,
    /// Whether `linked_issues` is present and inspected by at least one candidate canon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_issues: Option<bool>,
}

/// Resolved target scope, candidate canons, pruned active exhibits, and trigger intersections
/// captured during the `discover` stage (`docs/architecture/nodes/discover.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct CaseloadDiscovery {
    /// Active discovery mode (`"triggered"` vs. `"all-canons"`).
    #[serde(default)]
    pub mode: DiscoveryMode,
    /// Stably sorted repository-relative POSIX target file paths materialized from `.intake` directives and diffs.
    #[serde(default)]
    pub target_files: Vec<String>,
    /// Stably sorted workspace-relative POSIX paths of discovered `<scope>/.canons/policy-chains.yaml` policy files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_files: Vec<String>,
    /// Stably sorted discovered canon file paths or qualified realm references matching the filing (or full corpus).
    #[serde(default)]
    pub candidate_canons: Vec<String>,
    /// Optional ephemeral inline canon sources keyed by `inline:<name>` for `--caseload` resumption and archival.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inline_canons: BTreeMap<String, String>,
    /// Pruned subset of materialized exhibits triggered or inspected by at least one surviving candidate canon.
    #[serde(default)]
    pub active_exhibits: ActiveExhibits,
    /// Deterministic mapping from each candidate canon identifier to the matched target file paths and inspected exhibit planes.
    #[serde(default)]
    pub triggers_join: BTreeMap<String, Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::Caseload;

    #[test]
    fn test_caseload_discovery_round_trip_and_optional_exhibit_omission() {
        let mut triggers_join = BTreeMap::new();
        triggers_join.insert(
            "api-design/no-dup-literals".to_string(),
            vec!["src/lib.rs".to_string()],
        );
        triggers_join.insert(
            "git-workflow/precursor-contracts".to_string(),
            vec!["src/caseload/mod.rs".to_string()],
        );

        let discovery = CaseloadDiscovery {
            mode: DiscoveryMode::Triggered,
            target_files: vec!["src/caseload/mod.rs".to_string(), "src/lib.rs".to_string()],
            policy_files: Vec::new(),
            candidate_canons: vec![
                ".canons/api-design/no-dup-literals.md".to_string(),
                ".canons/git-workflow/precursor-contracts.md".to_string(),
            ],
            inline_canons: BTreeMap::new(),
            active_exhibits: ActiveExhibits {
                files: vec!["src/caseload/mod.rs".to_string(), "src/lib.rs".to_string()],
                pr_title: Some(true),
                pr_body: None,
                commit_messages: None,
                linked_issues: None,
            },
            triggers_join,
        };

        let caseload = Caseload::new().with_discovery(discovery);
        let encoded = serde_json::to_string(&caseload).expect("serialize discovery caseload");
        assert!(encoded.contains(r#""mode":"triggered""#));
        assert!(encoded.contains(r#""pr_title":true"#));
        assert!(!encoded.contains("policy_files"));
        assert!(!encoded.contains("pr_body"));
        assert!(!encoded.contains("commit_messages"));
        assert!(!encoded.contains("linked_issues"));

        let decoded: Caseload =
            serde_json::from_str(&encoded).expect("deserialize discovery caseload");
        assert_eq!(decoded, caseload);

        let mut with_policies = decoded.discovery.expect("discovery present");
        with_policies.policy_files = vec![
            ".canons/policy-chains.yaml".to_string(),
            "packages/ui/.canons/policy-chains.yaml".to_string(),
        ];
        let encoded_with_policies =
            serde_json::to_string(&with_policies).expect("serialize policy_files");
        assert!(encoded_with_policies.contains(
            r#""policy_files":[".canons/policy-chains.yaml","packages/ui/.canons/policy-chains.yaml"]"#
        ));
        let decoded_with_policies: CaseloadDiscovery =
            serde_json::from_str(&encoded_with_policies).expect("deserialize policy_files");
        assert_eq!(
            decoded_with_policies.policy_files,
            with_policies.policy_files
        );
        assert!(!encoded.contains("inline_canons"));

        let mut with_inline = decoded_with_policies;
        with_inline.inline_canons.insert(
            "inline:no-eval".to_string(),
            "Code MUST avoid dynamic eval().".to_string(),
        );
        let encoded_with_inline =
            serde_json::to_string(&with_inline).expect("serialize inline_canons");
        assert!(encoded_with_inline.contains(r#""inline_canons""#));
        let decoded_with_inline: CaseloadDiscovery =
            serde_json::from_str(&encoded_with_inline).expect("deserialize inline_canons");
        assert_eq!(decoded_with_inline.inline_canons, with_inline.inline_canons);
    }

    #[test]
    fn test_all_canons_mode_serialization_and_wire_labels() {
        let discovery = CaseloadDiscovery {
            mode: DiscoveryMode::AllCanons,
            ..CaseloadDiscovery::default()
        };
        let encoded = serde_json::to_string(&discovery).expect("serialize all-canons discovery");
        assert!(encoded.contains(r#""mode":"all-canons""#));

        let decoded: CaseloadDiscovery =
            serde_json::from_str(&encoded).expect("deserialize all-canons discovery");
        assert_eq!(decoded.mode, DiscoveryMode::AllCanons);
        assert_eq!(DiscoveryMode::Triggered.as_str(), "triggered");
        assert_eq!(DiscoveryMode::AllCanons.as_str(), "all-canons");
    }
}
