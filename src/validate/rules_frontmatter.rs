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

//! Static frontmatter lint rules for Canon Markdown files (`docs/architecture/nodes/validate.md` §4).

use crate::canon::{
    CanonParseError, CanonSourceInfo, ExtractedFrontmatter, FrontmatterKeySpan, InspectToken,
    SourceSpan, has_directory_traversal,
};
use crate::caseload::{AnnotationLevel, CodeAnnotation};
use crate::discover::GlobPattern;
use std::collections::BTreeSet;

const RECOGNIZED_KEYS: &[&str] = &[
    "id",
    "title",
    "triggers",
    "exists",
    "inspect",
    "tags",
    "references",
];

const ARRAY_STRING_KEYS: &[&str] = &["triggers", "inspect", "tags", "references"];
const SCOPED_PATH_KEYS: &[&str] = &["triggers", "exists", "references"];
const NEGATED_MODAL_SEGMENTS: &[&str] = &["must-not", "should-not", "shall-not", "cannot"];

/// Converts a Stage 1 or frontmatter [`CanonParseError`] into a `valid-yaml-frontmatter` [`CodeAnnotation`].
pub(crate) fn annotation_from_parse_error(path: &str, err: &CanonParseError) -> CodeAnnotation {
    let span = err.span.unwrap_or(SourceSpan {
        start_line: 1,
        start_column: 1,
        end_line: 1,
        end_column: 1,
    });
    CodeAnnotation {
        path: path.to_string(),
        start_line: span.start_line,
        end_line: span.end_line,
        start_column: Some(span.start_column),
        end_column: Some(span.end_column),
        annotation_level: AnnotationLevel::Failure,
        message: err.message.clone(),
        title: Some("valid-yaml-frontmatter".to_string()),
    }
}

/// Evaluates all static frontmatter lint rules against an [`ExtractedFrontmatter`] AST.
pub(crate) fn lint_frontmatter(
    path: &str,
    source_info: Option<&CanonSourceInfo>,
    scope: Option<&str>,
    extracted: &ExtractedFrontmatter,
    out: &mut Vec<CodeAnnotation>,
) {
    check_negated_file_stem(path, source_info, out);

    if extracted.key_spans.is_empty() {
        return;
    }

    let mut seen_keys = BTreeSet::new();
    for entry in &extracted.key_spans {
        if !seen_keys.insert(entry.key.as_str()) {
            out.push(make_annotation(
                path,
                entry.key_span,
                AnnotationLevel::Warning,
                "duplicate-frontmatter-keys",
                format!(
                    "Duplicate frontmatter key '{}'; later occurrence overrides earlier value.",
                    entry.key
                ),
            ));
        }

        if !RECOGNIZED_KEYS.contains(&entry.key.as_str()) {
            out.push(make_annotation(
                path,
                entry.key_span,
                AnnotationLevel::Warning,
                "no-unrecognized-keys",
                format!(
                    "Unrecognized frontmatter key '{}'. Allowed keys are: id, title, triggers, exists, inspect, tags, references.",
                    entry.key
                ),
            ));
            continue;
        }

        check_entry_types(path, entry, out);
        check_id_conventions(path, source_info, entry, out);
        check_scope_and_globs(path, scope, entry, out);
    }
}

fn check_negated_file_stem(
    path: &str,
    source_info: Option<&CanonSourceInfo>,
    out: &mut Vec<CodeAnnotation>,
) {
    let Some(info) = source_info else {
        return;
    };
    if let Some(phrase) = find_negated_modal_phrase(&info.file_stem) {
        out.push(CodeAnnotation {
            path: path.to_string(),
            start_line: 1,
            end_line: 1,
            start_column: Some(1),
            end_column: Some(1),
            annotation_level: AnnotationLevel::Warning,
            message: format!(
                "Canon file stem '{}' contains negative modal phrase '{phrase}'. Rephrase affirmatively or categorically.",
                info.file_stem
            ),
            title: Some("no-negated-file-stems".to_string()),
        });
    }
}

fn check_entry_types(path: &str, entry: &FrontmatterKeySpan, out: &mut Vec<CodeAnnotation>) {
    let key = entry.key.as_str();
    if key == "id" || key == "title" {
        if !entry.raw_value.is_string() {
            out.push(make_annotation(
                path,
                entry.value_span,
                AnnotationLevel::Failure,
                "valid-frontmatter-types",
                format!("Frontmatter property '{key}' must be a string."),
            ));
        }
        return;
    }

    if key == "exists" {
        check_exists_type(path, entry, out);
        return;
    }

    if ARRAY_STRING_KEYS.contains(&key) {
        check_array_string_type(path, entry, out);
    }
}

fn check_exists_type(path: &str, entry: &FrontmatterKeySpan, out: &mut Vec<CodeAnnotation>) {
    match &entry.raw_value {
        serde_yaml::Value::String(_) => {}
        serde_yaml::Value::Sequence(seq) => {
            for (idx, item) in seq.iter().enumerate() {
                if !item.is_string() {
                    let span = item_or_value_span(entry, idx);
                    out.push(make_annotation(
                        path,
                        span,
                        AnnotationLevel::Failure,
                        "valid-frontmatter-types",
                        "Frontmatter property 'exists' items must be strings.".to_string(),
                    ));
                }
            }
        }
        _ => {
            out.push(make_annotation(
                path,
                entry.value_span,
                AnnotationLevel::Failure,
                "valid-frontmatter-types",
                "Frontmatter property 'exists' must be a string or an array of strings."
                    .to_string(),
            ));
        }
    }
}

