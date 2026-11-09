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

//! Core data plane models for the cumulative `Caseload` state container (`.intake`, `.discovery`, `.validation`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Current canonical schema version for serialized `Caseload` envelopes.
pub const CASELOAD_SCHEMA_VERSION: &str = "1.0";

fn default_caseload_version() -> String {
    CASELOAD_SCHEMA_VERSION.to_string()
}

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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
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

impl Default for CaseloadIntake {
    fn default() -> Self {
        Self {
            pr_title: None,
            pr_body: None,
            intent: None,
            scope: IntakeScope::Targeted,
            target_paths: Vec::new(),
            diffs: BTreeMap::new(),
            linked_issues: None,
        }
    }
}

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

    /// Stably sorted discovered canon file paths or qualified realm references matching the filing (or full corpus).
    #[serde(default)]
    pub candidate_canons: Vec<String>,

    /// Pruned subset of materialized exhibits triggered or inspected by at least one surviving candidate canon.
    #[serde(default)]
    pub active_exhibits: ActiveExhibits,

    /// Deterministic mapping from each candidate canon identifier to the matched target file paths and inspected exhibit planes.
    #[serde(default)]
    pub triggers_join: BTreeMap<String, Vec<String>>,
}

/// Severity classification for a static canon diagnostic or compliance annotation (`docs/architecture/nodes/validate.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationLevel {
    /// Blocking validation or compliance error (`"failure"`).
    Failure,
    /// Non-blocking advisory lint warning (`"warning"`).
    Warning,
    /// Informational diagnostic notice (`"notice"`).
    Notice,
}

impl AnnotationLevel {
    /// Returns the canonical wire representation (`"failure"`, `"warning"`, or `"notice"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Failure => "failure",
            Self::Warning => "warning",
            Self::Notice => "notice",
        }
    }
}

/// Line-and-column source diagnostic annotation (`docs/architecture/nodes/validate.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CodeAnnotation {
    /// Workspace-relative POSIX file path or canon source reference.
    pub path: String,
    /// 1-based starting line number.
    pub start_line: usize,
    /// 1-based ending line number.
    pub end_line: usize,
    /// Optional 1-based starting column number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_column: Option<usize>,
    /// Optional 1-based ending column number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<usize>,
    /// Severity level (`"failure"`, `"warning"`, or `"notice"`).
    pub annotation_level: AnnotationLevel,
    /// Human-readable diagnostic explanation.
    pub message: String,
    /// Optional rule identifier or short diagnostic title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Overall validation outcome for an individual candidate canon (`"pass"` or `"fail"`).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ValidationOutcome {
    /// Canon passed static validation with zero blocking errors.
    #[default]
    Pass,
    /// Canon failed static validation with one or more errors (or exceeded `--max-warnings`).
    Fail,
}

impl ValidationOutcome {
    /// Returns the canonical wire representation (`"pass"` or `"fail"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
        }
    }
}

/// Static validation metadata and diagnostics for a single candidate canon (`docs/architecture/nodes/validate.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidatedCanonMetadata {
    /// Overall pass/fail validation result for this canon.
    pub result: ValidationOutcome,
    /// Count of warning-level diagnostics emitted for this canon.
    pub warning_count: usize,
    /// Count of failure-level (error) diagnostics emitted for this canon.
    pub error_count: usize,
    /// Stably sorted line-and-column diagnostics for this canon.
    #[serde(default)]
    pub diagnostics: Vec<CodeAnnotation>,
}

/// Candidate canon AST and frontmatter validation results captured during the `validate` stage (`docs/architecture/nodes/validate.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct CaseloadValidation {
    /// Deterministic map keyed by candidate canon identifier recording per-canon validation outcomes and diagnostics.
    #[serde(default)]
    pub results: BTreeMap<String, ValidatedCanonMetadata>,
    /// True when any candidate canon produced one or more validation errors (or when warnings exceed `--max-warnings`).
    #[serde(default)]
    pub has_errors: bool,
}

