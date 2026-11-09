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

//! Data plane models for the `.validation` namespace (`docs/architecture/nodes/validate.md` §3).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::Caseload;

    #[test]
    fn test_caseload_validation_round_trip_and_optional_annotation_fields() {
        let mut results = BTreeMap::new();
        results.insert(
            ".canons/api-design/example.md".to_string(),
            ValidatedCanonMetadata {
                result: ValidationOutcome::Fail,
                warning_count: 1,
                error_count: 1,
                diagnostics: vec![CodeAnnotation {
                    path: ".canons/api-design/example.md".to_string(),
                    start_line: 4,
                    end_line: 4,
                    start_column: None,
                    end_column: None,
                    annotation_level: AnnotationLevel::Failure,
                    message: "Unknown frontmatter key".to_string(),
                    title: None,
                }],
            },
        );

        let validation = CaseloadValidation {
            results,
            has_errors: true,
        };
        let caseload = Caseload::new().with_validation(validation);
        let encoded = serde_json::to_string(&caseload).expect("serialize validation caseload");
        assert!(encoded.contains(r#""result":"fail""#));
        assert!(encoded.contains(r#""annotation_level":"failure""#));
        assert!(!encoded.contains("start_column"));
        assert!(!encoded.contains("end_column"));
        assert!(!encoded.contains("title"));

        let decoded: Caseload =
            serde_json::from_str(&encoded).expect("deserialize validation caseload");
        assert_eq!(decoded, caseload);
    }

    #[test]
    fn test_annotation_level_ordering_and_wire_labels() {
        assert!(AnnotationLevel::Failure < AnnotationLevel::Warning);
        assert!(AnnotationLevel::Warning < AnnotationLevel::Notice);
        assert_eq!(AnnotationLevel::Failure.as_str(), "failure");
        assert_eq!(AnnotationLevel::Warning.as_str(), "warning");
        assert_eq!(AnnotationLevel::Notice.as_str(), "notice");
        assert_eq!(ValidationOutcome::Pass.as_str(), "pass");
        assert_eq!(ValidationOutcome::Fail.as_str(), "fail");
    }
}
