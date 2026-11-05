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

//! Pure two-stage unified diff parser (Stage 1 lexical tokenizer -> Stage 2 semantic AST builder).

use crate::caseload::{ContentOmissionReason, FileArtifact, FileChangeStatus, PatchOmissionReason};
use std::collections::BTreeMap;
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
            if let Some(mode) = line.strip_prefix("new file mode ") {
                tokens.push(DiffToken {
                    line_number,
                    kind: DiffTokenKind::NewFileMode(mode.trim().to_string()),
                });
                continue;
            }
            if let Some(mode) = line.strip_prefix("deleted file mode ") {
                tokens.push(DiffToken {
                    line_number,
                    kind: DiffTokenKind::DeletedFileMode(mode.trim().to_string()),
                });
                continue;
            }
            if let Some(mode) = line.strip_prefix("old mode ") {
                tokens.push(DiffToken {
                    line_number,
                    kind: DiffTokenKind::OldMode(mode.trim().to_string()),
                });
                continue;
            }
            if let Some(mode) = line.strip_prefix("new mode ") {
                tokens.push(DiffToken {
                    line_number,
                    kind: DiffTokenKind::NewMode(mode.trim().to_string()),
                });
                continue;
            }
            if let Some(pct_str) = line
                .strip_prefix("similarity index ")
                .and_then(|s| s.strip_suffix('%'))
                && let Ok(pct) = pct_str.trim().parse::<u8>()
            {
                tokens.push(DiffToken {
                    line_number,
                    kind: DiffTokenKind::SimilarityIndex(pct),
                });
                continue;
            }
            if let Some(pct_str) = line
                .strip_prefix("dissimilarity index ")
                .and_then(|s| s.strip_suffix('%'))
                && let Ok(pct) = pct_str.trim().parse::<u8>()
            {
                tokens.push(DiffToken {
                    line_number,
                    kind: DiffTokenKind::DissimilarityIndex(pct),
                });
                continue;
            }
            if let Some(path) = line.strip_prefix("rename from ") {
                tokens.push(DiffToken {
                    line_number,
                    kind: DiffTokenKind::RenameFrom(normalize_diff_path(path)),
                });
                continue;
            }
            if let Some(path) = line.strip_prefix("rename to ") {
                tokens.push(DiffToken {
                    line_number,
                    kind: DiffTokenKind::RenameTo(normalize_diff_path(path)),
                });
                continue;
            }
            if let Some(path) = line.strip_prefix("copy from ") {
                tokens.push(DiffToken {
                    line_number,
                    kind: DiffTokenKind::CopyFrom(normalize_diff_path(path)),
                });
                continue;
            }
            if let Some(path) = line.strip_prefix("copy to ") {
                tokens.push(DiffToken {
                    line_number,
                    kind: DiffTokenKind::CopyTo(normalize_diff_path(path)),
                });
                continue;
            }
            if let Some(path) = line.strip_prefix("--- ") {
                tokens.push(DiffToken {
                    line_number,
                    kind: DiffTokenKind::MinusFile(normalize_diff_path(path)),
                });
                continue;
            }
            if let Some(path) = line.strip_prefix("+++ ") {
                tokens.push(DiffToken {
                    line_number,
                    kind: DiffTokenKind::PlusFile(normalize_diff_path(path)),
                });
                continue;
            }
            if (line.starts_with("Binary files ") && line.ends_with(" differ"))
                || line == "GIT binary patch"
            {
                tokens.push(DiffToken {
                    line_number,
                    kind: DiffTokenKind::BinaryMarker(line.to_string()),
                });
                continue;
            }

            tokens.push(DiffToken {
                line_number,
                kind: DiffTokenKind::ExtendedHeader(line.to_string()),
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

        if let Some(added) = line.strip_prefix('+') {
            tokens.push(DiffToken {
                line_number,
                kind: DiffTokenKind::HunkAdd(added.to_string()),
            });
        } else if let Some(deleted) = line.strip_prefix('-') {
            tokens.push(DiffToken {
                line_number,
                kind: DiffTokenKind::HunkDelete(deleted.to_string()),
            });
        } else if let Some(context) = line.strip_prefix(' ') {
            tokens.push(DiffToken {
                line_number,
                kind: DiffTokenKind::HunkContext(context.to_string()),
            });
        } else if line.starts_with("\\ No newline at end of file") {
            tokens.push(DiffToken {
                line_number,
                kind: DiffTokenKind::NoNewlineAtEof(line.to_string()),
            });
        } else if line.is_empty() {
            // Blank context line with stripped trailing space
            tokens.push(DiffToken {
                line_number,
                kind: DiffTokenKind::HunkContext(String::new()),
            });
        } else {
            return Err(DiffParseError {
                line_number,
                message: format!("unexpected line inside diff hunk: `{line}`"),
            });
        }
    }

    Ok(tokens)
}

