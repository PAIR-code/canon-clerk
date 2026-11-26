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

//! Data plane models for the `.survey` Caseload namespace (`docs/architecture/nodes/survey.md` §3).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// Default maximum materialized characters per surveyed repository-identity file.
pub const DEFAULT_SURVEY_MAX_FILE_CHARS: usize = 4_000;

/// Default maximum aggregate materialized characters across all surveyed repository-identity files.
pub const DEFAULT_SURVEY_MAX_TOTAL_CHARS: usize = 12_000;

/// Machine-readable reason a matched candidate file was omitted from `.survey.files`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SurveyOmissionReason {
    /// Aggregate `max_total_chars` character budget was exhausted by lexicographically earlier files.
    BudgetExceeded,
    /// File contained NUL bytes or non-UTF-8 binary data.
    Binary,
    /// Symlink resolved outside the canonical workspace root (or could not be verified within it).
    OutsideWorkspace,
}

impl SurveyOmissionReason {
    /// Returns the canonical `snake_case` wire representation of this omission reason.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BudgetExceeded => "budget_exceeded",
            Self::Binary => "binary",
            Self::OutsideWorkspace => "outside_workspace",
        }
    }
}

impl fmt::Display for SurveyOmissionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Materialized UTF-8 repository-identity file recorded in `.survey.files`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SurveyFile {
    /// Materialized UTF-8 text (bounded by `max_file_chars` and remaining `max_total_chars`).
    pub content: String,
    /// Character count (`content.chars().count()`) of the materialized `content`.
    pub chars: usize,
    /// Full character count of the file on disk prior to truncation.
    pub original_chars: usize,
    /// `true` when `content` was truncated (`chars < original_chars`), `false` otherwise.
    pub truncated: bool,
}

/// Cumulative `.survey` Caseload namespace produced by Branch C (`survey`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CaseloadSurvey {
    /// Stably sorted workspace-relative POSIX paths of loaded `<scope>/.canons/policy-chains.yaml` files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_files: Vec<String>,
    /// Effective per-file character ceiling applied during materialization.
    pub max_file_chars: usize,
    /// Effective aggregate character ceiling across all materialized survey files.
    pub max_total_chars: usize,
    /// Sum of `chars` across all materialized entries in `files`.
    pub total_chars: usize,
    /// Materialized UTF-8 repository-identity files keyed by workspace-relative POSIX path.
    #[serde(default)]
    pub files: BTreeMap<String, SurveyFile>,
    /// Matched candidate files excluded from `files`, keyed by workspace-relative POSIX path.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub omitted: BTreeMap<String, SurveyOmissionReason>,
}

impl Default for CaseloadSurvey {
    fn default() -> Self {
        Self {
            policy_files: Vec::new(),
            max_file_chars: DEFAULT_SURVEY_MAX_FILE_CHARS,
            max_total_chars: DEFAULT_SURVEY_MAX_TOTAL_CHARS,
            total_chars: 0,
            files: BTreeMap::new(),
            omitted: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_survey_serializes_cleanly_without_optional_empty_fields() {
        let survey = CaseloadSurvey::default();
        let json = serde_json::to_string(&survey).expect("serialize empty survey");
        assert_eq!(
            json,
            r#"{"max_file_chars":4000,"max_total_chars":12000,"total_chars":0,"files":{}}"#
        );
        let decoded: CaseloadSurvey = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, survey);
    }

    #[test]
    fn test_survey_omission_reasons_round_trip_snake_case() {
        for (reason, expected) in [
            (SurveyOmissionReason::BudgetExceeded, "\"budget_exceeded\""),
            (SurveyOmissionReason::Binary, "\"binary\""),
            (
                SurveyOmissionReason::OutsideWorkspace,
                "\"outside_workspace\"",
            ),
        ] {
            let encoded = serde_json::to_string(&reason).expect("serialize reason");
            assert_eq!(encoded, expected);
            let decoded: SurveyOmissionReason =
                serde_json::from_str(&encoded).expect("deserialize reason");
            assert_eq!(decoded, reason);
        }
    }
}
