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

//! Stage 1 lexical scanner for unified diff streams.

use std::fmt;

/// Typed lexical token emitted by Stage 1 scanning of a unified diff stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffToken {
    /// 1-based line number in the input patch stream.
    pub line_number: usize,
    /// Classified token payload.
    pub kind: DiffTokenKind,
}

/// Lexical classification of a single line in a Git or POSIX unified diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffTokenKind {
    /// `diff --git a/<old> b/<new>` demarcation header.
    DiffGitHeader { old_path: String, new_path: String },
    /// `new file mode <mode>` extended header.
    NewFileMode(String),
    /// `deleted file mode <mode>` extended header.
    DeletedFileMode(String),
    /// `old mode <mode>` extended header.
    OldMode(String),
    /// `new mode <mode>` extended header.
    NewMode(String),
    /// `similarity index <pct>%` extended header.
    SimilarityIndex(u8),
    /// `dissimilarity index <pct>%` extended header.
    DissimilarityIndex(u8),
    /// `rename from <path>` extended header.
    RenameFrom(String),
    /// `rename to <path>` extended header.
    RenameTo(String),
    /// `copy from <path>` extended header.
    CopyFrom(String),
    /// `copy to <path>` extended header.
    CopyTo(String),
    /// `--- <path>` old-file marker.
    MinusFile(String),
    /// `+++ <path>` new-file marker.
    PlusFile(String),
    /// `Binary files ... differ` or `GIT binary patch` marker.
    BinaryMarker(String),
    /// `@@ -old_start,old_count +new_start,new_count @@` hunk coordinate header.
    HunkHeader {
        old_start: usize,
        old_count: usize,
        new_start: usize,
        new_count: usize,
        raw: String,
    },
    /// `+...` added line inside an active hunk.
    HunkAdd(String),
    /// `-...` deleted line inside an active hunk.
    HunkDelete(String),
    /// ` ...` context line inside an active hunk.
    HunkContext(String),
    /// `\ No newline at end of file` marker.
    NoNewlineAtEof(String),
    /// Other preamble or extended headers (e.g. `index ...`).
    ExtendedHeader(String),
}

/// Error returned when a unified diff stream contains malformed headers or corrupted hunks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffParseError {
    /// 1-based line number where the syntax error occurred.
    pub line_number: usize,
    /// Human-readable description of the parse failure.
    pub message: String,
}

impl fmt::Display for DiffParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Invalid unified diff at line {}: {}\n  Hint: Provide a valid unified diff patch (e.g. `git diff origin/main | canon-clerk intake --diff -`).",
            self.line_number, self.message
        )
    }
}

impl std::error::Error for DiffParseError {}

/// Unquotes a Git path token, decoding C-style escape sequences if enclosed in double quotes.
pub fn unquote_git_path(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        let inner = &trimmed[1..trimmed.len() - 1];
        let mut bytes = Vec::with_capacity(inner.len());
        let mut chars = inner.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars.peek().copied() {
                    Some('n') => {
                        chars.next();
                        bytes.push(b'\n');
                    }
                    Some('r') => {
                        chars.next();
                        bytes.push(b'\r');
                    }
                    Some('t') => {
                        chars.next();
                        bytes.push(b'\t');
                    }
                    Some('\\') => {
                        chars.next();
                        bytes.push(b'\\');
                    }
                    Some('"') => {
                        chars.next();
                        bytes.push(b'"');
                    }
                    Some(d1 @ '0'..='7') => {
                        chars.next();
                        let mut val = (d1 as u8) - b'0';
                        for _ in 0..2 {
                            if let Some(d @ '0'..='7') = chars.peek().copied() {
                                chars.next();
                                val = val.wrapping_mul(8).wrapping_add((d as u8) - b'0');
                            } else {
                                break;
                            }
                        }
                        bytes.push(val);
                    }
                    Some(other) => {
                        chars.next();
                        let mut buf = [0u8; 4];
                        bytes.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
                    }
                    None => bytes.push(b'\\'),
                }
            } else {
                let mut buf = [0u8; 4];
                bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        // Strip optional tab-separated timestamp on `--- a/foo\t2026-...`
        trimmed.split('\t').next().unwrap_or(trimmed).to_string()
    }
}