#[derive(Default)]
struct FileDraft {
    old_path: Option<String>,
    new_path: Option<String>,
    is_new_file: bool,
    is_deleted_file: bool,
    is_rename: bool,
    is_copy: bool,
    is_binary: bool,
    lines_added: usize,
    lines_deleted: usize,
    patch_lines: Vec<String>,
}

impl FileDraft {
    fn into_artifact(self) -> Option<FileArtifact> {
        let status = if self.is_new_file {
            FileChangeStatus::Added
        } else if self.is_deleted_file {
            FileChangeStatus::Deleted
        } else if self.is_copy {
            FileChangeStatus::Copied
        } else if self.is_rename {
            FileChangeStatus::Renamed
        } else {
            FileChangeStatus::Modified
        };

        let path = match status {
            FileChangeStatus::Deleted => self.old_path.clone().or(self.new_path.clone())?,
            _ => self.new_path.clone().or(self.old_path.clone())?,
        };

        if path.is_empty() || path == "/dev/null" {
            return None;
        }

        let previous_path = match status {
            FileChangeStatus::Renamed | FileChangeStatus::Copied => self
                .old_path
                .filter(|old| old != &path && old != "/dev/null"),
            _ => None,
        };

        let (patch, patch_omission_reason) = if self.is_binary {
            (None, Some(PatchOmissionReason::Binary))
        } else if self.patch_lines.is_empty() {
            (None, Some(PatchOmissionReason::Unchanged))
        } else {
            (Some(self.patch_lines.join("\n")), None)
        };

        let content_omission_reason = if self.is_binary {
            Some(ContentOmissionReason::Binary)
        } else if status == FileChangeStatus::Deleted {
            Some(ContentOmissionReason::Deleted)
        } else {
            Some(ContentOmissionReason::NotRequested)
        };

        Some(FileArtifact {
            path,
            status,
            previous_path,
            lines_added: self.lines_added,
            lines_deleted: self.lines_deleted,
            patch,
            patch_omission_reason,
            content: None,
            content_omission_reason,
        })
    }
}

fn commit_draft(draft: Option<FileDraft>, artifacts: &mut BTreeMap<String, FileArtifact>) {
    if let Some(d) = draft
        && let Some(artifact) = d.into_artifact()
    {
        if let Some(existing) = artifacts.get_mut(&artifact.path) {
            // Codify multi-occurrence semantics: merge subsequent patches for the same file path
            existing.lines_added += artifact.lines_added;
            existing.lines_deleted += artifact.lines_deleted;
            if let Some(new_patch) = artifact.patch {
                match &mut existing.patch {
                    Some(prev_patch) => {
                        prev_patch.push('\n');
                        prev_patch.push_str(&new_patch);
                    }
                    None => {
                        existing.patch = Some(new_patch);
                        existing.patch_omission_reason = None;
                    }
                }
            }
            if artifact.status == FileChangeStatus::Deleted {
                existing.status = FileChangeStatus::Deleted;
                existing.content_omission_reason = Some(ContentOmissionReason::Deleted);
            }
        } else {
            artifacts.insert(artifact.path.clone(), artifact);
        }
    }
}

