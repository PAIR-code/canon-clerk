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

//! Stage 2 semantic parser for the Canon body cognitive tetrad (`SPEC.md` §5).

use super::lexer::{
    CanonParseError, CanonToken, CanonTokenKind, DirectiveName, SourceSpan,
    normalize_canon_content, tokenize_canon,
};
use serde::{Deserialize, Serialize};

/// Recorded occurrence of a cognitive directive in the Canon body with 1-based source coordinates.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DirectiveOccurrence {
    /// Normalized directive name (`exception`, `rationale`, or `remediation`).
    pub name: DirectiveName,
    /// Raw authored directive label (e.g. `"Exception:"`, `"**Remediation:**"`, `"## Exceptions"`).
    pub label: String,
    /// 1-based source coordinates covering the directive header and its body lines.
    pub span: SourceSpan,
    /// Trimmed text content of this directive occurrence (empty string if blank/whitespace-only).
    pub content: String,
}

/// Structured cognitive tetrad and raw body extracted from a Canon's Markdown body (`SPEC.md` §5).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CanonBody {
    /// Primary normative invariant statement (What).
    pub invariant: String,
    /// 1-based source coordinates of the invariant statement in the document, if present in body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invariant_span: Option<SourceSpan>,
    /// Discrete permissible deviation clauses evaluated as logical ORs (When / `Exception`).
    pub exceptions: Vec<String>,
    /// Precedent engineering rationale (Why / `Rationale`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    /// Actionable contributor remediation instructions (How / `Remediation`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    /// Ordered list of all directive occurrences (including blank or repeated directives) with spans.
    pub directive_spans: Vec<DirectiveOccurrence>,
    /// Full Markdown body with YAML frontmatter removed.
    pub raw_body: String,
}

struct ActiveDirective {
    name: DirectiveName,
    label: String,
    start_span: SourceSpan,
    end_span: SourceSpan,
    lines: Vec<String>,
}

impl ActiveDirective {
    fn into_occurrence(self) -> DirectiveOccurrence {
        let content = self.lines.join("\n").trim().to_string();
        DirectiveOccurrence {
            name: self.name,
            label: self.label,
            span: SourceSpan {
                start_line: self.start_span.start_line,
                start_column: self.start_span.start_column,
                end_line: self.end_span.end_line,
                end_column: self.end_span.end_column,
            },
            content,
        }
    }
}

