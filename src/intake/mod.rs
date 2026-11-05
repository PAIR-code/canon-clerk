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

//! Core domain execution engine for the `intake` Caseload DAG stage (Branch A root node).

pub mod diff;

pub use diff::{
    DiffParseError, DiffToken, DiffTokenKind, assemble_diff_artifacts, normalize_diff_path,
    parse_unified_diff, tokenize_diff, unquote_git_path,
};

use crate::caseload::{Caseload, CaseloadIntake, IntakeScope, LinkedIssueContext};
use std::collections::BTreeSet;
use std::fmt;

/// Domain input parameters for executing the `intake` pipeline stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntakeOptions {
    /// Pull request title or commit subject line.
    pub pr_title: Option<String>,

    /// Pull request Markdown description or commit body.
    pub pr_body: Option<String>,

    /// Prospective architectural design intent or plan description.
    pub intent: Option<String>,

    /// Explicit target path, directory, or glob directives (`Some(vec![])` when an explicit stream yields 0 paths).
    pub target_paths: Option<Vec<String>>,

    /// Raw unified diff patch content (`Some("")` when an explicit diff stream yields 0 changes).
    pub patch_content: Option<String>,

    /// Whether to evaluate the entire repository codebase as target artifacts (`--all-targets`).
    pub all_targets: bool,

    /// Optional linked issues context.
    pub linked_issues: Option<Vec<LinkedIssueContext>>,
}

impl IntakeOptions {
    /// Returns `true` if at least one explicit filing source or directive was designated.
    pub fn has_designated_source(&self) -> bool {
        self.all_targets
            || self.patch_content.is_some()
            || self.target_paths.is_some()
            || has_non_blank_text(&self.intent)
            || has_non_blank_text(&self.pr_title)
            || has_non_blank_text(&self.pr_body)
            || self
                .linked_issues
                .as_ref()
                .is_some_and(|issues| !issues.is_empty())
    }
}

/// Domain errors produced during `intake` execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntakeError {
    /// Invoked with zero designated input sources and no pre-populated `.intake` on the incoming Caseload.
    MissingInputSource,
    /// Unified diff patch stream failed lexical or semantic parsing.
    MalformedDiff(DiffParseError),
}

impl fmt::Display for IntakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingInputSource => write!(
                f,
                "No filing input source provided for `intake`.\n  \
                 Hint: Provide target paths (`canon-clerk intake src/lib.rs`), \
                 pipe a unified diff (`git diff origin/main | canon-clerk intake --diff -`), \
                 pipe newline-delimited paths (`git diff --name-only | canon-clerk intake -`), \
                 specify `--all-targets`, `--intent <TEXT>`, or pass `--caseload <PATH>`."
            ),
            Self::MalformedDiff(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for IntakeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MissingInputSource => None,
            Self::MalformedDiff(err) => Some(err),
        }
    }
}

impl From<DiffParseError> for IntakeError {
    fn from(err: DiffParseError) -> Self {
        Self::MalformedDiff(err)
    }
}

fn has_non_blank_text(opt: &Option<String>) -> bool {
    opt.as_ref().is_some_and(|s| !s.trim().is_empty())
}

