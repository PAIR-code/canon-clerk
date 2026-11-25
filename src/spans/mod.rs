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

//! Pure-Rust pan-source topological span tiling, explicit lexical span fences, and Strategy D
//! breadcrumb skeleton rendering (`docs/architecture/nodes/vet.md` §4.3, `audit.md` §3–§4).

mod head_context;
mod skeleton;
mod tiler;

pub use head_context::{DEFAULT_DIFF_HEAD_LINES, ENV_DIFF_HEAD_LINES, MAX_HEAD_CONTEXT_CHARS};
pub(crate) use head_context::{
    expand_diff_patch_lines, format_diff_patch_with_head_context, render_exhibit_lines,
};
pub(crate) use skeleton::render_breadcrumb_skeleton;
pub(crate) use tiler::{TiledSpan, render_fenced_spans, tile_exhibit_lines};

/// Structural language family used for topological cut scoring and Strategy D ancestor spines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceKind {
    Markdown,
    Toml,
    Yaml,
    Code,
}

impl SourceKind {
    /// Classifies a repository-relative file path by extension.
    pub(crate) fn from_path(path: &str) -> Self {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".md") || lower.ends_with(".mdx") || lower.ends_with(".markdown") {
            Self::Markdown
        } else if lower.ends_with(".toml") {
            Self::Toml
        } else if lower.ends_with(".yml") || lower.ends_with(".yaml") {
            Self::Yaml
        } else {
            Self::Code
        }
    }
}

/// One logical line of an admitted file exhibit (unified diff hunk or workspace snapshot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExhibitLine {
    /// Source text stripped of diff prefixes and line-number gutters.
    pub(crate) text: String,
    /// Rendered 1-based line-annotated representation (`+  42 | ...`, `   42 | ...`, `    - | ...`, or `@@ ... @@`).
    pub(crate) rendered: String,
    /// 1-based post-image target-file line number (`None` for hunk headers, metadata, or `-` deletions).
    pub(crate) target_line: Option<usize>,
    /// Nearest 1-based post-image anchor line (used for deleted-only spans and hunk headers).
    pub(crate) anchor_line: usize,
    /// `true` when this line is a unified diff hunk header (`@@ -a,b +c,d @@ ...`).
    pub(crate) is_hunk_header: bool,
    /// `true` when this line is a deleted diff line (`-`).
    pub(crate) is_deletion: bool,
}

/// Parses a full-file UTF-8 snapshot into 1-based [`ExhibitLine`] records.
pub(crate) fn parse_snapshot_lines(content: &str) -> Vec<ExhibitLine> {
    content
        .lines()
        .enumerate()
        .map(|(idx, line)| {
            let line_no = idx + 1;
            ExhibitLine {
                text: line.to_string(),
                rendered: format!("{line_no:>4} | {line}"),
                target_line: Some(line_no),
                anchor_line: line_no,
                is_hunk_header: false,
                is_deletion: false,
            }
        })
        .collect()
}

/// Parses a unified diff patch into 1-based target-file [`ExhibitLine`] records matching
/// `audit::exhibits::annotate_unified_patch_lines`.
pub(crate) fn parse_unified_patch_lines(patch: &str) -> Vec<ExhibitLine> {
    let mut lines = Vec::new();
    let mut current_new_line: Option<usize> = None;

    for raw_line in patch.lines() {
        if let Some(start_new) = parse_hunk_new_start(raw_line) {
            current_new_line = Some(start_new);
            lines.push(ExhibitLine {
                text: raw_line.to_string(),
                rendered: raw_line.to_string(),
                target_line: None,
                anchor_line: start_new,
                is_hunk_header: true,
                is_deletion: false,
            });
            continue;
        }

        let anchor = current_new_line.unwrap_or(1);
        match current_new_line {
            Some(line_no) if raw_line.starts_with('+') && !raw_line.starts_with("+++") => {
                let body = &raw_line[1..];
                lines.push(ExhibitLine {
                    text: body.to_string(),
                    rendered: format!("+{line_no:>4} | {body}"),
                    target_line: Some(line_no),
                    anchor_line: line_no,
                    is_hunk_header: false,
                    is_deletion: false,
                });
                current_new_line = Some(line_no + 1);
            }
            Some(line_no) if raw_line.starts_with(' ') => {
                let body = &raw_line[1..];
                lines.push(ExhibitLine {
                    text: body.to_string(),
                    rendered: format!(" {line_no:>4} | {body}"),
                    target_line: Some(line_no),
                    anchor_line: line_no,
                    is_hunk_header: false,
                    is_deletion: false,
                });
                current_new_line = Some(line_no + 1);
            }
            Some(_) if raw_line.starts_with('-') && !raw_line.starts_with("---") => {
                let body = &raw_line[1..];
                lines.push(ExhibitLine {
                    text: body.to_string(),
                    rendered: format!("    - | {body}"),
                    target_line: None,
                    anchor_line: anchor,
                    is_hunk_header: false,
                    is_deletion: true,
                });
            }
            _ => {
                lines.push(ExhibitLine {
                    text: raw_line.to_string(),
                    rendered: raw_line.to_string(),
                    target_line: None,
                    anchor_line: anchor,
                    is_hunk_header: false,
                    is_deletion: false,
                });
            }
        }
    }

    lines
}

