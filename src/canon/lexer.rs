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

//! Stage 1 lexical scanner for Canon Markdown and YAML frontmatter files.

use serde::{Deserialize, Serialize};
use std::fmt;

/// 1-based line and column source coordinate span.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceSpan {
    /// 1-based starting line number.
    pub start_line: usize,
    /// 1-based starting column number (in Unicode scalar values).
    pub start_column: usize,
    /// 1-based ending line number (inclusive).
    pub end_line: usize,
    /// 1-based ending column number (inclusive).
    pub end_column: usize,
}

impl SourceSpan {
    /// Constructs a 1-based span covering a single source line.
    pub fn for_line(line_number: usize, line_text: &str) -> Self {
        let char_count = line_text.chars().count();
        let start_col = line_text
            .chars()
            .position(|ch| !ch.is_whitespace())
            .map_or(1, |idx| idx + 1);
        let end_col = char_count.max(1);
        Self {
            start_line: line_number,
            start_column: start_col,
            end_line: line_number,
            end_column: end_col,
        }
    }

    /// Constructs a 1-based span anchored at the beginning of a single line.
    pub fn at_line_start(line_number: usize, line_text: &str) -> Self {
        let end_col = line_text.chars().count().max(1);
        Self {
            start_line: line_number,
            start_column: 1,
            end_line: line_number,
            end_column: end_col,
        }
    }
}

/// Cognitive directive keyword names recognized by Canon Clerk (`SPEC.md` §5).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DirectiveName {
    Exception,
    Rationale,
    Remediation,
}

impl DirectiveName {
    /// Returns the canonical lowercase directive identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exception => "exception",
            Self::Rationale => "rationale",
            Self::Remediation => "remediation",
        }
    }
}

/// Classified lexical payload for a [`CanonToken`].
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CanonTokenKind {
    /// Opening YAML frontmatter block delimited by `---` on line 1 and a closing `---`.
    Frontmatter {
        /// Unparsed YAML content between the `---` delimiters.
        yaml: String,
        /// 1-based line number where the YAML content starts (typically 2).
        yaml_start_line: usize,
    },
    /// Markdown section heading (`#` through `######`) that is not a directive header.
    Heading {
        /// Heading level from 1 (`#`) to 6 (`######`).
        level: u8,
        /// Cleaned heading text with leading/trailing `#` markers removed.
        text: String,
    },
    /// Cognitive directive line or section header (`Exception`, `Rationale`, `Remediation`).
    Directive {
        /// Normalized directive name.
        name: DirectiveName,
        /// Raw directive label as authored in source (e.g. `"**Remediation:**"`, `"## Exceptions"`).
        label: String,
        /// Trimmed inline text following the directive label on the same line.
        value: String,
    },
    /// Fenced Markdown code block delimited by ` ``` ` or `~~~`.
    CodeBlock {
        /// Optional language tag from the opening fence.
        #[serde(skip_serializing_if = "Option::is_none")]
        lang: Option<String>,
        /// Inner code lines between the opening and closing fences.
        content: String,
        /// Whether a matching closing fence was encountered before EOF.
        closed: bool,
    },
    /// Standard Markdown text line outside frontmatter and code fences.
    Text {
        /// Line text content.
        content: String,
    },
    /// Empty or whitespace-only line.
    BlankLine,
}

/// Lexical token emitted by Stage 1 scanning of a Canon Markdown document.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CanonToken {
    /// 1-based source line and column coordinates.
    pub span: SourceSpan,
    /// Complete raw source text corresponding to this token.
    pub raw: String,
    /// Classified token payload.
    pub kind: CanonTokenKind,
}

/// Error returned when a Canon file fails lexical scanning, YAML parsing, or scope containment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonParseError {
    /// Human-readable description of the parse failure.
    pub message: String,
    /// Optional source file path or realm reference associated with the error.
    pub file_path: Option<String>,
    /// Optional 1-based source coordinates pinpointing the error location.
    pub span: Option<SourceSpan>,
}

impl CanonParseError {
    /// Constructs a new parse error with a message and optional source span.
    pub fn new(message: impl Into<String>, span: Option<SourceSpan>) -> Self {
        Self {
            message: message.into(),
            file_path: None,
            span,
        }
    }