/// Strips standard Git `a/` or `b/` diff prefixes and normalizes path separators to POSIX `/`.
pub fn normalize_diff_path(raw: &str) -> String {
    let unquoted = unquote_git_path(raw).replace('\\', "/");
    if unquoted == "/dev/null" {
        return unquoted;
    }
    if let Some(rest) = unquoted
        .strip_prefix("a/")
        .or_else(|| unquoted.strip_prefix("b/"))
    {
        rest.to_string()
    } else if let Some(rest) = unquoted.strip_prefix("./") {
        rest.to_string()
    } else {
        unquoted
    }
}

/// Parses the two path operands from a `diff --git <old> <new>` header line.
fn parse_diff_git_paths(rest: &str) -> Option<(String, String)> {
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Handle quoted first path: `"a/..." ...`
    if trimmed.starts_with('"') {
        let mut idx = 1;
        let bytes = trimmed.as_bytes();
        while idx < bytes.len() {
            if bytes[idx] == b'\\' {
                idx += 2;
                continue;
            }
            if bytes[idx] == b'"' {
                let first = &trimmed[..=idx];
                let remainder = trimmed[idx + 1..].trim();
                if remainder.is_empty() {
                    return None;
                }
                return Some((normalize_diff_path(first), normalize_diff_path(remainder)));
            }
            idx += 1;
        }
        return None;
    }

    // Unquoted paths: prefer splitting on ` b/` when present to handle spaces in paths gracefully
    if let Some(split_pos) = trimmed.find(" b/") {
        let first = &trimmed[..split_pos];
        let second = &trimmed[split_pos + 1..];
        return Some((normalize_diff_path(first), normalize_diff_path(second)));
    }

    if let Some(split_pos) = trimmed.find(" \"b/") {
        let first = &trimmed[..split_pos];
        let second = &trimmed[split_pos + 1..];
        return Some((normalize_diff_path(first), normalize_diff_path(second)));
    }

    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let first = parts.next()?;
    let second = parts.next()?.trim();
    if second.is_empty() {
        return None;
    }
    Some((normalize_diff_path(first), normalize_diff_path(second)))
}

/// Parses a unified diff hunk header line (`@@ -old_start[,old_count] +new_start[,new_count] @@...`).
fn parse_hunk_header(line: &str) -> Option<DiffTokenKind> {
    let rest = line.strip_prefix("@@ -")?;
    let end_marker = rest.find(" @@")?;
    let coords = &rest[..end_marker];
    let mut parts = coords.split_whitespace();
    let old_part = parts.next()?;
    let new_part = parts.next()?.strip_prefix('+')?;

    let (old_start, old_count) = parse_range(old_part)?;
    let (new_start, new_count) = parse_range(new_part)?;

    Some(DiffTokenKind::HunkHeader {
        old_start,
        old_count,
        new_start,
        new_count,
        raw: line.to_string(),
    })
}

fn parse_range(spec: &str) -> Option<(usize, usize)> {
    if let Some((start_s, count_s)) = spec.split_once(',') {
        Some((start_s.parse().ok()?, count_s.parse().ok()?))
    } else {
        Some((spec.parse().ok()?, 1))
    }
}