pub(crate) fn parse_hunk_new_start(line: &str) -> Option<usize> {
    let after_prefix = line.strip_prefix("@@ -")?;
    let (header_body, _) = after_prefix.split_once(" @@")?;
    let (_, new_range) = header_body.split_once(" +")?;
    let start_str = new_range
        .split_once(',')
        .map_or(new_range, |(start, _)| start);
    let parsed = start_str.trim().parse::<usize>().ok()?;
    Some(parsed.max(1))
}

/// Counts leading visual indentation (tabs count as 4 spaces).
pub(crate) fn leading_indent(line: &str) -> usize {
    let mut width = 0usize;
    for ch in line.chars() {
        match ch {
            ' ' => width += 1,
            '\t' => width += 4,
            _ => break,
        }
    }
    width
}

/// Computes fence-aware Markdown ATX heading levels (`Some(1..=6)`) for every line in `lines`,
/// ignoring lines inside fenced code blocks (```` ``` ```` or `~~~`) and deleted diff lines.
pub(crate) fn markdown_heading_levels(lines: &[ExhibitLine]) -> Vec<Option<usize>> {
    let mut levels = vec![None; lines.len()];
    let mut active_fence: Option<(char, usize)> = None;

    for (idx, line) in lines.iter().enumerate() {
        if line.is_deletion || line.is_hunk_header {
            continue;
        }
        let trimmed = line.text.trim_start();
        if let Some((fence_ch, fence_len)) = active_fence {
            if is_closing_fence(trimmed, fence_ch, fence_len) {
                active_fence = None;
            }
            continue;
        }
        if let Some(fence) = parse_opening_fence(trimmed) {
            active_fence = Some(fence);
            continue;
        }
        levels[idx] = parse_atx_heading_level(trimmed);
    }

    levels
}

fn parse_opening_fence(trimmed: &str) -> Option<(char, usize)> {
    let first = trimmed.chars().next()?;
    if first != '`' && first != '~' {
        return None;
    }
    let run_len = trimmed.chars().take_while(|&c| c == first).count();
    if run_len >= 3 {
        Some((first, run_len))
    } else {
        None
    }
}

fn is_closing_fence(trimmed: &str, fence_ch: char, min_len: usize) -> bool {
    let run_len = trimmed.chars().take_while(|&c| c == fence_ch).count();
    run_len >= min_len && trimmed[run_len..].trim().is_empty()
}

fn parse_atx_heading_level(trimmed: &str) -> Option<usize> {
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = &trimmed[hashes..];
    if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
        Some(hashes)
    } else {
        None
    }
}

/// Returns `true` if `trimmed` is a TOML `[section]` or `[[array_section]]` header.
pub(crate) fn is_toml_section_header(trimmed: &str) -> bool {
    if !trimmed.starts_with('[') {
        return false;
    }
    let without_comment = trimmed.split('#').next().unwrap_or(trimmed).trim();
    without_comment.ends_with(']') && without_comment.len() >= 3
}

/// Returns `true` if `trimmed` is a contiguous declaration preamble (attribute, decorator, or doc comment)
/// that should stay glued to the declaration immediately below it.
pub(crate) fn is_preamble_line(trimmed: &str, kind: SourceKind) -> bool {
    if trimmed.is_empty() {
        return false;
    }
    match kind {
        SourceKind::Code => {
            trimmed.starts_with("#[")
                || trimmed.starts_with('@')
                || trimmed.starts_with("///")
                || trimmed.starts_with("//!")
                || trimmed.starts_with("/**")
                || trimmed.starts_with("/*")
                || trimmed.starts_with("* ")
                || trimmed == "*"
                || trimmed == "*/"
                || trimmed.starts_with("//")
        }
        SourceKind::Yaml | SourceKind::Toml => trimmed.starts_with('#'),
        SourceKind::Markdown => false,
    }
}
