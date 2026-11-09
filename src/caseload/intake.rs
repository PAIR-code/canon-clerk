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

//! Data plane models for the `.intake` namespace (`docs/architecture/nodes/intake.md` §3).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// File modification status relative to the base commit or working tree.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Unchanged,
}

impl FileChangeStatus {
    /// Returns the canonical snake_case string label for display and serialization.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
            Self::Renamed => "renamed",
            Self::Copied => "copied",
            Self::Unchanged => "unchanged",
        }
    }
}

/// Explicit rationale recorded when a `FileArtifact`'s unified diff patch is omitted.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PatchOmissionReason {
    Unchanged,
    Binary,
    Oversized,
    NotRequested,
}

/// Explicit rationale recorded when a `FileArtifact`'s full file content is omitted.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContentOmissionReason {
    NotRequested,
    Binary,
    Oversized,
    Deleted,
}

/// Immutable representation of a modified or referenced file exhibit.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct FileArtifact {
    /// Repository-relative POSIX path (e.g. `"src/lib.rs"`).
    pub path: String,
    /// File status relative to the base commit.
    pub status: FileChangeStatus,
    /// Original repository-relative POSIX path prior to a rename or copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_path: Option<String>,
    /// Count of inserted lines across all hunks.
    pub lines_added: usize,
    /// Count of deleted lines across all hunks.
    pub lines_deleted: usize,
    /// Unified diff hunk text (present when modified hunks exist unless omitted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
    /// Explicit rationale when `patch` is `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_omission_reason: Option<PatchOmissionReason>,
    /// Full UTF-8 file content at `HEAD` (present when materialized unless omitted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Explicit rationale when `content` is `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_omission_reason: Option<ContentOmissionReason>,
}

/// Contextual metadata for an issue linked to a pull request or change filing.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct LinkedIssueContext {
    /// Issue number in the repository issue tracker.
    pub number: u64,
    /// Issue title text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Issue description body in Markdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Canonical web URL to the issue.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Target evaluation scope established during `intake`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IntakeScope {
    /// Evaluates specific modified or targeted files/globs.
    #[default]
    #[serde(rename = "targeted")]
    Targeted,
    /// Evaluates the entire subject-matter repository codebase (`--all-targets`).
    #[serde(rename = "all-targets")]
    AllTargets,
}

impl IntakeScope {
    /// Returns the canonical wire representation (`"targeted"` or `"all-targets"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Targeted => "targeted",
            Self::AllTargets => "all-targets",
        }
    }
}

/// Ingested filing exhibits and discovery directives captured during the `intake` stage.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct CaseloadIntake {
    /// Pull request title or commit subject line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_title: Option<String>,
    /// Pull request Markdown description or commit body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_body: Option<String>,
    /// Prospective architectural design intent or plan description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    /// Scope of intake target evaluation (`"targeted"` or `"all-targets"`).
    #[serde(default)]
    pub scope: IntakeScope,
    /// Stably sorted target file paths, directory pointers, or glob directives for `discover`.
    #[serde(default)]
    pub target_paths: Vec<String>,
    /// Ingested code modifications keyed in deterministic lexicographical order by POSIX path.
    #[serde(default)]
    pub diffs: BTreeMap<String, FileArtifact>,
    /// Optional linked issue contexts associated with the filing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_issues: Option<Vec<LinkedIssueContext>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::Caseload;

    #[test]
    fn test_populated_caseload_intake_round_trip() {
        let mut diffs = BTreeMap::new();
        diffs.insert(
            "src/main.rs".to_string(),
            FileArtifact {
                path: "src/main.rs".to_string(),
                status: FileChangeStatus::Modified,
                previous_path: None,
                lines_added: 12,
                lines_deleted: 3,
                patch: Some("@@ -1,3 +1,12 @@\n+fn main() {}".to_string()),
                patch_omission_reason: None,
                content: None,
                content_omission_reason: Some(ContentOmissionReason::NotRequested),
            },
        );
        diffs.insert(
            "assets/logo.png".to_string(),
            FileArtifact {
                path: "assets/logo.png".to_string(),
                status: FileChangeStatus::Added,
                previous_path: None,
                lines_added: 0,
                lines_deleted: 0,
                patch: None,
                patch_omission_reason: Some(PatchOmissionReason::Binary),
                content: None,
                content_omission_reason: Some(ContentOmissionReason::Binary),
            },
        );

        let intake = CaseloadIntake {
            pr_title: Some("feat(intake): implement front door".to_string()),
            pr_body: Some("Implements #222".to_string()),
            intent: Some("Establish deterministic filing intake".to_string()),
            scope: IntakeScope::Targeted,
            target_paths: vec!["src/main.rs".to_string()],
            diffs,
            linked_issues: Some(vec![LinkedIssueContext {
                number: 222,
                title: Some("Implement intake in Rust".to_string()),
                body: None,
                url: Some("https://github.com/PAIR-code/canon-clerk/issues/222".to_string()),
            }]),
        };

        let caseload = Caseload::new().with_intake(intake);
        let json = serde_json::to_string_pretty(&caseload).expect("serialize caseload");
        let decoded: Caseload = serde_json::from_str(&json).expect("deserialize caseload");

        assert_eq!(decoded, caseload);
        // Verify BTreeMap deterministic key ordering ("assets/logo.png" precedes "src/main.rs")
        let assets_pos = json
            .find(r#""assets/logo.png":"#)
            .expect("assets key present");
        let main_pos = json.find(r#""src/main.rs":"#).expect("main key present");
        assert!(
            assets_pos < main_pos,
            "diffs map keys must serialize in deterministic lexicographical order"
        );
    }

    #[test]
    fn test_all_targets_scope_and_status_wire_labels() {
        let intake = CaseloadIntake {
            scope: IntakeScope::AllTargets,
            ..CaseloadIntake::default()
        };
        let json = serde_json::to_string(&intake).expect("serialize intake");
        assert!(json.contains(r#""scope":"all-targets""#));

        let decoded: CaseloadIntake = serde_json::from_str(&json).expect("deserialize intake");
        assert_eq!(decoded.scope, IntakeScope::AllTargets);
        assert_eq!(IntakeScope::Targeted.as_str(), "targeted");
        assert_eq!(IntakeScope::AllTargets.as_str(), "all-targets");
        assert_eq!(FileChangeStatus::Added.as_str(), "added");
        assert_eq!(FileChangeStatus::Modified.as_str(), "modified");
        assert_eq!(FileChangeStatus::Deleted.as_str(), "deleted");
        assert_eq!(FileChangeStatus::Renamed.as_str(), "renamed");
        assert_eq!(FileChangeStatus::Copied.as_str(), "copied");
        assert_eq!(FileChangeStatus::Unchanged.as_str(), "unchanged");
    }
}
