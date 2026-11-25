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
mod rules_directives;
mod rules_frontmatter;
mod rules_globs;
mod rules_links;

use crate::canon::{
    Canon, CanonScope, CanonSourceKind, DEFAULT_FALLBACK_ID, ParseCanonOptions, derive_id,
    extract_frontmatter_from_tokens, has_directory_traversal, parse_body_from_tokens,
    resolve_source_info, tokenize_canon,
};
use crate::caseload::{
    AnnotationLevel, Caseload, CaseloadDiscovery, CaseloadValidation, CodeAnnotation,
    DiscoveryMode, ValidatedCanonMetadata, ValidationOutcome,
};
use crate::discover::{DiscoverError, GlobPattern, WorkspaceSnapshot, scope_relative_path};
use rules_body::lint_body;
use rules_frontmatter::{annotation_from_parse_error, lint_frontmatter};
use std::collections::BTreeMap;
use std::fmt;
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

    let references = extract_reference_patterns(extracted_opt.as_ref());
    lint_body(
        path,
        source_info.as_ref(),
        effective_scope.as_deref(),
        &references,
        &tokens,
        &body,
        &mut diagnostics,
    );
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
            let scope_key = (
                canon.source_kind.clone(),
                canon.scope.clone(),
                canon.id.clone(),
            );
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
            check_valid_reference_targets(canon_ref, canon, workspace, &mut meta.diagnostics);
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

    check_no_non_canon_files(discovery, workspace, &mut raw_results, &mut total_warnings);

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
    snapshot.backfill_inline_canons(&discovery.inline_canons);

    // Ensure any explicit candidate canon ref (builtin:* or workspace path) is loaded
    for canon_ref in &discovery.candidate_canons {
        snapshot.backfill_canon_ref(workspace_root, canon_ref);
    }

    execute_validate(options, caseload, &snapshot)
}