/// The central, cumulative state container flowing through the Canon Clerk Caseload DAG.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Caseload {
    /// Schema specification version (always `"1.0"`).
    #[serde(default = "default_caseload_version")]
    pub version: String,

    /// Intake: Tendered change diffs, target path directives, design intent, and PR metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intake: Option<CaseloadIntake>,

    /// Discovery: Matched target files, candidate canons, active exhibits, and trigger intersections.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery: Option<CaseloadDiscovery>,

    /// Validation: Candidate canon AST and frontmatter validation results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<CaseloadValidation>,

    /// Configuration: Resolved provider credentials, model specifiers, and runtime options.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,

    /// Probe: Live provider connectivity and endpoint latency diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probe: Option<serde_json::Value>,

    /// Apprisal: Prospective statutory applicability assessments against design intent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apprisal: Option<serde_json::Value>,

    /// Docket: Macro triage colorability assessments and active docket cases.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docket: Option<serde_json::Value>,

    /// Evidence: Micro triage admitted exhibits per active case.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<serde_json::Value>,

    /// Verdict: Final compliance decrees and line annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<serde_json::Value>,
}

impl Default for Caseload {
    fn default() -> Self {
        Self::new()
    }
}

impl Caseload {
    /// Constructs a new empty `Caseload` envelope stamped with schema version `"1.0"`.
    pub fn new() -> Self {
        Self {
            version: CASELOAD_SCHEMA_VERSION.to_string(),
            intake: None,
            discovery: None,
            validation: None,
            config: None,
            probe: None,
            apprisal: None,
            docket: None,
            evidence: None,
            verdict: None,
        }
    }

    /// Returns a new `Caseload` with the `.intake` namespace populated, preserving all other stage fields.
    pub fn with_intake(mut self, intake: CaseloadIntake) -> Self {
        self.intake = Some(intake);
        self
    }

    /// Returns a new `Caseload` with the `.discovery` namespace populated, preserving all other stage fields.
    pub fn with_discovery(mut self, discovery: CaseloadDiscovery) -> Self {
        self.discovery = Some(discovery);
        self
    }

    /// Returns a new `Caseload` with the `.validation` namespace populated, preserving all other stage fields.
    pub fn with_validation(mut self, validation: CaseloadValidation) -> Self {
        self.validation = Some(validation);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod serialization {
        use super::*;

        #[test]
        fn test_empty_caseload_round_trip() {
            let caseload = Caseload::new();
            let json = serde_json::to_string(&caseload).expect("serialize empty caseload");
            assert_eq!(json, r#"{"version":"1.0"}"#);

            let decoded: Caseload =
                serde_json::from_str(&json).expect("deserialize empty caseload");
            assert_eq!(decoded, caseload);
        }

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
        fn test_all_targets_scope_serialization() {
            let intake = CaseloadIntake {
                scope: IntakeScope::AllTargets,
                ..CaseloadIntake::default()
            };
            let json = serde_json::to_string(&intake).expect("serialize intake");
            assert!(json.contains(r#""scope":"all-targets""#));

            let decoded: CaseloadIntake = serde_json::from_str(&json).expect("deserialize intake");
            assert_eq!(decoded.scope, IntakeScope::AllTargets);
        }
    }

    mod envelope_immutability {
        use super::*;

        #[test]
        fn test_with_intake_preserves_existing_stage_namespaces() {
            let raw_upstream = r#"{
                "version": "1.0",
                "config": {
                    "screener_model": "gemini-flash-lite-latest"
                }
            }"#;
            let upstream: Caseload =
                serde_json::from_str(raw_upstream).expect("parse upstream caseload");
            assert!(upstream.config.is_some());
            assert!(upstream.intake.is_none());

            let enriched = upstream.clone().with_intake(CaseloadIntake::default());
            assert!(enriched.intake.is_some());
            assert_eq!(enriched.config, upstream.config);

            let with_disc = enriched
                .clone()
                .with_discovery(CaseloadDiscovery::default());
            assert!(with_disc.discovery.is_some());
            assert_eq!(with_disc.intake, enriched.intake);
            assert_eq!(with_disc.config, enriched.config);

            let with_val = with_disc
                .clone()
                .with_validation(CaseloadValidation::default());
            assert!(with_val.validation.is_some());
            assert_eq!(with_val.discovery, with_disc.discovery);
            assert_eq!(with_val.intake, with_disc.intake);
            assert_eq!(with_val.config, with_disc.config);
        }
    }
}