fn classify_preamble_line(line: &str) -> DiffTokenKind {
    if let Some(mode) = line.strip_prefix("new file mode ") {
        return DiffTokenKind::NewFileMode(mode.trim().to_string());
    }
    if let Some(mode) = line.strip_prefix("deleted file mode ") {
        return DiffTokenKind::DeletedFileMode(mode.trim().to_string());
    }
    if let Some(mode) = line.strip_prefix("old mode ") {
        return DiffTokenKind::OldMode(mode.trim().to_string());
    }
    if let Some(mode) = line.strip_prefix("new mode ") {
        return DiffTokenKind::NewMode(mode.trim().to_string());
    }
    if let Some(pct_str) = line
        .strip_prefix("similarity index ")
        .and_then(|s| s.strip_suffix('%'))
        && let Ok(pct) = pct_str.trim().parse::<u8>()
    {
        return DiffTokenKind::SimilarityIndex(pct);
    }
    if let Some(pct_str) = line
        .strip_prefix("dissimilarity index ")
        .and_then(|s| s.strip_suffix('%'))
        && let Ok(pct) = pct_str.trim().parse::<u8>()
    {
        return DiffTokenKind::DissimilarityIndex(pct);
    }
    if let Some(path) = line.strip_prefix("rename from ") {
        return DiffTokenKind::RenameFrom(normalize_diff_path(path));
    }
    if let Some(path) = line.strip_prefix("rename to ") {
        return DiffTokenKind::RenameTo(normalize_diff_path(path));
    }
    if let Some(path) = line.strip_prefix("copy from ") {
        return DiffTokenKind::CopyFrom(normalize_diff_path(path));
    }
    if let Some(path) = line.strip_prefix("copy to ") {
        return DiffTokenKind::CopyTo(normalize_diff_path(path));
    }
    if let Some(path) = line.strip_prefix("--- ") {
        return DiffTokenKind::MinusFile(normalize_diff_path(path));
    }
    if let Some(path) = line.strip_prefix("+++ ") {
        return DiffTokenKind::PlusFile(normalize_diff_path(path));
    }
    if (line.starts_with("Binary files ") && line.ends_with(" differ"))
        || line == "GIT binary patch"
    {
        return DiffTokenKind::BinaryMarker(line.to_string());
    }
    DiffTokenKind::ExtendedHeader(line.to_string())
}

fn classify_hunk_line(line_number: usize, line: &str) -> Result<DiffTokenKind, DiffParseError> {
    if let Some(added) = line.strip_prefix('+') {
        Ok(DiffTokenKind::HunkAdd(added.to_string()))
    } else if let Some(deleted) = line.strip_prefix('-') {
        Ok(DiffTokenKind::HunkDelete(deleted.to_string()))
    } else if let Some(context) = line.strip_prefix(' ') {
        Ok(DiffTokenKind::HunkContext(context.to_string()))
    } else if line.starts_with("\\ No newline at end of file") {
        Ok(DiffTokenKind::NoNewlineAtEof(line.to_string()))
    } else if line.is_empty() {
        // Blank context line with stripped trailing space
        Ok(DiffTokenKind::HunkContext(String::new()))
    } else {
        Err(DiffParseError {
            line_number,
            message: format!("unexpected line inside diff hunk: `{line}`"),
        })
    }
}

/// Stage 1 Lexical Scanner: Tokenizes a unified diff string into an ordered stream of [`DiffToken`]s.
pub fn tokenize_diff(input: &str) -> Result<Vec<DiffToken>, DiffParseError> {
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut tokens = Vec::new();
    let mut in_hunk = false;

    for (idx, raw_line) in input.lines().enumerate() {
        let line_number = idx + 1;
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);

        if let Some(rest) = line.strip_prefix("diff --git ") {
            in_hunk = false;
            let (old_path, new_path) =
                parse_diff_git_paths(rest).ok_or_else(|| DiffParseError {
                    line_number,
                    message: format!("malformed `diff --git` header: `{line}`"),
                })?;
            tokens.push(DiffToken {
                line_number,
                kind: DiffTokenKind::DiffGitHeader { old_path, new_path },
            });
            continue;
        }

        if line.starts_with("@@ ") {
            let kind = parse_hunk_header(line).ok_or_else(|| DiffParseError {
                line_number,
                message: format!("malformed hunk coordinate header: `{line}`"),
            })?;
            in_hunk = true;
            tokens.push(DiffToken { line_number, kind });
            continue;
        }

        if !in_hunk {
            tokens.push(DiffToken {
                line_number,
                kind: classify_preamble_line(line),
            });
            continue;
        }

        // Inside an active hunk: check if a new file's `--- ` header or binary marker started without `diff --git`
        if let Some(path) = line.strip_prefix("--- ")
            && (path.starts_with("a/") || path.starts_with("\"/") || path == "/dev/null")
        {
            in_hunk = false;
            tokens.push(DiffToken {
                line_number,
                kind: DiffTokenKind::MinusFile(normalize_diff_path(path)),
            });
            continue;
        }

        tokens.push(DiffToken {
            line_number,
            kind: classify_hunk_line(line_number, line)?,
        });
    }

    Ok(tokens)
}
