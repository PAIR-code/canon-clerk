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

//! Core domain execution engine and static canon linter for the `validate` Caseload DAG stage (`docs/architecture/nodes/validate.md`).

mod rules_body;
mod rules_frontmatter;

use crate::canon::{
    Canon, DEFAULT_FALLBACK_ID, ParseCanonOptions, derive_id, extract_frontmatter_from_tokens,
    parse_body_from_tokens, resolve_source_info, tokenize_canon,
};
use crate::caseload::{
    AnnotationLevel, Caseload, CaseloadValidation, CodeAnnotation, ValidatedCanonMetadata,
    ValidationOutcome,
};
use crate::discover::{DiscoverError, WorkspaceSnapshot};
use rules_body::lint_body;
use rules_frontmatter::{annotation_from_parse_error, lint_frontmatter};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

/// Domain input parameters for executing the `validate` pipeline stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidateOptions {
    /// Optional maximum total warning threshold before `has_errors` transitions to `true` (`--max-warnings <n>`).
    pub max_warnings: Option<usize>,
}

/// Domain errors produced during `validate` execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidateError {
    /// Invoked without `caseload.discovery` populated upstream.
    MissingDiscovery,
    /// Workspace or canon file read/discovery error.
    Discover(DiscoverError),
}

impl fmt::Display for ValidateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDiscovery => write!(
                f,
                "No filing source or canon scope provided for `validate`.\n  \
                 Hint: Pass `--all-canons` to validate the entire repository corpus, \
                 or provide a diff (`--diff -`) or target files to validate triggered canons."
            ),
            Self::Discover(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for ValidateError {}

impl From<DiscoverError> for ValidateError {
    fn from(err: DiscoverError) -> Self {
        Self::Discover(err)
    }
}

/// Pure static linter that evaluates a single Canon Markdown document in-memory and returns
/// structured [`ValidatedCanonMetadata`] with deterministically sorted [`CodeAnnotation`]s.
///
/// Strictly pure with zero filesystem or network I/O side effects.
pub fn lint_canon_source(raw_content: &str, options: &ParseCanonOptions) -> ValidatedCanonMetadata {
    let source_info = options.source_ref.as_deref().and_then(resolve_source_info);
    let path = source_info
        .as_ref()
        .map(|s| s.source_ref.as_str())
        .or(options.source_ref.as_deref())
        .unwrap_or("<memory>");

    let effective_scope = options
        .scope
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.replace('\\', "/").trim_matches('/').to_string())
        .or_else(|| {
            source_info
                .as_ref()
                .and_then(|s| s.scope.as_ref().map(|sc| sc.0.clone()))
        });

    let mut diagnostics = Vec::new();

    let tokens = match tokenize_canon(raw_content) {
        Ok(t) => t,
        Err(err) => {
            diagnostics.push(annotation_from_parse_error(path, &err));
            return finalize_metadata(diagnostics, false);
        }
    };

    let extracted_opt = match extract_frontmatter_from_tokens(&tokens) {
        Ok(ext) => {
            lint_frontmatter(
                path,
                source_info.as_ref(),
                effective_scope.as_deref(),
                &ext,
                &mut diagnostics,
            );
            Some(ext)
        }
        Err(err) => {
            diagnostics.push(annotation_from_parse_error(path, &err));
            None
        }
    };

    let body = parse_body_from_tokens(&tokens);
    let derived_id = derive_id(
        extracted_opt.as_ref().and_then(|e| e.frontmatter.get("id")),
        source_info.as_ref(),
    );

    if body.invariant.trim().is_empty() && derived_id == DEFAULT_FALLBACK_ID {
        diagnostics.push(CodeAnnotation {
            path: path.to_string(),
            start_line: 1,
            end_line: 1,
            start_column: Some(1),
            end_column: Some(1),
            annotation_level: AnnotationLevel::Failure,
            message: "Canon is missing both a body invariant statement and a derivable identifier."
                .to_string(),
            title: Some("non-empty-invariant".to_string()),
        });
    }

    lint_body(path, &tokens, &body, &mut diagnostics);
    finalize_metadata(diagnostics, false)
}

