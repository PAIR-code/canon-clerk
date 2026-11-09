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

//! Static Markdown body and cognitive tetrad lint rules for Canon files (`docs/architecture/nodes/validate.md` §4).

use crate::canon::{CanonBody, CanonToken, CanonTokenKind, DirectiveName, SourceSpan};
use crate::caseload::{AnnotationLevel, CodeAnnotation};

const UPPERCASE_RFC2119_KEYWORDS: &[&str] = &[
    "MUST",
    "REQUIRED",
    "SHALL",
    "SHOULD",
    "RECOMMENDED",
    "MAY",
    "OPTIONAL",
];

const LOWERCASE_MODAL_VERBS: &[&str] = &["must", "should", "shall"];

/// Evaluates all static Markdown body and directive lint rules against tokenized Canon content.
pub(crate) fn lint_body(
    path: &str,
    tokens: &[CanonToken],
    body: &CanonBody,
    out: &mut Vec<CodeAnnotation>,
) {
    check_unclosed_code_blocks(path, tokens, out);
    check_invariant_and_exception_modals(path, tokens, body, out);
    check_directive_occurrences(path, body, out);
}

fn check_unclosed_code_blocks(path: &str, tokens: &[CanonToken], out: &mut Vec<CodeAnnotation>) {
    for token in tokens {
        if let CanonTokenKind::CodeBlock { closed: false, .. } = &token.kind {
            out.push(CodeAnnotation {
                path: path.to_string(),
                start_line: token.span.start_line,
                end_line: token.span.end_line,
                start_column: Some(token.span.start_column),
                end_column: Some(token.span.end_column),
                annotation_level: AnnotationLevel::Failure,
                message: "Unclosed fenced code block in canon body.".to_string(),
                title: Some("unclosed-code-block".to_string()),
            });
        }
    }
}

struct InvariantScanState {
    skipped_title: bool,
    active_directive: Option<DirectiveName>,
    in_prose_paragraph: bool,
    prose_paragraph_count: usize,
    has_uppercase_rfc2119: bool,
    has_lowercase_modal: bool,
}

fn check_invariant_and_exception_modals(
    path: &str,
    tokens: &[CanonToken],
    body: &CanonBody,
    out: &mut Vec<CodeAnnotation>,
) {
    let mut state = InvariantScanState {
        skipped_title: false,
        active_directive: None,
        in_prose_paragraph: false,
        prose_paragraph_count: 0,
        has_uppercase_rfc2119: false,
        has_lowercase_modal: false,
    };

    for token in tokens {
        scan_body_token(path, token, &mut state, out);
    }

    let Some(inv_span) = body.invariant_span else {
        return;
    };

    if !state.has_uppercase_rfc2119 && !state.has_lowercase_modal {
        out.push(CodeAnnotation {
            path: path.to_string(),
            start_line: inv_span.start_line,
            end_line: inv_span.end_line,
            start_column: Some(inv_span.start_column),
            end_column: Some(inv_span.end_column),
            annotation_level: AnnotationLevel::Warning,
            message: "Canon invariant should state a normative requirement using uppercase RFC 2119 keywords (e.g. 'MUST', 'SHOULD') or categorical prohibitions ('are forbidden').".to_string(),
            title: Some("RFC 2119 Violation".to_string()),
        });
    }

    if state.prose_paragraph_count > 1 {
        out.push(CodeAnnotation {
            path: path.to_string(),
            start_line: inv_span.start_line,
            end_line: inv_span.end_line,
            start_column: Some(inv_span.start_column),
            end_column: Some(inv_span.end_column),
            annotation_level: AnnotationLevel::Warning,
            message: "Canon invariant spans multiple paragraphs; each canon should address a single atomic invariant.".to_string(),
            title: Some("rule-atomicity".to_string()),
        });
    }
}