fn expand_plural_exception_bullets(label: &str, content: &str) -> Vec<String> {
    let is_plural = label.to_ascii_lowercase().contains("exceptions");
    if !is_plural {
        return vec![content.to_string()];
    }

    let non_empty_lines: Vec<&str> = content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if non_empty_lines.is_empty() {
        return Vec::new();
    }

    let all_bullets = non_empty_lines
        .iter()
        .all(|l| l.starts_with("- ") || l.starts_with("* "));
    if all_bullets {
        non_empty_lines
            .into_iter()
            .map(|l| l[2..].trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        vec![content.to_string()]
    }
}

fn append_token_to_section(
    token: &CanonToken,
    text_line: String,
    active_directive: &mut Option<ActiveDirective>,
    invariant_lines: &mut Vec<String>,
    invariant_span: &mut Option<SourceSpan>,
) {
    if let Some(dir) = active_directive.as_mut() {
        if !matches!(token.kind, CanonTokenKind::BlankLine) {
            dir.end_span = token.span;
        }
        dir.lines.push(text_line);
    } else {
        if !matches!(token.kind, CanonTokenKind::BlankLine) {
            match invariant_span {
                Some(span) => {
                    span.end_line = token.span.end_line;
                    span.end_column = token.span.end_column;
                }
                None => *invariant_span = Some(token.span),
            }
        }
        invariant_lines.push(text_line);
    }
}

fn collect_directive_tetrad(
    occurrences: &[DirectiveOccurrence],
) -> (Vec<String>, Option<String>, Option<String>) {
    let mut exceptions = Vec::new();
    let mut rationale_parts = Vec::new();
    let mut remediation_parts = Vec::new();

    for occ in occurrences {
        if occ.content.is_empty() {
            continue;
        }
        match occ.name {
            DirectiveName::Exception => {
                exceptions.extend(expand_plural_exception_bullets(&occ.label, &occ.content));
            }
            DirectiveName::Rationale => rationale_parts.push(occ.content.as_str()),
            DirectiveName::Remediation => remediation_parts.push(occ.content.as_str()),
        }
    }

    let rationale = (!rationale_parts.is_empty()).then(|| rationale_parts.join("\n\n"));
    let remediation = (!remediation_parts.is_empty()).then(|| remediation_parts.join("\n\n"));
    (exceptions, rationale, remediation)
}

/// Stage 2 Body Parser: Assembles a [`CanonBody`] from a Stage 1 [`CanonToken`] slice.
pub fn parse_body_from_tokens(tokens: &[CanonToken]) -> CanonBody {
    let mut skipped_heading = false;
    let mut invariant_lines: Vec<String> = Vec::new();
    let mut invariant_span: Option<SourceSpan> = None;
    let mut active_directive: Option<ActiveDirective> = None;
    let mut occurrences: Vec<DirectiveOccurrence> = Vec::new();
    let mut raw_body_lines: Vec<&str> = Vec::new();

    for token in tokens {
        if matches!(token.kind, CanonTokenKind::Frontmatter { .. }) {
            continue;
        }
        raw_body_lines.push(token.raw.as_str());

        if let CanonTokenKind::Heading { level, text } = &token.kind {
            if !skipped_heading && *level <= 2 {
                skipped_heading = true;
                continue;
            }
            if active_directive.is_none()
                && invariant_lines.iter().all(|l| l.trim().is_empty())
                && *level == 2
                && text.eq_ignore_ascii_case("rule")
            {
                continue;
            }
        }

        match &token.kind {
            CanonTokenKind::Directive { name, label, value } => {
                if let Some(prev) = active_directive.take() {
                    occurrences.push(prev.into_occurrence());
                }
                let mut lines = Vec::new();
                if !value.is_empty() {
                    lines.push(value.clone());
                }
                active_directive = Some(ActiveDirective {
                    name: *name,
                    label: label.clone(),
                    start_span: token.span,
                    end_span: token.span,
                    lines,
                });
            }
            CanonTokenKind::CodeBlock { .. } | CanonTokenKind::Heading { .. } => {
                append_token_to_section(
                    token,
                    token.raw.clone(),
                    &mut active_directive,
                    &mut invariant_lines,
                    &mut invariant_span,
                );
            }
            CanonTokenKind::Text { content } => {
                append_token_to_section(
                    token,
                    content.clone(),
                    &mut active_directive,
                    &mut invariant_lines,
                    &mut invariant_span,
                );
            }
            CanonTokenKind::BlankLine => {
                append_token_to_section(
                    token,
                    String::new(),
                    &mut active_directive,
                    &mut invariant_lines,
                    &mut invariant_span,
                );
            }
            CanonTokenKind::Frontmatter { .. } => {}
        }
    }

    if let Some(last) = active_directive.take() {
        occurrences.push(last.into_occurrence());
    }

    let invariant = invariant_lines.join("\n").trim().to_string();
    if invariant.is_empty() {
        invariant_span = None;
    }

    let (exceptions, rationale, remediation) = collect_directive_tetrad(&occurrences);

    CanonBody {
        invariant,
        invariant_span,
        exceptions,
        rationale,
        remediation,
        directive_spans: occurrences,
        raw_body: raw_body_lines.join("\n"),
    }
}

/// Parses a Canon Markdown body string into structured cognitive directives (`SPEC.md` §5).
pub fn parse_body(raw_body: &str) -> Result<CanonBody, CanonParseError> {
    let normalized = normalize_canon_content(raw_body);
    let tokens = tokenize_canon(&normalized)?;
    let mut parsed = parse_body_from_tokens(&tokens);
    parsed.raw_body = normalized;
    Ok(parsed)
}