fn normalize_optional_text(opt: &Option<String>) -> Option<String> {
    opt.as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

/// Normalizes a target path or glob directive to POSIX separators and strips redundant `./` prefixes.
fn normalize_target_directive(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let posix = trimmed.replace('\\', "/");
    let cleaned = if posix == "." {
        ".".to_string()
    } else if let Some(stripped) = posix.strip_prefix("./") {
        stripped.to_string()
    } else {
        posix
    };
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Executes the pure `intake` domain stage, cataloging tendered exhibits and discovery directives
/// onto the cumulative [`Caseload`].
pub fn execute_intake(
    options: &IntakeOptions,
    incoming_caseload: Option<Caseload>,
) -> Result<Caseload, IntakeError> {
    let base_caseload = incoming_caseload.unwrap_or_default();

    if !options.has_designated_source() {
        // Fast-forward invariant: if an incoming Caseload already has `.intake` populated
        // and no new intake options were supplied, return the Caseload unchanged.
        if base_caseload.intake.is_some() {
            return Ok(base_caseload);
        }
        return Err(IntakeError::MissingInputSource);
    }

    let diffs = match &options.patch_content {
        Some(patch) => parse_unified_diff(patch)?,
        None => Default::default(),
    };

    let target_paths: Vec<String> = options
        .target_paths
        .as_ref()
        .map(|paths| {
            paths
                .iter()
                .filter_map(|p| normalize_target_directive(p))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        })
        .unwrap_or_default();

    let scope = if options.all_targets {
        IntakeScope::AllTargets
    } else {
        IntakeScope::Targeted
    };

    let linked_issues = options
        .linked_issues
        .clone()
        .filter(|issues| !issues.is_empty());

    let intake = CaseloadIntake {
        pr_title: normalize_optional_text(&options.pr_title),
        pr_body: normalize_optional_text(&options.pr_body),
        intent: normalize_optional_text(&options.intent),
        scope,
        target_paths,
        diffs,
        linked_issues,
    };

    Ok(base_caseload.with_intake(intake))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::FileChangeStatus;

    mod source_validation {
        use super::*;

        #[test]
        fn test_naked_invocation_fails_with_missing_input_source() {
            let options = IntakeOptions::default();
            let err = execute_intake(&options, None).expect_err("naked options must fail");
            assert_eq!(err, IntakeError::MissingInputSource);
            assert!(err.to_string().contains("Hint:"));
        }

        #[test]
        fn test_whitespace_only_metadata_is_treated_as_missing_source() {
            let options = IntakeOptions {
                pr_title: Some("   \n\t  ".to_string()),
                intent: Some(String::new()),
                ..IntakeOptions::default()
            };
            let err = execute_intake(&options, None)
                .expect_err("blank metadata without sources must fail");
            assert_eq!(err, IntakeError::MissingInputSource);
        }

        #[test]
        fn test_empty_explicit_diff_stream_succeeds_as_clean_noop() {
            let options = IntakeOptions {
                patch_content: Some(String::new()),
                ..IntakeOptions::default()
            };
            let caseload =
                execute_intake(&options, None).expect("explicit empty diff stream must succeed");
            let intake = caseload.intake.expect("intake populated");
            assert!(intake.diffs.is_empty());
            assert!(intake.target_paths.is_empty());
            assert_eq!(intake.scope, IntakeScope::Targeted);
        }

        #[test]
        fn test_empty_explicit_target_paths_stream_succeeds_as_clean_noop() {
            let options = IntakeOptions {
                target_paths: Some(Vec::new()),
                ..IntakeOptions::default()
            };
            let caseload =
                execute_intake(&options, None).expect("explicit empty path stream must succeed");
            let intake = caseload.intake.expect("intake populated");
            assert!(intake.diffs.is_empty());
            assert!(intake.target_paths.is_empty());
        }
    }

    mod directive_and_exhibit_cataloging {
        use super::*;

        #[test]
        fn test_target_paths_are_normalized_deduplicated_and_sorted() {
            let options = IntakeOptions {
                target_paths: Some(vec![
                    "./src/main.rs".to_string(),
                    "src\\lib.rs".to_string(),
                    "src/main.rs".to_string(),
                    "   ".to_string(),
                    "docs/**/*.md".to_string(),
                ]),
                ..IntakeOptions::default()
            };

            let caseload = execute_intake(&options, None).expect("execute intake");
            let intake = caseload.intake.expect("intake populated");
            assert_eq!(
                intake.target_paths,
                vec![
                    "docs/**/*.md".to_string(),
                    "src/lib.rs".to_string(),
                    "src/main.rs".to_string()
                ]
            );
        }

        #[test]
        fn test_all_targets_flag_sets_all_targets_scope() {
            let options = IntakeOptions {
                all_targets: true,
                ..IntakeOptions::default()
            };
            let caseload = execute_intake(&options, None).expect("execute intake");
            let intake = caseload.intake.expect("intake populated");
            assert_eq!(intake.scope, IntakeScope::AllTargets);
        }

        #[test]
        fn test_catalogs_diff_exhibits_and_pr_metadata() {
            let patch = "\
diff --git a/src/cli.rs b/src/cli.rs
index 1111111..2222222 100644
--- a/src/cli.rs
+++ b/src/cli.rs
@@ -1,1 +1,2 @@
 use clap::Parser;
+pub mod intake;
";
            let options = IntakeOptions {
                pr_title: Some("  feat(intake): add CLI subcommand  ".to_string()),
                pr_body: Some("Closes #222".to_string()),
                intent: Some("Support POSIX stream intake".to_string()),
                patch_content: Some(patch.to_string()),
                ..IntakeOptions::default()
            };

            let caseload = execute_intake(&options, None).expect("execute intake");
            let intake = caseload.intake.expect("intake populated");
            assert_eq!(
                intake.pr_title.as_deref(),
                Some("feat(intake): add CLI subcommand")
            );
            assert_eq!(intake.pr_body.as_deref(), Some("Closes #222"));
            assert_eq!(
                intake.intent.as_deref(),
                Some("Support POSIX stream intake")
            );
            assert_eq!(intake.diffs.len(), 1);
            assert_eq!(
                intake.diffs["src/cli.rs"].status,
                FileChangeStatus::Modified
            );
            assert_eq!(intake.diffs["src/cli.rs"].lines_added, 1);
        }
    }

    mod caseload_immutability_and_fast_forward {
        use super::*;

        #[test]
        fn test_fast_forwards_when_incoming_caseload_already_has_intake_and_no_new_options() {
            let existing_intake = CaseloadIntake {
                pr_title: Some("Existing filing".to_string()),
                ..CaseloadIntake::default()
            };
            let upstream = Caseload::new().with_intake(existing_intake.clone());

            let result = execute_intake(&IntakeOptions::default(), Some(upstream))
                .expect("fast-forward should succeed");
            assert_eq!(result.intake, Some(existing_intake));
        }

        #[test]
        fn test_preserves_upstream_sections_when_attaching_intake() {
            let mut upstream = Caseload::new();
            upstream.config =
                Some(serde_json::json!({ "screener_model": "gemini-flash-lite-latest" }));

            let options = IntakeOptions {
                intent: Some("Prospective auth rotation".to_string()),
                target_paths: Some(vec!["src/auth".to_string()]),
                ..IntakeOptions::default()
            };

            let enriched = execute_intake(&options, Some(upstream.clone())).expect("attach intake");
            assert_eq!(enriched.config, upstream.config);
            assert_eq!(
                enriched.intake.as_ref().unwrap().intent.as_deref(),
                Some("Prospective auth rotation")
            );
        }
    }
}