fn extract_reference_patterns(
    extracted: Option<&crate::canon::ExtractedFrontmatter>,
) -> Vec<String> {
    let Some(ext) = extracted else {
        return Vec::new();
    };
    let Some(val) = ext.frontmatter.get("references") else {
        return Vec::new();
    };
    match val {
        serde_yaml::Value::String(s) => vec![s.clone()],
        serde_yaml::Value::Sequence(seq) => seq
            .iter()
            .filter_map(|v| v.as_str().map(ToOwned::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

fn check_valid_reference_targets(
    canon_ref: &str,
    canon: &Canon,
    workspace: &WorkspaceSnapshot,
    out: &mut Vec<CodeAnnotation>,
) {
    if canon.source_kind != Some(CanonSourceKind::Workspace) {
        return;
    }

    let scope_obj = canon.scope.as_deref().map(|s| CanonScope(s.to_string()));
    for entry in &canon.frontmatter_key_spans {
        if entry.key != "references" {
            continue;
        }
        let items: Vec<(String, crate::canon::SourceSpan)> = match &entry.raw_value {
            serde_yaml::Value::String(s) => vec![(s.clone(), entry.value_span)],
            serde_yaml::Value::Sequence(seq) => seq
                .iter()
                .enumerate()
                .filter_map(|(idx, v)| {
                    let s = v.as_str()?.to_string();
                    let span = entry
                        .item_spans
                        .get(idx)
                        .copied()
                        .unwrap_or(entry.value_span);
                    Some((s, span))
                })
                .collect(),
            _ => Vec::new(),
        };

        for (pattern, span) in items {
            if canon.scope.is_some() && has_directory_traversal(&pattern) {
                continue;
            }
            let Ok(glob) = GlobPattern::parse(&pattern) else {
                continue;
            };
            let matched = workspace.existing_files.iter().any(|existing| {
                scope_relative_path(existing, scope_obj.as_ref())
                    .is_some_and(|rel| glob.is_match(&rel))
            });
            if !matched {
                out.push(CodeAnnotation {
                    path: canon_ref.to_string(),
                    start_line: span.start_line,
                    end_line: span.end_line,
                    start_column: Some(span.start_column),
                    end_column: Some(span.end_column),
                    annotation_level: AnnotationLevel::Warning,
                    message: format!(
                        "Reference pattern '{pattern}' matches zero files in the workspace (SPEC.md §4.2.7)."
                    ),
                    title: Some("valid-reference-targets".to_string()),
                });
            }
        }
    }
}

fn check_no_non_canon_files(
    discovery: &CaseloadDiscovery,
    workspace: &WorkspaceSnapshot,
    raw_results: &mut BTreeMap<String, Vec<CodeAnnotation>>,
    total_warnings: &mut usize,
) {
    for rel_path in &workspace.existing_files {
        let in_canons_dir = rel_path.starts_with(".canons/") || rel_path.contains("/.canons/");
        if !in_canons_dir {
            continue;
        }
        if rel_path.to_ascii_lowercase().ends_with(".md") {
            continue;
        }
        if rel_path == ".canons/config.yaml" || rel_path.ends_with("/.canons/config.yaml") {
            continue;
        }
        let should_report = discovery.mode == DiscoveryMode::AllCanons
            || discovery.target_files.iter().any(|t| t == rel_path);
        if !should_report {
            continue;
        }

        let diag = CodeAnnotation {
            path: rel_path.clone(),
            start_line: 1,
            end_line: 1,
            start_column: Some(1),
            end_column: Some(1),
            annotation_level: AnnotationLevel::Warning,
            message: format!(
                "Non-canon file '{rel_path}' resides under '.canons/'. Remove it or move it outside '.canons/' (only '.md' files and direct '.canons/config.yaml' policy files are permitted)."
            ),
            title: Some("no-non-canon-files".to_string()),
        };
        *total_warnings += 1;
        raw_results.entry(rel_path.clone()).or_default().push(diag);
    }
}

type ScopeIdKey = (Option<CanonSourceKind>, Option<String>, String);

fn build_scope_id_collision_map(
    canons: &BTreeMap<String, Canon>,
) -> BTreeMap<ScopeIdKey, Vec<String>> {
    let mut map: BTreeMap<ScopeIdKey, Vec<String>> = BTreeMap::new();
    for (path, canon) in canons {
        map.entry((
            canon.source_kind.clone(),
            canon.scope.clone(),
            canon.id.clone(),
        ))
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

        #[test]
        fn test_x_extension_fields_exempt_from_unrecognized_keys_but_checked_for_duplicates() {
            let valid_raw = "\
---
x-owner: \"@platform\"
x-presets:
  - recommended
triggers:
  - \"src/**/*.rs\"
---
Items MUST be valid.";

            let opts = ParseCanonOptions::with_source_ref(".canons/items-must-be-valid.canon.md");
            let meta = lint_canon_source(valid_raw, &opts);
            assert_eq!(meta.error_count, 0);
            assert_eq!(meta.warning_count, 0, "{:?}", meta.diagnostics);

            // Verify x-* keys are not leaked into parsed Canon normalized fields
            let parsed = crate::canon::parse_canon(valid_raw, &opts).expect("parse_canon");
            assert_eq!(parsed.id, "items-must-be-valid");
            assert!(parsed.tags.is_empty());

            // Bare `x-` and duplicate `x-*` must still warn
            let bad_raw = "\
---
x-: true
x-owner: \"@a\"
x-owner: \"@b\"
---
Items MUST be valid.";
            let bad_meta = lint_canon_source(bad_raw, &opts);
            let titles: Vec<&str> = bad_meta
                .diagnostics
                .iter()
                .filter_map(|d| d.title.as_deref())
                .collect();
            assert!(titles.contains(&"no-unrecognized-keys"), "{titles:?}");
            assert!(titles.contains(&"duplicate-frontmatter-keys"), "{titles:?}");
        }

        #[test]
        fn test_prefer_brace_alternation_and_glob_hygiene() {
            let uncollapsed = "\
---
triggers:
  - \"**/*.go\"
  - \"**/*.rs\"
  - \"**/*.ts\"
---
Items MUST be valid.";
            let meta = lint_canon_source(
                uncollapsed,
                &ParseCanonOptions::with_source_ref(".canons/items-must-be-valid.canon.md"),
            );
            assert!(
                meta.diagnostics.iter().any(|d| {
                    d.title.as_deref() == Some("prefer-brace-alternation")
                        && d.message.contains("**/*.{go,rs,ts}")
                }),
                "{:?}",
                meta.diagnostics
            );

            // Two literal filenames should NOT trigger prefer-brace-alternation
            let two_literals = "\
---
triggers:
  - \"README.md\"
  - \"SPEC.md\"
---
Items MUST be valid.";
            let two_meta = lint_canon_source(
                two_literals,
                &ParseCanonOptions::with_source_ref(".canons/items-must-be-valid.canon.md"),
            );
            assert_eq!(two_meta.warning_count, 0, "{:?}", two_meta.diagnostics);

            // Three literal sibling filenames SHOULD trigger prefer-brace-alternation
            let three_literals = "\
---
triggers:
  - \"AGENTS.md\"
  - \"README.md\"
  - \"SPEC.md\"
---
Items MUST be valid.";
            let three_meta = lint_canon_source(
                three_literals,
                &ParseCanonOptions::with_source_ref(".canons/items-must-be-valid.canon.md"),
            );
            assert!(
                three_meta.diagnostics.iter().any(|d| {
                    d.title.as_deref() == Some("prefer-brace-alternation")
                        && d.message.contains("{AGENTS,README,SPEC}.md")
                }),
                "{:?}",
                three_meta.diagnostics
            );

            // Duplicate (via brace expansion) and subsumed globs
            let dup_and_subsumed = "\
---
triggers:
  - \"**\"
  - \"**/*.ts\"
  - \"**/*.{ts,tsx}\"
---
Items MUST be valid.";
            let ds_meta = lint_canon_source(
                dup_and_subsumed,
                &ParseCanonOptions::with_source_ref(".canons/items-must-be-valid.canon.md"),
            );
            let titles: Vec<&str> = ds_meta
                .diagnostics
                .iter()
                .filter_map(|d| d.title.as_deref())
                .collect();
            assert!(titles.contains(&"no-duplicate-globs"), "{titles:?}");
            assert!(titles.contains(&"no-subsumed-globs"), "{titles:?}");
        }

        #[test]
        fn test_kebab_case_filenames() {
            let raw = "Items MUST be valid.";
            let bad_meta = lint_canon_source(
                raw,
                &ParseCanonOptions::with_source_ref(".canons/Bad_FileName.canon.md"),
            );
            assert!(
                bad_meta
                    .diagnostics
                    .iter()
                    .any(|d| d.title.as_deref() == Some("kebab-case-filenames")),
                "{:?}",
                bad_meta.diagnostics
            );

            let good_meta = lint_canon_source(
                raw,
                &ParseCanonOptions::with_source_ref(".canons/items-must-be-valid-1.canon.md"),
            );
            assert_eq!(good_meta.warning_count, 0, "{:?}", good_meta.diagnostics);
        }
    }

    mod body_and_anatomy_linting {
        use super::*;

        #[test]
        fn test_precedent_directive_after_invariant_passes_without_atomicity_warning() {
            let raw = "\
Boolean-returning functions MUST specify both `true` and `false` polarity in doc comments.

Precedent: Google Style Guides; Effective Java (Item 56).";

            let meta = lint_canon_source(
                raw,
                &ParseCanonOptions::with_source_ref(
                    ".canons/boolean-return-docs-must-specify-polarity.canon.md",
                ),
            );
            assert_eq!(meta.result, ValidationOutcome::Pass);
            assert_eq!(meta.error_count, 0);
            assert_eq!(meta.warning_count, 0);
        }

        #[test]
        fn test_flags_blank_and_duplicate_precedent_directives() {
            let raw = "\
Items MUST be valid.

Precedent:   

Precedent: RFC 2119.";

            let meta = lint_canon_source(
                raw,
                &ParseCanonOptions::with_source_ref(".canons/items-must-be-valid.canon.md"),
            );
            assert_eq!(meta.error_count, 0);
            assert_eq!(meta.warning_count, 2);
            assert!(
                meta.diagnostics
                    .iter()
                    .any(|d| d.title.as_deref() == Some("blank-directive"))
            );
            assert!(
                meta.diagnostics
                    .iter()
                    .any(|d| d.title.as_deref() == Some("duplicate-directive"))
            );
        }

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

        #[test]
        fn test_flags_directive_casing_and_deprecated_directives() {
            let raw = "\
Items MUST be valid.

exception: Legacy callers MAY bypass IFF documented.

Guidance: Use the standard helper.

## Supplement
Extra notes.

RATIONALE: Keep directives consistent.";

            let meta = lint_canon_source(
                raw,
                &ParseCanonOptions::with_source_ref(".canons/items-must-be-valid.canon.md"),
            );
            let titles: Vec<&str> = meta
                .diagnostics
                .iter()
                .filter_map(|d| d.title.as_deref())
                .collect();
            assert_eq!(
                titles
                    .iter()
                    .filter(|&&t| t == "valid-directive-casing")
                    .count(),
                2,
                "{titles:?}"
            );
            assert_eq!(
                titles
                    .iter()
                    .filter(|&&t| t == "no-deprecated-directive")
                    .count(),
                2,
                "{titles:?}"
            );
        }

        #[test]
        fn test_links_declared_in_references_flags_undeclared_and_ignores_external_or_code() {
            let raw = "\
---
references:
  - \"SPEC.md\"
---
Items MUST follow [SPEC](SPEC.md) and [architecture](docs/arch.md#overview).

See also [external](https://example.com), [anchor](#local), and `[inline](docs/ignored.md)`.

```markdown
[fenced](docs/fenced.md)
```";

            let meta = lint_canon_source(
                raw,
                &ParseCanonOptions::with_source_ref(".canons/items-must-follow-spec.canon.md"),
            );
            let link_diags: Vec<&CodeAnnotation> = meta
                .diagnostics
                .iter()
                .filter(|d| d.title.as_deref() == Some("links-declared-in-references"))
                .collect();
            assert_eq!(link_diags.len(), 1, "{link_diags:?}");
            assert!(link_diags[0].message.contains("docs/arch.md"));
        }
    }

    mod caseload_stage {
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
                config_files: Vec::new(),
                candidate_canons: vec![
                    ".canons/a/first-must-hold.md".to_string(),
                    ".canons/b/second-must-hold.md".to_string(),
                ],
                active_exhibits: ActiveExhibits::default(),
                triggers_join: BTreeMap::new(),
                ..CaseloadDiscovery::default()
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
                config_files: Vec::new(),
                candidate_canons: vec![".canons/rule-must-hold.md".to_string()],
                active_exhibits: ActiveExhibits::default(),
                triggers_join: BTreeMap::new(),
                ..CaseloadDiscovery::default()
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
        fn test_cross_realm_same_id_coexists_without_duplicate_canon_id_error() {
            let ws = WorkspaceSnapshot::new()
                .with_canon_markdown(
                    "---\ntriggers:\n  - \"src/**/*.rs\"\n---\nLocal CLI flags MUST use positive polarity.\n",
                    &ParseCanonOptions::with_source_ref(
                        ".canons/cli-flags-must-use-positive-polarity.md",
                    ),
                )
                .with_canon_markdown(
                    "---\ntriggers:\n  - \"src/**/*.rs\"\n---\nBuilt-in CLI flags MUST use positive polarity.\n",
                    &ParseCanonOptions::with_source_ref(
                        "builtin:cli/cli-flags-must-use-positive-polarity",
                    ),
                );
            let caseload = Caseload::new().with_discovery(CaseloadDiscovery {
                mode: DiscoveryMode::AllCanons,
                target_files: Vec::new(),
                config_files: Vec::new(),
                candidate_canons: vec![
                    ".canons/cli-flags-must-use-positive-polarity.md".to_string(),
                    "builtin:cli/cli-flags-must-use-positive-polarity".to_string(),
                ],
                active_exhibits: ActiveExhibits::default(),
                triggers_join: BTreeMap::new(),
                ..CaseloadDiscovery::default()
            });

            let validated =
                execute_validate(&ValidateOptions::default(), caseload, &ws).expect("validate");
            let val = validated.validation.expect("validation");
            assert!(
                !val.has_errors,
                "cross-realm same-id canons must coexist per SPEC.md §6.2: {:?}",
                val.results
            );
            assert_eq!(val.results.len(), 2);
        }

        #[test]
        fn test_directory_hygiene_and_valid_reference_targets_in_workspace_snapshot() {
            let ws = WorkspaceSnapshot::new()
                .with_file("SPEC.md")
                .with_file("packages/ui/README.md")
                .with_file(".canons/config.yaml")
                .with_file(".canons/README.md")
                .with_file(".canons/stray-notes.txt")
                .with_file(".canons/nested/config.yaml")
                .with_canon_markdown(
                    "---\nreferences:\n  - \"SPEC.md\"\n  - \"docs/missing.md\"\n---\nItems MUST stay grounded.",
                    &ParseCanonOptions::with_source_ref(".canons/items-must-stay-grounded.canon.md"),
                )
                .with_canon_markdown(
                    "---\nreferences:\n  - \"README.md\"\n  - \"missing-local.md\"\n---\nUI items MUST stay grounded.",
                    &ParseCanonOptions::with_source_ref(
                        "packages/ui/.canons/ui-items-must-stay-grounded.canon.md",
                    ),
                );

            let caseload = Caseload::new().with_discovery(CaseloadDiscovery {
                mode: DiscoveryMode::AllCanons,
                target_files: Vec::new(),
                config_files: vec![".canons/config.yaml".to_string()],
                candidate_canons: vec![
                    ".canons/items-must-stay-grounded.canon.md".to_string(),
                    "packages/ui/.canons/ui-items-must-stay-grounded.canon.md".to_string(),
                ],
                active_exhibits: ActiveExhibits::default(),
                triggers_join: BTreeMap::new(),
                ..CaseloadDiscovery::default()
            });

            let validated =
                execute_validate(&ValidateOptions::default(), caseload, &ws).expect("validate");
            let val = validated.validation.expect("validation");

            // Stray non-.md file and nested config.yaml are flagged; direct config.yaml and README.md are allowed
            assert!(val.results.contains_key(".canons/stray-notes.txt"));
            assert!(val.results.contains_key(".canons/nested/config.yaml"));
            assert!(!val.results.contains_key(".canons/config.yaml"));
            assert!(!val.results.contains_key(".canons/README.md"));

            let root_ref_diags: Vec<&CodeAnnotation> = val
                .results
                .get(".canons/items-must-stay-grounded.canon.md")
                .unwrap()
                .diagnostics
                .iter()
                .filter(|d| d.title.as_deref() == Some("valid-reference-targets"))
                .collect();
            assert_eq!(root_ref_diags.len(), 1, "{root_ref_diags:?}");
            assert!(root_ref_diags[0].message.contains("docs/missing.md"));

            let scoped_ref_diags: Vec<&CodeAnnotation> = val
                .results
                .get("packages/ui/.canons/ui-items-must-stay-grounded.canon.md")
                .unwrap()
                .diagnostics
                .iter()
                .filter(|d| d.title.as_deref() == Some("valid-reference-targets"))
                .collect();
            assert_eq!(scoped_ref_diags.len(), 1, "{scoped_ref_diags:?}");
            assert!(scoped_ref_diags[0].message.contains("missing-local.md"));
        }
    }
}
