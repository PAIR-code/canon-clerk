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

//! Static Canon Anatomy directive lint rules (`blank-directive`, `duplicate-directive`,
//! `valid-directive-casing`, `no-deprecated-directive`) (`docs/architecture/nodes/validate.md` §4).

use crate::canon::{CanonBody, CanonToken, CanonTokenKind, DirectiveName, SourceSpan};
use crate::caseload::{AnnotationLevel, CodeAnnotation};

/// Evaluates all static directive occurrence, casing, and deprecation lint rules.
pub(crate) fn check_directives(
    path: &str,
    tokens: &[CanonToken],
    body: &CanonBody,
    out: &mut Vec<CodeAnnotation>,
) {
    check_directive_occurrences(path, body, out);
    check_deprecated_directives(path, tokens, out);
}

fn check_directive_occurrences(path: &str, body: &CanonBody, out: &mut Vec<CodeAnnotation>) {
    let mut seen_precedent = false;
    let mut seen_rationale = false;
    let mut seen_remediation = false;

    for occ in &body.directive_spans {
        check_directive_label_casing(path, occ.name, &occ.label, occ.span, out);

        if occ.content.trim().is_empty() {
            out.push(make_directive_warning(
                path,
                occ.span,
                "blank-directive",
                format!(
                    "Directive '{}' has blank or whitespace-only content and will be omitted.",
                    occ.label
                ),
            ));
        }

        match occ.name {
            DirectiveName::Precedent => {
                if seen_precedent {
                    out.push(make_directive_warning(
                        path,
                        occ.span,
                        "duplicate-directive",
                        "Duplicate 'Precedent' directive; single-occurrence directives should appear at most once per canon.".to_string(),
                    ));
                }
                seen_precedent = true;
            }
            DirectiveName::Rationale => {
                if seen_rationale {
                    out.push(make_directive_warning(
                        path,
                        occ.span,
                        "duplicate-directive",
                        "Duplicate 'Rationale' directive; single-occurrence directives should appear at most once per canon.".to_string(),
                    ));
                }
                seen_rationale = true;
            }
            DirectiveName::Remediation => {
                if seen_remediation {
                    out.push(make_directive_warning(
                        path,
                        occ.span,
                        "duplicate-directive",
                        "Duplicate 'Remediation' directive; single-occurrence directives should appear at most once per canon.".to_string(),
                    ));
                }
                seen_remediation = true;
            }
            DirectiveName::Exception => {}
        }
    }
}

fn check_directive_label_casing(
    path: &str,
    name: DirectiveName,
    label: &str,
    span: SourceSpan,
    out: &mut Vec<CodeAnnotation>,
) {
    let keyword = label
        .trim()
        .trim_start_matches('#')
        .trim()
        .trim_matches('*')
        .trim_end_matches(':')
        .trim_matches('*')
        .trim();

    let (valid, expected) = match name {
        DirectiveName::Exception => (
            keyword == "Exception" || keyword == "Exceptions",
            if keyword.eq_ignore_ascii_case("exceptions") {
                "Exceptions"
            } else {
                "Exception"
            },
        ),
        DirectiveName::Precedent => (keyword == "Precedent", "Precedent"),
        DirectiveName::Rationale => (keyword == "Rationale", "Rationale"),
        DirectiveName::Remediation => (keyword == "Remediation", "Remediation"),
    };

    if !valid && !keyword.is_empty() {
        let header_span = SourceSpan {
            start_line: span.start_line,
            start_column: span.start_column,
            end_line: span.start_line,
            end_column: span.start_column + label.chars().count().saturating_sub(1),
        };
        out.push(make_directive_warning(
            path,
            header_span,
            "valid-directive-casing",
            format!(
                "Directive '{keyword}' should use standard capitalized casing '{expected}' (SPEC.md §5.2)."
            ),
        ));
    }
}

fn check_deprecated_directives(path: &str, tokens: &[CanonToken], out: &mut Vec<CodeAnnotation>) {
    for token in tokens {
        let deprecated = match &token.kind {
            CanonTokenKind::Heading { text, .. } => match_deprecated_heading(text),
            CanonTokenKind::Text { .. } => match_deprecated_inline(&token.raw),
            _ => None,
        };

        if let Some((raw_word, canonical_name, remediation)) = deprecated {
            out.push(make_directive_warning(
                path,
                token.span,
                "no-deprecated-directive",
                format!(
                    "Deprecated directive '{raw_word}' ({canonical_name}) detected; {remediation}"
                ),
            ));
        }
    }
}

fn match_deprecated_heading(text: &str) -> Option<(&str, &'static str, &'static str)> {
    let trimmed = text.trim().trim_end_matches(':').trim();
    if trimmed.eq_ignore_ascii_case("guidance") {
        Some((
            trimmed,
            "Guidance",
            "rename to 'Remediation:' for contributor action steps or 'Rationale:' for explanatory reasoning.",
        ))
    } else if trimmed.eq_ignore_ascii_case("supplement") {
        Some((
            trimmed,
            "Supplement",
            "fold supplementary context into 'Rationale:' or frontmatter 'references:'.",
        ))
    } else {
        None
    }
}

fn match_deprecated_inline(line: &str) -> Option<(&str, &'static str, &'static str)> {
    let trimmed = line.trim();
    let (has_bold, after_open) = if let Some(rest) = trimmed.strip_prefix("**") {
        (true, rest)
    } else {
        (false, trimmed)
    };

    let lower = after_open.to_ascii_lowercase();
    let (kw_len, canonical_name, remediation) = if lower.starts_with("guidance") {
        (
            "guidance".len(),
            "Guidance",
            "rename to 'Remediation:' for contributor action steps or 'Rationale:' for explanatory reasoning.",
        )
    } else if lower.starts_with("supplement") {
        (
            "supplement".len(),
            "Supplement",
            "fold supplementary context into 'Rationale:' or frontmatter 'references:'.",
        )
    } else {
        return None;
    };

    let raw_word = &after_open[..kw_len];
    let mut cursor = &after_open[kw_len..];
    let mut saw_colon = false;

    if let Some(rest) = cursor.strip_prefix(':') {
        saw_colon = true;
        cursor = rest;
    }
    if has_bold {
        cursor = cursor.strip_prefix("**")?;
    }
    if let Some(rest) = cursor.strip_prefix(':') {
        saw_colon = true;
        cursor = rest;
    }

    if !saw_colon && !has_bold {
        return None;
    }
    if !cursor.is_empty() && !cursor.starts_with(char::is_whitespace) {
        return None;
    }

    Some((raw_word, canonical_name, remediation))
}

fn make_directive_warning(
    path: &str,
    span: SourceSpan,
    title: &str,
    message: String,
) -> CodeAnnotation {
    CodeAnnotation {
        path: path.to_string(),
        start_line: span.start_line,
        end_line: span.end_line,
        start_column: Some(span.start_column),
        end_column: Some(span.end_column),
        annotation_level: AnnotationLevel::Warning,
        message,
        title: Some(title.to_string()),
    }
}