    /// Attaches a file path or source reference context to this error.
    pub fn with_file_path(mut self, file_path: Option<&str>) -> Self {
        if let Some(path) = file_path {
            self.file_path = Some(path.to_string());
        }
        self
    }
}

impl fmt::Display for CanonParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.file_path, &self.span) {
            (Some(path), Some(span)) => write!(
                f,
                "{}:{}:{}: {}",
                path, span.start_line, span.start_column, self.message
            ),
            (Some(path), None) => write!(f, "{}: {}", path, self.message),
            (None, Some(span)) => {
                write!(
                    f,
                    "line {}:{}: {}",
                    span.start_line, span.start_column, self.message
                )
            }
            (None, None) => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for CanonParseError {}

/// Strips an optional UTF-8 BOM (`\u{feff}`) and normalizes Windows CRLF (`\r\n`) to LF (`\n`).
pub fn normalize_canon_content(raw_content: &str) -> String {
    let without_bom = raw_content.strip_prefix('\u{feff}').unwrap_or(raw_content);
    if without_bom.contains("\r\n") {
        without_bom.replace("\r\n", "\n")
    } else {
        without_bom.to_string()
    }
}

fn match_directive_keyword(text: &str) -> Option<(DirectiveName, usize)> {
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("exceptions") {
        Some((DirectiveName::Exception, "exceptions".len()))
    } else if lower.starts_with("exception") {
        Some((DirectiveName::Exception, "exception".len()))
    } else if lower.starts_with("rationale") {
        Some((DirectiveName::Rationale, "rationale".len()))
    } else if lower.starts_with("remediation") {
        Some((DirectiveName::Remediation, "remediation".len()))
    } else {
        None
    }
}

fn parse_heading_directive(line: &str) -> Option<(DirectiveName, String, String)> {
    let trimmed = line.trim();
    let hash_count = trimmed.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hash_count) {
        return None;
    }
    let after_hashes = &trimmed[hash_count..];
    if !after_hashes.starts_with(char::is_whitespace) {
        return None;
    }
    let content = after_hashes.trim_start();
    let (name, kw_len) = match_directive_keyword(content)?;
    let after_kw = &content[kw_len..];

    if after_kw.is_empty() {
        return Some((name, trimmed.to_string(), String::new()));
    }
    if let Some(rest) = after_kw.strip_prefix(':') {
        let value = rest.trim().to_string();
        let label_len = trimmed.len() - rest.len();
        let label = trimmed[..label_len].trim().to_string();
        return Some((name, label, value));
    }
    None
}

fn parse_inline_directive(line: &str) -> Option<(DirectiveName, String, String)> {
    let trimmed = line.trim();
    let (has_bold, after_open) = if let Some(rest) = trimmed.strip_prefix("**") {
        (true, rest)
    } else {
        (false, trimmed)
    };

    let (name, kw_len) = match_directive_keyword(after_open)?;
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

    let value = cursor.trim().to_string();
    let consumed_len = trimmed.len() - cursor.len();
    let label = trimmed[..consumed_len].trim().to_string();
    Some((name, label, value))
}

fn parse_heading_line(line: &str) -> Option<(u8, String)> {
    let trimmed = line.trim();
    let hash_count = trimmed.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hash_count) {
        return None;
    }
    let after_hashes = &trimmed[hash_count..];
    if !after_hashes.starts_with(char::is_whitespace) {
        return None;
    }
    let stripped = after_hashes.trim().trim_end_matches('#').trim();
    if stripped.is_empty() {
        return None;
    }
    let level = u8::try_from(hash_count).ok()?;
    Some((level, stripped.to_string()))
}

fn scan_frontmatter_token(lines: &[&str]) -> Result<Option<(CanonToken, usize)>, CanonParseError> {
    let Some(&first_line) = lines.first() else {
        return Ok(None);
    };
    if first_line.trim() != "---" {
        return Ok(None);
    }

    let closing_idx = lines
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(idx, &line)| (line.trim() == "---").then_some(idx))
        .ok_or_else(|| {
            CanonParseError::new(
                "Unclosed frontmatter block; expected terminating '---' delimiter",
                Some(SourceSpan::at_line_start(1, first_line)),
            )
        })?;

    let yaml = lines[1..closing_idx].join("\n");
    let raw = lines[0..=closing_idx].join("\n");
    let end_line = closing_idx + 1;
    let end_column = lines[closing_idx].chars().count().max(1);

    let token = CanonToken {
        span: SourceSpan {
            start_line: 1,
            start_column: 1,
            end_line,
            end_column,
        },
        raw,
        kind: CanonTokenKind::Frontmatter {
            yaml,
            yaml_start_line: 2,
        },
    };
    Ok(Some((token, closing_idx + 1)))
}