fn scan_body_token(
    path: &str,
    token: &CanonToken,
    state: &mut InvariantScanState,
    out: &mut Vec<CodeAnnotation>,
) {
    match &token.kind {
        CanonTokenKind::Frontmatter { .. } => {}
        CanonTokenKind::Heading { level, text } => {
            if !state.skipped_title && *level <= 2 {
                state.skipped_title = true;
                return;
            }
            if state.active_directive.is_none()
                && state.prose_paragraph_count == 0
                && *level == 2
                && text.eq_ignore_ascii_case("rule")
            {
                return;
            }
            state.in_prose_paragraph = false;
        }
        CanonTokenKind::Directive { name, label, .. } => {
            state.active_directive = Some(*name);
            state.in_prose_paragraph = false;
            if *name == DirectiveName::Exception {
                let masked = mask_directive_prefix_and_inline_code(&token.raw, label.len());
                scan_line_for_lowercase_modals(path, token.span.start_line, &masked, out);
            }
        }
        CanonTokenKind::BlankLine | CanonTokenKind::CodeBlock { .. } => {
            state.in_prose_paragraph = false;
        }
        CanonTokenKind::Text { content } => {
            if state.active_directive.is_none() {
                if !content.trim().is_empty() && !state.in_prose_paragraph {
                    state.in_prose_paragraph = true;
                    state.prose_paragraph_count += 1;
                }
                let masked = mask_inline_code(&token.raw);
                if contains_normative_anchor(&masked) {
                    state.has_uppercase_rfc2119 = true;
                }
                let found =
                    scan_line_for_lowercase_modals(path, token.span.start_line, &masked, out);
                if found {
                    state.has_lowercase_modal = true;
                }
            } else if state.active_directive == Some(DirectiveName::Exception) {
                let masked = mask_inline_code(&token.raw);
                scan_line_for_lowercase_modals(path, token.span.start_line, &masked, out);
            }
        }
    }
}

fn check_directive_occurrences(path: &str, body: &CanonBody, out: &mut Vec<CodeAnnotation>) {
    let mut seen_rationale = false;
    let mut seen_remediation = false;

    for occ in &body.directive_spans {
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

/// Replaces inline code spans (`` `...` ``) with spaces of identical character length so 1-based column positions remain exact.
fn mask_inline_code(line: &str) -> String {
    let mut chars: Vec<char> = line.chars().collect();
    let mut idx = 0;
    while idx < chars.len() {
        if chars[idx] == '`' {
            let start = idx;
            let mut fence_len = 0;
            while idx < chars.len() && chars[idx] == '`' {
                fence_len += 1;
                idx += 1;
            }
            let mut close_start = None;
            let mut probe = idx;
            while probe < chars.len() {
                if chars[probe] == '`' {
                    let run_start = probe;
                    let mut run_len = 0;
                    while probe < chars.len() && chars[probe] == '`' {
                        run_len += 1;
                        probe += 1;
                    }
                    if run_len == fence_len {
                        close_start = Some(run_start + run_len);
                        break;
                    }
                } else {
                    probe += 1;
                }
            }
            let end = close_start.unwrap_or(chars.len());
            for ch in &mut chars[start..end] {
                *ch = ' ';
            }
            idx = end;
        } else {
            idx += 1;
        }
    }
    chars.into_iter().collect()
}

fn mask_directive_prefix_and_inline_code(raw_line: &str, prefix_byte_len: usize) -> String {
    let prefix_chars = raw_line
        .get(..prefix_byte_len)
        .map_or(0, |s| s.chars().count());
    let masked = mask_inline_code(raw_line);
    masked
        .chars()
        .enumerate()
        .map(|(i, c)| if i < prefix_chars { ' ' } else { c })
        .collect()
}

fn contains_normative_anchor(masked_line: &str) -> bool {
    let words: Vec<&str> = masked_line
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| !w.is_empty())
        .collect();

    if words.iter().any(|w| UPPERCASE_RFC2119_KEYWORDS.contains(w)) {
        return true;
    }

    let lower = masked_line.to_ascii_lowercase();
    lower.contains("is forbidden")
        || lower.contains("are forbidden")
        || lower.contains("is prohibited")
        || lower.contains("are prohibited")
}

fn scan_line_for_lowercase_modals(
    path: &str,
    line_number: usize,
    masked_line: &str,
    out: &mut Vec<CodeAnnotation>,
) -> bool {
    let chars: Vec<char> = masked_line.chars().collect();
    let mut idx = 0;
    let mut found_any = false;

    while idx < chars.len() {
        if !chars[idx].is_ascii_alphabetic() {
            idx += 1;
            continue;
        }
        let start = idx;
        while idx < chars.len() && chars[idx].is_ascii_alphabetic() {
            idx += 1;
        }
        let word: String = chars[start..idx].iter().collect();
        if LOWERCASE_MODAL_VERBS.contains(&word.as_str()) {
            found_any = true;
            let start_col = start + 1;
            let end_col = idx;
            out.push(CodeAnnotation {
                path: path.to_string(),
                start_line: line_number,
                end_line: line_number,
                start_column: Some(start_col),
                end_column: Some(end_col),
                annotation_level: AnnotationLevel::Failure,
                message: format!(
                    "Normative keyword '{word}' must be uppercase '{}'.",
                    word.to_ascii_uppercase()
                ),
                title: Some("RFC 2119 Violation".to_string()),
            });
        }
    }

    found_any
}