/// Stage 2 Semantic AST Builder: Assembles [`FileArtifact`] records from a [`DiffToken`] slice.
pub fn assemble_diff_artifacts(
    tokens: &[DiffToken],
) -> Result<BTreeMap<String, FileArtifact>, DiffParseError> {
    if tokens.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut artifacts = BTreeMap::new();
    let mut current: Option<FileDraft> = None;
    let mut saw_file_demarcation = false;

    for token in tokens {
        match &token.kind {
            DiffTokenKind::DiffGitHeader { old_path, new_path } => {
                saw_file_demarcation = true;
                commit_draft(current.take(), &mut artifacts);
                current = Some(FileDraft {
                    old_path: Some(old_path.clone()),
                    new_path: Some(new_path.clone()),
                    is_rename: old_path != new_path,
                    ..FileDraft::default()
                });
            }
            DiffTokenKind::NewFileMode(_) => {
                if let Some(draft) = current.as_mut() {
                    draft.is_new_file = true;
                    draft.is_rename = false;
                }
            }
            DiffTokenKind::DeletedFileMode(_) => {
                if let Some(draft) = current.as_mut() {
                    draft.is_deleted_file = true;
                    draft.is_rename = false;
                }
            }
            DiffTokenKind::RenameFrom(old_path) => {
                if let Some(draft) = current.as_mut() {
                    draft.is_rename = true;
                    draft.old_path = Some(old_path.clone());
                }
            }
            DiffTokenKind::RenameTo(new_path) => {
                if let Some(draft) = current.as_mut() {
                    draft.is_rename = true;
                    draft.new_path = Some(new_path.clone());
                }
            }
            DiffTokenKind::CopyFrom(old_path) => {
                if let Some(draft) = current.as_mut() {
                    draft.is_copy = true;
                    draft.is_rename = false;
                    draft.old_path = Some(old_path.clone());
                }
            }
            DiffTokenKind::CopyTo(new_path) => {
                if let Some(draft) = current.as_mut() {
                    draft.is_copy = true;
                    draft.is_rename = false;
                    draft.new_path = Some(new_path.clone());
                }
            }
            DiffTokenKind::MinusFile(path) => {
                saw_file_demarcation = true;
                // Support standard unified diffs without a `diff --git` header
                let starts_new_standalone_file = match &current {
                    None => true,
                    Some(draft) => !draft.patch_lines.is_empty(),
                };
                if starts_new_standalone_file {
                    commit_draft(current.take(), &mut artifacts);
                    current = Some(FileDraft::default());
                }
                if let Some(draft) = current.as_mut() {
                    if path == "/dev/null" {
                        draft.is_new_file = true;
                        draft.is_rename = false;
                    } else {
                        draft.old_path = Some(path.clone());
                    }
                }
            }
            DiffTokenKind::PlusFile(path) => {
                if let Some(draft) = current.as_mut() {
                    if path == "/dev/null" {
                        draft.is_deleted_file = true;
                        draft.is_rename = false;
                    } else {
                        draft.new_path = Some(path.clone());
                    }
                } else {
                    return Err(DiffParseError {
                        line_number: token.line_number,
                        message: "`+++` header encountered without preceding `diff --git` or `---` header"
                            .to_string(),
                    });
                }
            }
            DiffTokenKind::BinaryMarker(_) => {
                if let Some(draft) = current.as_mut() {
                    draft.is_binary = true;
                }
            }
            DiffTokenKind::HunkHeader { raw, .. } => {
                let draft = current.as_mut().ok_or_else(|| DiffParseError {
                    line_number: token.line_number,
                    message: "hunk header `@@` encountered outside of a file diff block"
                        .to_string(),
                })?;
                draft.patch_lines.push(raw.clone());
            }
            DiffTokenKind::HunkAdd(text) => {
                let draft = current.as_mut().ok_or_else(|| DiffParseError {
                    line_number: token.line_number,
                    message: "added hunk line encountered outside of a file diff block".to_string(),
                })?;
                draft.lines_added += 1;
                draft.patch_lines.push(format!("+{text}"));
            }
            DiffTokenKind::HunkDelete(text) => {
                let draft = current.as_mut().ok_or_else(|| DiffParseError {
                    line_number: token.line_number,
                    message: "deleted hunk line encountered outside of a file diff block"
                        .to_string(),
                })?;
                draft.lines_deleted += 1;
                draft.patch_lines.push(format!("-{text}"));
            }
            DiffTokenKind::HunkContext(text) => {
                let draft = current.as_mut().ok_or_else(|| DiffParseError {
                    line_number: token.line_number,
                    message: "context hunk line encountered outside of a file diff block"
                        .to_string(),
                })?;
                draft.patch_lines.push(format!(" {text}"));
            }
            DiffTokenKind::NoNewlineAtEof(raw) => {
                if let Some(draft) = current.as_mut() {
                    draft.patch_lines.push(raw.clone());
                }
            }
            DiffTokenKind::OldMode(_)
            | DiffTokenKind::NewMode(_)
            | DiffTokenKind::SimilarityIndex(_)
            | DiffTokenKind::DissimilarityIndex(_)
            | DiffTokenKind::ExtendedHeader(_) => {}
        }
    }

    if !saw_file_demarcation {
        return Err(DiffParseError {
            line_number: tokens[0].line_number,
            message: "input does not contain any recognizable unified diff headers (`diff --git` or `---`/`+++`)"
                .to_string(),
        });
    }

    commit_draft(current.take(), &mut artifacts);
    Ok(artifacts)
}