fn scan_code_fence_token(lines: &[&str], start_idx: usize) -> Option<(CanonToken, usize)> {
    let open_line = lines[start_idx];
    let trimmed = open_line.trim();
    let fence_char = trimmed.chars().next()?;
    if fence_char != '`' && fence_char != '~' {
        return None;
    }
    let fence_len = trimmed.chars().take_while(|&c| c == fence_char).count();
    if fence_len < 3 {
        return None;
    }
    let info = trimmed[fence_len..].trim();
    if fence_char == '`' && info.contains('`') {
        return None;
    }
    let lang = (!info.is_empty()).then(|| info.to_string());

    let mut code_lines = Vec::new();
    let mut raw_lines = vec![open_line];
    let mut end_idx = start_idx;
    let mut closed = false;

    for (idx, &candidate) in lines.iter().enumerate().skip(start_idx + 1) {
        raw_lines.push(candidate);
        end_idx = idx;
        let cand_trimmed = candidate.trim();
        let close_count = cand_trimmed
            .chars()
            .take_while(|&c| c == fence_char)
            .count();
        if close_count >= fence_len && cand_trimmed[close_count..].trim().is_empty() {
            closed = true;
            break;
        }
        code_lines.push(candidate);
    }

    let start_line = start_idx + 1;
    let end_line = end_idx + 1;
    let end_column = lines[end_idx].chars().count().max(1);
    let token = CanonToken {
        span: SourceSpan {
            start_line,
            start_column: 1,
            end_line,
            end_column,
        },
        raw: raw_lines.join("\n"),
        kind: CanonTokenKind::CodeBlock {
            lang,
            content: code_lines.join("\n"),
            closed,
        },
    };
    Some((token, end_idx + 1))
}

/// Stage 1 Lexical Scanner: Tokenizes raw Canon Markdown into an ordered stream of [`CanonToken`]s.
///
/// Strictly pure with zero filesystem or network I/O side effects.
pub fn tokenize_canon(raw_content: &str) -> Result<Vec<CanonToken>, CanonParseError> {
    let normalized = normalize_canon_content(raw_content);
    if normalized.is_empty() {
        return Ok(Vec::new());
    }

    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut tokens = Vec::new();
    let mut line_idx = 0;

    if let Some((fm_token, next_idx)) = scan_frontmatter_token(&lines)? {
        tokens.push(fm_token);
        line_idx = next_idx;
    }

    while line_idx < lines.len() {
        if let Some((code_token, next_idx)) = scan_code_fence_token(&lines, line_idx) {
            tokens.push(code_token);
            line_idx = next_idx;
            continue;
        }

        let line = lines[line_idx];
        let line_number = line_idx + 1;
        let trimmed = line.trim();

        if trimmed.is_empty() {
            tokens.push(CanonToken {
                span: SourceSpan::at_line_start(line_number, line),
                raw: line.to_string(),
                kind: CanonTokenKind::BlankLine,
            });
            line_idx += 1;
            continue;
        }

        if let Some((name, label, value)) =
            parse_heading_directive(line).or_else(|| parse_inline_directive(line))
        {
            tokens.push(CanonToken {
                span: SourceSpan::for_line(line_number, line),
                raw: line.to_string(),
                kind: CanonTokenKind::Directive { name, label, value },
            });
            line_idx += 1;
            continue;
        }

        if let Some((level, text)) = parse_heading_line(line) {
            tokens.push(CanonToken {
                span: SourceSpan::for_line(line_number, line),
                raw: line.to_string(),
                kind: CanonTokenKind::Heading { level, text },
            });
            line_idx += 1;
            continue;
        }

        tokens.push(CanonToken {
            span: SourceSpan::for_line(line_number, line),
            raw: line.to_string(),
            kind: CanonTokenKind::Text {
                content: line.to_string(),
            },
        });
        line_idx += 1;
    }

    Ok(tokens)
}