/// Executes the pure `validate` domain stage over `caseload.discovery.candidate_canons` against an in-memory [`WorkspaceSnapshot`].
pub fn execute_validate(
    options: &ValidateOptions,
    caseload: Caseload,
    workspace: &WorkspaceSnapshot,
) -> Result<Caseload, ValidateError> {
    if caseload.validation.is_some() && options.max_warnings.is_none() {
        return Ok(caseload);
    }

    let discovery = caseload
        .discovery
        .as_ref()
        .ok_or(ValidateError::MissingDiscovery)?;

    let duplicate_id_map = build_scope_id_collision_map(&workspace.canons);

    let mut raw_results: BTreeMap<String, Vec<CodeAnnotation>> = BTreeMap::new();
    let mut total_warnings = 0usize;
    let mut any_errors = false;

    for canon_ref in &discovery.candidate_canons {
        let mut diagnostics = if let Some(canon) = workspace.canons.get(canon_ref) {
            let parse_opts = ParseCanonOptions {
                source_ref: Some(canon_ref.clone()),
                scope: canon.scope.clone(),
            };
            let mut meta = lint_canon_source(&canon.raw_content, &parse_opts);
            let scope_key = (canon.scope.clone(), canon.id.clone());
            if let Some(colliding_paths) = duplicate_id_map.get(&scope_key)
                && colliding_paths.len() > 1
            {
                let others: Vec<&str> = colliding_paths
                    .iter()
                    .map(String::as_str)
                    .filter(|&p| p != canon_ref.as_str())
                    .collect();
                meta.diagnostics.push(CodeAnnotation {
                    path: canon_ref.clone(),
                    start_line: 1,
                    end_line: 1,
                    start_column: Some(1),
                    end_column: Some(1),
                    annotation_level: AnnotationLevel::Failure,
                    message: format!(
                        "Duplicate canon id '{}' within scope; conflicts with: {}.",
                        canon.id,
                        others.join(", ")
                    ),
                    title: Some("duplicate-canon-id".to_string()),
                });
            }
            meta.diagnostics
        } else {
            vec![CodeAnnotation {
                path: canon_ref.clone(),
                start_line: 1,
                end_line: 1,
                start_column: Some(1),
                end_column: Some(1),
                annotation_level: AnnotationLevel::Failure,
                message: format!("Candidate canon file not found in workspace: '{canon_ref}'."),
                title: Some("file-not-found".to_string()),
            }]
        };

        sort_diagnostics(&mut diagnostics);
        for d in &diagnostics {
            match d.annotation_level {
                AnnotationLevel::Failure => any_errors = true,
                AnnotationLevel::Warning => total_warnings += 1,
                AnnotationLevel::Notice => {}
            }
        }
        raw_results.insert(canon_ref.clone(), diagnostics);
    }

    let warnings_exceeded = options.max_warnings.is_some_and(|max| total_warnings > max);
    let has_errors = any_errors || warnings_exceeded;

    let results = raw_results
        .into_iter()
        .map(|(canon_ref, diagnostics)| {
            let meta = finalize_metadata(diagnostics, warnings_exceeded);
            (canon_ref, meta)
        })
        .collect();

    let validation = CaseloadValidation {
        results,
        has_errors,
    };

    Ok(caseload.with_validation(validation))
}

/// Materializes candidate canons from `workspace_root` on disk and executes [`execute_validate`].
pub fn execute_validate_on_disk(
    options: &ValidateOptions,
    caseload: Caseload,
    workspace_root: &Path,
) -> Result<Caseload, ValidateError> {
    if caseload.validation.is_some() && options.max_warnings.is_none() {
        return Ok(caseload);
    }

    let discovery = caseload
        .discovery
        .as_ref()
        .ok_or(ValidateError::MissingDiscovery)?;

    let mut snapshot = WorkspaceSnapshot::from_disk(workspace_root, &[])?;

    // Ensure any explicit candidate canon path present on disk is loaded even if outside default globs
    for canon_ref in &discovery.candidate_canons {
        if !snapshot.canons.contains_key(canon_ref) {
            let full_path = workspace_root.join(canon_ref);
            if full_path.is_file()
                && let Ok(raw_content) = fs::read_to_string(&full_path)
            {
                let opts = ParseCanonOptions::with_source_ref(canon_ref.clone());
                snapshot = snapshot.with_canon_markdown(&raw_content, &opts);
            }
        }
    }

    execute_validate(options, caseload, &snapshot)
}