/// Parses a unified diff string in-memory into a deterministically ordered map of [`FileArtifact`]s.
pub fn parse_unified_diff(input: &str) -> Result<BTreeMap<String, FileArtifact>, DiffParseError> {
    let tokens = tokenize_diff(input)?;
    assemble_diff_artifacts(&tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod tokenizer {
        use super::*;

        #[test]
        fn test_tokenize_empty_and_whitespace_returns_empty_vec() {
            assert!(tokenize_diff("").expect("empty").is_empty());
            assert!(tokenize_diff("   \n\t\n  ").expect("whitespace").is_empty());
        }

        #[test]
        fn test_tokenize_git_headers_and_hunk_lines() {
            let patch = "\
diff --git a/src/lib.rs b/src/lib.rs
index 1234567..89abcdef 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,2 +1,3 @@
 pub mod cli;
+pub mod caseload;
 // end
";
            let tokens = tokenize_diff(patch).expect("tokenize valid patch");
            assert_eq!(
                tokens[0],
                DiffToken {
                    line_number: 1,
                    kind: DiffTokenKind::DiffGitHeader {
                        old_path: "src/lib.rs".to_string(),
                        new_path: "src/lib.rs".to_string(),
                    }
                }
            );
            assert!(matches!(
                tokens[4].kind,
                DiffTokenKind::HunkHeader {
                    old_start: 1,
                    old_count: 2,
                    new_start: 1,
                    new_count: 3,
                    ..
                }
            ));
            assert_eq!(
                tokens[6].kind,
                DiffTokenKind::HunkAdd("pub mod caseload;".to_string())
            );
        }

        #[test]
        fn test_unquote_git_path_decodes_escaped_and_octal_sequences() {
            assert_eq!(
                normalize_diff_path(r#""a/dir with spaces/file\tname.rs""#),
                "dir with spaces/file\tname.rs"
            );
            // \303\251 is UTF-8 for 'é'
            assert_eq!(normalize_diff_path(r#""b/caf\303\251.rs""#), "café.rs");
        }
    }

    mod semantic_assembly {
        use super::*;

        #[test]
        fn test_parse_multi_file_added_modified_deleted() {
            let patch = "\
diff --git a/src/added.rs b/src/added.rs
new file mode 100644
index 0000000..1111111
--- /dev/null
+++ b/src/added.rs
@@ -0,0 +1,2 @@
+pub fn hello() {}
+pub fn world() {}
diff --git a/src/modified.rs b/src/modified.rs
index 2222222..3333333 100644
--- a/src/modified.rs
+++ b/src/modified.rs
@@ -5,3 +5,3 @@
 fn keep() {}
-fn old() {}
+fn new() {}
diff --git a/src/deleted.rs b/src/deleted.rs
deleted file mode 100644
index 4444444..0000000
--- a/src/deleted.rs
+++ /dev/null
@@ -1,2 +0,0 @@
-line 1
-line 2
";
            let artifacts = parse_unified_diff(patch).expect("parse multi-file diff");
            assert_eq!(artifacts.len(), 3);

            let added = &artifacts["src/added.rs"];
            assert_eq!(added.status, FileChangeStatus::Added);
            assert_eq!(added.lines_added, 2);
            assert_eq!(added.lines_deleted, 0);
            assert!(
                added
                    .patch
                    .as_deref()
                    .unwrap()
                    .contains("+pub fn hello() {}")
            );
            assert_eq!(added.patch_omission_reason, None);
            assert_eq!(
                added.content_omission_reason,
                Some(ContentOmissionReason::NotRequested)
            );

            let modified = &artifacts["src/modified.rs"];
            assert_eq!(modified.status, FileChangeStatus::Modified);
            assert_eq!(modified.lines_added, 1);
            assert_eq!(modified.lines_deleted, 1);

            let deleted = &artifacts["src/deleted.rs"];
            assert_eq!(deleted.status, FileChangeStatus::Deleted);
            assert_eq!(deleted.lines_added, 0);
            assert_eq!(deleted.lines_deleted, 2);
            assert_eq!(
                deleted.content_omission_reason,
                Some(ContentOmissionReason::Deleted)
            );
        }

        #[test]
        fn test_parse_pure_rename_and_copied_files() {
            let patch = "\
diff --git a/src/old_name.rs b/src/new_name.rs
similarity index 100%
rename from src/old_name.rs
rename to src/new_name.rs
diff --git a/src/template.rs b/src/copied.rs
similarity index 90%
copy from src/template.rs
copy to src/copied.rs
--- a/src/template.rs
+++ b/src/copied.rs
@@ -1 +1,2 @@
 fn base() {}
+fn extra() {}
";
            let artifacts = parse_unified_diff(patch).expect("parse rename and copy");
            assert_eq!(artifacts.len(), 2);

            let renamed = &artifacts["src/new_name.rs"];
            assert_eq!(renamed.status, FileChangeStatus::Renamed);
            assert_eq!(renamed.previous_path.as_deref(), Some("src/old_name.rs"));
            assert_eq!(renamed.patch, None);
            assert_eq!(
                renamed.patch_omission_reason,
                Some(PatchOmissionReason::Unchanged)
            );

            let copied = &artifacts["src/copied.rs"];
            assert_eq!(copied.status, FileChangeStatus::Copied);
            assert_eq!(copied.previous_path.as_deref(), Some("src/template.rs"));
            assert_eq!(copied.lines_added, 1);
            assert!(copied.patch.is_some());
        }

        #[test]
        fn test_parse_binary_file_sets_binary_omission_reasons() {
            let patch = "\
diff --git a/assets/icon.png b/assets/icon.png
new file mode 100644
index 0000000..abcdef1
Binary files /dev/null and b/assets/icon.png differ
";
            let artifacts = parse_unified_diff(patch).expect("parse binary diff");
            let icon = &artifacts["assets/icon.png"];
            assert_eq!(icon.status, FileChangeStatus::Added);
            assert_eq!(icon.patch, None);
            assert_eq!(
                icon.patch_omission_reason,
                Some(PatchOmissionReason::Binary)
            );
            assert_eq!(
                icon.content_omission_reason,
                Some(ContentOmissionReason::Binary)
            );
        }

        #[test]
        fn test_repeated_file_diffs_merge_hunks_and_line_counts() {
            let patch = "\
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,1 +1,2 @@
 pub mod a;
+pub mod b;
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,1 +11,2 @@
 pub mod x;
+pub mod y;
";
            let artifacts = parse_unified_diff(patch).expect("parse repeated file diff");
            assert_eq!(artifacts.len(), 1);
            let lib = &artifacts["src/lib.rs"];
            assert_eq!(lib.lines_added, 2);
            assert!(lib.patch.as_deref().unwrap().contains("+pub mod b;"));
            assert!(lib.patch.as_deref().unwrap().contains("+pub mod y;"));
        }

        #[test]
        fn test_corrupted_diff_rejects_with_parse_error() {
            let err = parse_unified_diff("this is not a unified diff at all\njust random text")
                .expect_err("non-diff text must fail");
            assert_eq!(err.line_number, 1);
            assert!(err.to_string().contains("Invalid unified diff"));
        }
    }
}