fn check_array_string_type(path: &str, entry: &FrontmatterKeySpan, out: &mut Vec<CodeAnnotation>) {
    let key = entry.key.as_str();
    let serde_yaml::Value::Sequence(seq) = &entry.raw_value else {
        out.push(make_annotation(
            path,
            entry.value_span,
            AnnotationLevel::Failure,
            "valid-frontmatter-types",
            format!("Frontmatter property '{key}' must be an array of strings."),
        ));
        return;
    };

    for (idx, item) in seq.iter().enumerate() {
        let span = item_or_value_span(entry, idx);
        let Some(text) = item.as_str() else {
            out.push(make_annotation(
                path,
                span,
                AnnotationLevel::Failure,
                "valid-frontmatter-types",
                format!("Frontmatter property '{key}' items must be strings."),
            ));
            continue;
        };

        if key == "inspect" {
            let bare = text.strip_suffix('?').unwrap_or(text);
            if InspectToken::parse_token(bare).is_none() {
                out.push(make_annotation(
                    path,
                    span,
                    AnnotationLevel::Failure,
                    "valid-frontmatter-types",
                    format!(
                        "Invalid inspect token '{text}'. Allowed tokens are: diff, pr_title, pr_body, commit_messages, linked_issues."
                    ),
                ));
            }
        }
    }
}

fn check_id_conventions(
    path: &str,
    source_info: Option<&CanonSourceInfo>,
    entry: &FrontmatterKeySpan,
    out: &mut Vec<CodeAnnotation>,
) {
    if entry.key != "id" {
        return;
    }
    let Some(raw_id) = entry.raw_value.as_str().map(str::trim) else {
        return;
    };
    if raw_id.is_empty() {
        return;
    }

    if let Some(info) = source_info
        && !info.file_stem.is_empty()
        && raw_id != info.file_stem
    {
        out.push(make_annotation(
            path,
            entry.value_span,
            AnnotationLevel::Warning,
            "id-matches-filename",
            format!(
                "Frontmatter id '{raw_id}' diverges from file stem '{}'.",
                info.file_stem
            ),
        ));
    }

    if let Some(phrase) = find_negated_modal_phrase(raw_id) {
        out.push(make_annotation(
            path,
            entry.value_span,
            AnnotationLevel::Warning,
            "no-negated-ids",
            format!(
                "Frontmatter id '{raw_id}' contains negative modal phrase '{phrase}'. Rephrase affirmatively or categorically."
            ),
        ));
    }
}

fn check_scope_and_globs(
    path: &str,
    scope: Option<&str>,
    entry: &FrontmatterKeySpan,
    out: &mut Vec<CodeAnnotation>,
) {
    let key = entry.key.as_str();
    if !SCOPED_PATH_KEYS.contains(&key) {
        return;
    }

    match &entry.raw_value {
        serde_yaml::Value::String(pattern) => {
            validate_single_pattern(path, scope, entry, pattern, entry.value_span, out);
        }
        serde_yaml::Value::Sequence(seq) => {
            for (idx, item) in seq.iter().enumerate() {
                if let Some(pattern) = item.as_str() {
                    let span = item_or_value_span(entry, idx);
                    validate_single_pattern(path, scope, entry, pattern, span, out);
                }
            }
        }
        _ => {}
    }
}

fn validate_single_pattern(
    path: &str,
    scope: Option<&str>,
    entry: &FrontmatterKeySpan,
    pattern: &str,
    span: SourceSpan,
    out: &mut Vec<CodeAnnotation>,
) {
    let key = entry.key.as_str();
    if let Some(sc) = scope
        && has_directory_traversal(pattern)
    {
        out.push(make_annotation(
            path,
            span,
            AnnotationLevel::Failure,
            "scope-containment",
            format!(
                "Scoped canon in '{sc}' cannot reference paths outside its scope: '{pattern}'."
            ),
        ));
    }

    if let Err(err) = GlobPattern::parse(pattern) {
        let rule_id = if key == "exists" {
            "exists-patterns"
        } else {
            "glob-syntax"
        };
        out.push(make_annotation(
            path,
            span,
            AnnotationLevel::Failure,
            rule_id,
            format!(
                "Invalid glob pattern '{pattern}' in '{key}': {}.",
                err.message
            ),
        ));
    }
}

fn find_negated_modal_phrase(text: &str) -> Option<&'static str> {
    let normalized = text.to_ascii_lowercase().replace(['_', ' '], "-");
    for &phrase in NEGATED_MODAL_SEGMENTS {
        if normalized == phrase
            || normalized.starts_with(&format!("{phrase}-"))
            || normalized.ends_with(&format!("-{phrase}"))
            || normalized.contains(&format!("-{phrase}-"))
        {
            return Some(phrase);
        }
    }
    None
}

fn item_or_value_span(entry: &FrontmatterKeySpan, index: usize) -> SourceSpan {
    entry
        .item_spans
        .get(index)
        .copied()
        .unwrap_or(entry.value_span)
}

fn make_annotation(
    path: &str,
    span: SourceSpan,
    level: AnnotationLevel,
    title: &str,
    message: String,
) -> CodeAnnotation {
    CodeAnnotation {
        path: path.to_string(),
        start_line: span.start_line,
        end_line: span.end_line,
        start_column: Some(span.start_column),
        end_column: Some(span.end_column),
        annotation_level: level,
        message,
        title: Some(title.to_string()),
    }
}