fn build_scope_id_collision_map(
    canons: &BTreeMap<String, Canon>,
) -> BTreeMap<(Option<String>, String), Vec<String>> {
    let mut map: BTreeMap<(Option<String>, String), Vec<String>> = BTreeMap::new();
    for (path, canon) in canons {
        map.entry((canon.scope.clone(), canon.id.clone()))
            .or_default()
            .push(path.clone());
    }
    map
}

fn sort_diagnostics(diagnostics: &mut [CodeAnnotation]) {
    diagnostics.sort_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then_with(|| {
                a.start_column
                    .unwrap_or(0)
                    .cmp(&b.start_column.unwrap_or(0))
            })
            .then_with(|| {
                a.title
                    .as_deref()
                    .unwrap_or("")
                    .cmp(b.title.as_deref().unwrap_or(""))
            })
            .then_with(|| a.message.cmp(&b.message))
    });
}

fn finalize_metadata(
    mut diagnostics: Vec<CodeAnnotation>,
    warnings_exceeded: bool,
) -> ValidatedCanonMetadata {
    sort_diagnostics(&mut diagnostics);
    let mut warning_count = 0usize;
    let mut error_count = 0usize;

    for diag in &diagnostics {
        match diag.annotation_level {
            AnnotationLevel::Failure => error_count += 1,
            AnnotationLevel::Warning => warning_count += 1,
            AnnotationLevel::Notice => {}
        }
    }

    let failed = error_count > 0 || (warnings_exceeded && warning_count > 0);
    let result = if failed {
        ValidationOutcome::Fail
    } else {
        ValidationOutcome::Pass
    };

    ValidatedCanonMetadata {
        result,
        warning_count,
        error_count,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{ActiveExhibits, CaseloadDiscovery, DiscoveryMode};
    use std::path::PathBuf;

    mod frontmatter_linting {
        use super::*;

        #[test]
        fn test_valid_canon_passes_with_zero_diagnostics() {
            let raw = "\
---
triggers:
  - \"src/**/*.rs\"
inspect:
  - diff
tags:
  - rust
---
# Modules Must Document Public Items

All public items MUST include rustdoc comments.

Rationale: Documentation prevents API misuse.

**Remediation:** Add `///` doc comments to exported functions and types.";

            let meta = lint_canon_source(
                raw,
                &ParseCanonOptions::with_source_ref(
                    ".canons/rust/modules-must-document-public-items.md",
                ),
            );
            assert_eq!(meta.result, ValidationOutcome::Pass);
            assert_eq!(meta.error_count, 0);
            assert_eq!(meta.warning_count, 0);
            assert!(meta.diagnostics.is_empty());
        }

        #[test]
        fn test_flags_malformed_yaml_and_unclosed_frontmatter() {
            let unclosed = "---\ntriggers:\n  - \"src/**\"\n# Missing closing delimiter";
            let meta_unclosed = lint_canon_source(
                unclosed,
                &ParseCanonOptions::with_source_ref(".canons/rule-must-work.md"),
            );
            assert_eq!(meta_unclosed.result, ValidationOutcome::Fail);
            assert_eq!(meta_unclosed.error_count, 1);
            assert_eq!(
                meta_unclosed.diagnostics[0].title.as_deref(),
                Some("valid-yaml-frontmatter")
            );
            assert_eq!(meta_unclosed.diagnostics[0].start_line, 1);

            let bad_yaml = "---\ntriggers: [unclosed\n---\nItems MUST pass.";
            let meta_bad = lint_canon_source(
                bad_yaml,
                &ParseCanonOptions::with_source_ref(".canons/rule-must-work.md"),
            );
            assert_eq!(meta_bad.result, ValidationOutcome::Fail);
            assert_eq!(
                meta_bad.diagnostics[0].title.as_deref(),
                Some("valid-yaml-frontmatter")
            );
        }

        #[test]
        fn test_flags_unrecognized_keys_and_invalid_types() {
            let raw = "\
---
id: 123
unknown_key: true
inspect:
  - invalid_plane
exists: 42
---
Items MUST be valid.";

            let meta = lint_canon_source(
                raw,
                &ParseCanonOptions::with_source_ref(".canons/items-must-be-valid.md"),
            );
            assert_eq!(meta.result, ValidationOutcome::Fail);
            assert_eq!(meta.error_count, 3); // id type, inspect token, exists type
            assert_eq!(meta.warning_count, 1); // unknown_key
        }

        #[test]
        fn test_flags_scope_containment_and_exists_glob_errors() {
            let raw = "\
---
triggers:
  - \"../outside.rs\"
exists:
  - \"src/[unclosed.rs\"
---
Scoped items MUST stay inside scope.";

            let meta = lint_canon_source(
                raw,
                &ParseCanonOptions::with_source_ref(
                    "packages/ui/.canons/scoped-items-must-stay.md",
                ),
            );
            assert_eq!(meta.result, ValidationOutcome::Fail);
            assert_eq!(meta.error_count, 2);
            let titles: Vec<&str> = meta
                .diagnostics
                .iter()
                .filter_map(|d| d.title.as_deref())
                .collect();
            assert!(titles.contains(&"scope-containment"));
            assert!(titles.contains(&"exists-patterns"));
        }

        #[test]
        fn test_flags_negated_file_stems_and_divergent_ids() {
            let raw = "\
---
id: custom-must-not-diverge
---
Items MUST omit negated modal stems.";

            let meta = lint_canon_source(
                raw,
                &ParseCanonOptions::with_source_ref(".canons/items-must-not-diverge.md"),
            );
            assert_eq!(meta.error_count, 0);
            assert_eq!(meta.warning_count, 3); // no-negated-file-stems, id-matches-filename, no-negated-ids
        }
    }

    mod body_and_tetrad_linting {
        use super::*;

        #[test]
        fn test_flags_lowercase_rfc2119_keywords_outside_inline_code() {
            let raw = "\
---
triggers:
  - \"src/**/*.rs\"
---
Functions `must` obey invariants, and callers must validate inputs.";

            let meta = lint_canon_source(
                raw,
                &ParseCanonOptions::with_source_ref(".canons/callers-must-validate-inputs.md"),
            );
            assert_eq!(meta.result, ValidationOutcome::Fail);
            assert_eq!(meta.error_count, 1);
            let diag = &meta.diagnostics[0];
            assert_eq!(diag.title.as_deref(), Some("RFC 2119 Violation"));
            assert_eq!(diag.start_line, 5);
            assert_eq!(diag.start_column, Some(47));
            assert_eq!(
                diag.message,
                "Normative keyword 'must' must be uppercase 'MUST'."
            );
        }

        #[test]
        fn test_flags_blank_and_duplicate_single_occurrence_directives() {
            let raw = "\
Items MUST be valid.

Exception:   

Rationale: First reason.

Rationale: Second reason.";

            let meta = lint_canon_source(
                raw,
                &ParseCanonOptions::with_source_ref(".canons/items-must-be-valid.md"),
            );
            assert_eq!(meta.error_count, 0);
            assert_eq!(meta.warning_count, 2); // blank Exception + duplicate Rationale
        }

        #[test]
        fn test_flags_multi_paragraph_non_atomic_invariant() {
            let raw = "\
First rule paragraph MUST hold.

Second rule paragraph MUST also hold.

Rationale: Two rules in one canon.";

            let meta = lint_canon_source(
                raw,
                &ParseCanonOptions::with_source_ref(".canons/items-must-hold.md"),
            );
            assert_eq!(meta.error_count, 0);
            assert!(
                meta.diagnostics
                    .iter()
                    .any(|d| d.title.as_deref() == Some("rule-atomicity"))
            );
        }
    }

    mod caseload_stage_and_repository_corpus {
        use super::*;

        #[test]
        fn test_detects_duplicate_canon_ids_in_same_scope() {
            let ws = WorkspaceSnapshot::new()
                .with_canon_markdown(
                    "---\nid: shared-id\n---\nFirst rule MUST hold.",
                    &ParseCanonOptions::with_source_ref(".canons/a/first-must-hold.md"),
                )
                .with_canon_markdown(
                    "---\nid: shared-id\n---\nSecond rule MUST hold.",
                    &ParseCanonOptions::with_source_ref(".canons/b/second-must-hold.md"),
                );

            let caseload = Caseload::new().with_discovery(CaseloadDiscovery {
                mode: DiscoveryMode::AllCanons,
                target_files: Vec::new(),
                candidate_canons: vec![
                    ".canons/a/first-must-hold.md".to_string(),
                    ".canons/b/second-must-hold.md".to_string(),
                ],
                active_exhibits: ActiveExhibits::default(),
                triggers_join: BTreeMap::new(),
            });

            let enriched =
                execute_validate(&ValidateOptions::default(), caseload, &ws).expect("validate");
            let val = enriched.validation.expect("validation present");
            assert!(val.has_errors);
            let first_meta = val.results.get(".canons/a/first-must-hold.md").unwrap();
            assert_eq!(first_meta.result, ValidationOutcome::Fail);
            assert!(
                first_meta
                    .diagnostics
                    .iter()
                    .any(|d| d.title.as_deref() == Some("duplicate-canon-id"))
            );
        }

        #[test]
        fn test_max_warnings_threshold_trips_has_errors() {
            let ws = WorkspaceSnapshot::new().with_canon_markdown(
                "---\nunknown_field: 1\n---\nRule MUST hold.",
                &ParseCanonOptions::with_source_ref(".canons/rule-must-hold.md"),
            );
            let caseload = Caseload::new().with_discovery(CaseloadDiscovery {
                mode: DiscoveryMode::AllCanons,
                target_files: Vec::new(),
                candidate_canons: vec![".canons/rule-must-hold.md".to_string()],
                active_exhibits: ActiveExhibits::default(),
                triggers_join: BTreeMap::new(),
            });

            let pass_caseload = execute_validate(
                &ValidateOptions {
                    max_warnings: Some(1),
                },
                caseload.clone(),
                &ws,
            )
            .expect("validate max_warnings 1");
            assert!(!pass_caseload.validation.unwrap().has_errors);

            let fail_caseload = execute_validate(
                &ValidateOptions {
                    max_warnings: Some(0),
                },
                caseload,
                &ws,
            )
            .expect("validate max_warnings 0");
            let val = fail_caseload.validation.unwrap();
            assert!(val.has_errors);
            assert_eq!(
                val.results.get(".canons/rule-must-hold.md").unwrap().result,
                ValidationOutcome::Fail
            );
        }

        #[test]
        fn test_all_repository_canons_pass_validation_with_zero_errors() {
            let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let snapshot = WorkspaceSnapshot::from_disk(&repo_root, &[]).expect("load repo canons");
            let candidate_canons: Vec<String> = snapshot.canons.keys().cloned().collect();
            assert!(
                !candidate_canons.is_empty(),
                "repository must contain canons"
            );

            let caseload = Caseload::new().with_discovery(CaseloadDiscovery {
                mode: DiscoveryMode::AllCanons,
                target_files: Vec::new(),
                candidate_canons,
                active_exhibits: ActiveExhibits::default(),
                triggers_join: BTreeMap::new(),
            });

            let enriched = execute_validate(
                &ValidateOptions {
                    max_warnings: Some(0),
                },
                caseload,
                &snapshot,
            )
            .expect("validate repository corpus");
            let val = enriched.validation.expect("validation populated");

            for (path, meta) in &val.results {
                assert_eq!(
                    meta.error_count, 0,
                    "Canon {path} had unexpected errors: {:?}",
                    meta.diagnostics
                );
                assert_eq!(
                    meta.warning_count, 0,
                    "Canon {path} had unexpected warnings: {:?}",
                    meta.diagnostics
                );
            }
            assert!(!val.has_errors);
        }
    }
}
