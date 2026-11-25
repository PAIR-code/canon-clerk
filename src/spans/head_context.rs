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

//! Diff exhibit head-context budget expansion (`docs/architecture/overview.md` §9).
//!
//! Merges post-patch lines `1..=N` with unified diff hunks so small files (`<= N` lines)
//! render as complete post-patch annotated diffs and larger files (`> N` lines) preserve
//! top-of-file preambles (license headers, YAML frontmatter, imports, `README.md` leads)
//! without duplicating lines or bloating prompts on minified single-line files.

use super::{ExhibitLine, SourceKind, parse_unified_patch_lines};
use std::fmt::Write as _;

/// Built-in default post-image head-context line budget for modified diff exhibits
/// (`docs/architecture/overview.md` §9).
pub const DEFAULT_DIFF_HEAD_LINES: usize = 80;

/// Internal safety character ceiling for the injected head-context prefix so minified
/// single-line files never bloat screening or trial prompts.
pub const MAX_HEAD_CONTEXT_CHARS: usize = 4_096;

/// Environment variable overriding `--diff-head-lines` (`docs/architecture/overview.md` §9).
pub const ENV_DIFF_HEAD_LINES: &str = "CANON_CLERK_DIFF_HEAD_LINES";

/// Typed post-image head-context line budget for modified diff exhibits (`0` disables expansion).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiffHeadBudget(pub(crate) usize);

impl DiffHeadBudget {
    pub(crate) const fn lines(self) -> usize {
        self.0
    }
}

/// Named-field inputs for expanding a unified diff patch with post-image head context.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DiffHeadExpandInput<'a> {
    pub(crate) path: &'a str,
    pub(crate) patch: &'a str,
    pub(crate) post_patch_content: Option<&'a str>,
    pub(crate) budget: DiffHeadBudget,
}

struct ParsedHunk {
    header: ExhibitLine,
    new_start: usize,
    body: Vec<ExhibitLine>,
}

/// Renders a slice of [`ExhibitLine`] records into a newline-delimited string.
pub(crate) fn render_exhibit_lines(lines: &[ExhibitLine]) -> String {
    let mut out = String::with_capacity(lines.len().saturating_mul(48));
    for line in lines {
        let _ = writeln!(out, "{}", line.rendered);
    }
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Expands a unified diff patch with up to `input.budget.lines()` post-image head lines from
/// `input.post_patch_content`, bounded internally by [`MAX_HEAD_CONTEXT_CHARS`].
///
/// Falls back cleanly to [`parse_unified_patch_lines`] when `budget` is `0`,
/// `post_patch_content` is absent/empty, the patch has no hunks, or line 1 alone exceeds
/// [`MAX_HEAD_CONTEXT_CHARS`].
pub(crate) fn expand_diff_patch_lines(input: DiffHeadExpandInput<'_>) -> Vec<ExhibitLine> {
    let parsed = parse_unified_patch_lines(input.patch);
    let diff_head_lines = input.budget.lines();
    if diff_head_lines == 0 {
        return parsed;
    }
    let Some(content) = input.post_patch_content else {
        return parsed;
    };
    let file_lines: Vec<&str> = content.lines().collect();
    if file_lines.is_empty() {
        return parsed;
    }

    let effective_head_limit = compute_effective_head_limit(&file_lines, diff_head_lines);
    if effective_head_limit == 0 {
        return parsed;
    }

    let (leading, hunks) = split_into_hunks(parsed);
    if hunks.is_empty() {
        return leading;
    }

    let kind = SourceKind::from_path(input.path);
    let mut out = Vec::with_capacity(effective_head_limit + hunks.len().saturating_mul(16));
    out.extend(leading);

    let mut cursor = 1usize;
    let mut emitted_post_head_hunk = false;

    for hunk in hunks {
        if !emitted_post_head_hunk && hunk.new_start <= effective_head_limit + 1 {
            emit_post_image_range(
                &file_lines,
                cursor,
                hunk.new_start.saturating_sub(1),
                &mut out,
            );
            cursor = cursor.max(hunk.new_start);
            for line in hunk.body {
                if let Some(ln) = line.target_line {
                    cursor = cursor.max(ln + 1);
                }
                out.push(line);
            }
        } else {
            if !emitted_post_head_hunk {
                if cursor <= effective_head_limit {
                    emit_post_image_range(&file_lines, cursor, effective_head_limit, &mut out);
                    cursor = effective_head_limit + 1;
                }
                if cursor < hunk.new_start {
                    out.push(make_elision_line(kind, cursor, hunk.new_start - 1));
                }
                emitted_post_head_hunk = true;
            }
            out.push(hunk.header);
            for line in hunk.body {
                if let Some(ln) = line.target_line {
                    cursor = cursor.max(ln + 1);
                }
                out.push(line);
            }
        }
    }

    if !emitted_post_head_hunk {
        if cursor <= effective_head_limit {
            emit_post_image_range(&file_lines, cursor, effective_head_limit, &mut out);
            cursor = effective_head_limit + 1;
        }
        if cursor <= file_lines.len() {
            out.push(make_elision_line(kind, cursor, file_lines.len()));
        }
    }

    out
}

/// Formats a unified diff patch with head-context expansion into a 1-based line-annotated string.
pub(crate) fn format_diff_patch_with_head_context(input: DiffHeadExpandInput<'_>) -> String {
    let lines = expand_diff_patch_lines(input);
    render_exhibit_lines(&lines)
}

fn compute_effective_head_limit(file_lines: &[&str], diff_head_lines: usize) -> usize {
    let line_cap = diff_head_lines.min(file_lines.len());
    let mut cumulative_chars = 0usize;
    let mut count = 0usize;
    for line in file_lines.iter().take(line_cap) {
        let line_chars = line.chars().count().saturating_add(1);
        if cumulative_chars.saturating_add(line_chars) > MAX_HEAD_CONTEXT_CHARS {
            break;
        }
        cumulative_chars = cumulative_chars.saturating_add(line_chars);
        count += 1;
    }
    count
}

fn split_into_hunks(lines: Vec<ExhibitLine>) -> (Vec<ExhibitLine>, Vec<ParsedHunk>) {
    let mut leading = Vec::new();
    let mut hunks: Vec<ParsedHunk> = Vec::new();

    for line in lines {
        if line.is_hunk_header {
            let new_start = line.anchor_line.max(1);
            hunks.push(ParsedHunk {
                header: line,
                new_start,
                body: Vec::new(),
            });
        } else if let Some(current) = hunks.last_mut() {
            current.body.push(line);
        } else {
            leading.push(line);
        }
    }

    (leading, hunks)
}

fn emit_post_image_range(
    file_lines: &[&str],
    start_line: usize,
    end_line: usize,
    out: &mut Vec<ExhibitLine>,
) {
    if start_line > end_line || start_line == 0 {
        return;
    }
    let clamped_end = end_line.min(file_lines.len());
    for line_no in start_line..=clamped_end {
        let text = file_lines[line_no - 1];
        out.push(ExhibitLine {
            text: text.to_string(),
            rendered: format!(" {line_no:>4} | {text}"),
            target_line: Some(line_no),
            anchor_line: line_no,
            is_hunk_header: false,
            is_deletion: false,
        });
    }
}

fn make_elision_line(kind: SourceKind, start_line: usize, end_line: usize) -> ExhibitLine {
    let rendered = match kind {
        SourceKind::Markdown => format!("<!-- ... [L{start_line}..L{end_line} elided] -->"),
        _ => format!("// ... [L{start_line}..L{end_line} elided]"),
    };
    ExhibitLine {
        text: rendered.clone(),
        rendered,
        target_line: None,
        anchor_line: start_line,
        is_hunk_header: false,
        is_deletion: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_file_renders_complete_post_patch_annotated_diff_including_tail_rationale() {
        // Mirrors PR #453: 14-line canon edited on line 10; -U3 hunk stops at line 13,
        // cutting off `Rationale:` on line 14.
        let post_patch = "\
---
triggers:
  - \"docs/**/*.md\"
  - \"specs/**/*.md\"
inspect:
  - diff
tags:
  - documentation
---
Normative specifications MUST be single-sourced and delegated via hyperlink.

Exception: Brief informative mentions MAY appear inline.

Rationale: Separating normative definitions prevents silent divergence.";

        let patch = "\
@@ -7,7 +7,7 @@
 tags:
   - documentation
 ---
-Normative specifications MUST be single-sourced.
+Normative specifications MUST be single-sourced and delegated via hyperlink.
 
 Exception: Brief informative mentions MAY appear inline.
 ";

        let rendered = format_diff_patch_with_head_context(DiffHeadExpandInput {
            path: "builtins/documentation/example.canon.md",
            patch,
            post_patch_content: Some(post_patch),
            budget: DiffHeadBudget(DEFAULT_DIFF_HEAD_LINES),
        });

        assert!(rendered.contains("    1 | ---"));
        assert!(rendered.contains("    - | Normative specifications MUST be single-sourced."));
        assert!(rendered.contains(
            "+  10 | Normative specifications MUST be single-sourced and delegated via hyperlink."
        ));
        assert!(rendered.contains(
            "   14 | Rationale: Separating normative definitions prevents silent divergence."
        ));
        assert!(
            !rendered.contains("elided"),
            "small file <= N lines must not emit elision markers:\n{rendered}"
        );
        assert!(
            !rendered.contains("@@"),
            "small file merged seamlessly from line 1 should omit @@ header:\n{rendered}"
        );
    }

    #[test]
    fn large_file_with_non_overlapping_hunk_emits_head_prefix_elision_and_hunk() {
        let mut post_lines = Vec::new();
        for i in 1..=120 {
            post_lines.push(format!("fn line_{i}() {{}}"));
        }
        let post_patch = post_lines.join("\n");
        let patch = "\
@@ -100,4 +100,4 @@
 fn line_100() {}
-fn old_101() {}
+fn line_101() {}
 fn line_102() {}";

        let rendered = format_diff_patch_with_head_context(DiffHeadExpandInput {
            path: "src/lib.rs",
            patch,
            post_patch_content: Some(&post_patch),
            budget: DiffHeadBudget(20),
        });

        assert!(rendered.contains("    1 | fn line_1() {}"));
        assert!(rendered.contains("   20 | fn line_20() {}"));
        assert!(!rendered.contains("   21 | fn line_21() {}"));
        assert!(rendered.contains("// ... [L21..L99 elided]"));
        assert!(rendered.contains("@@ -100,4 +100,4 @@"));
        assert!(rendered.contains("+ 101 | fn line_101() {}"));
    }

    #[test]
    fn large_markdown_file_uses_html_comment_elision_marker() {
        let mut post_lines = Vec::new();
        for i in 1..=50 {
            post_lines.push(format!("Line {i}"));
        }
        let post_patch = post_lines.join("\n");
        let patch = "\
@@ -40,3 +40,3 @@
 Line 40
-Old 41
+Line 41
 Line 42";

        let rendered = format_diff_patch_with_head_context(DiffHeadExpandInput {
            path: "README.md",
            patch,
            post_patch_content: Some(&post_patch),
            budget: DiffHeadBudget(10),
        });

        assert!(rendered.contains("    1 | Line 1"));
        assert!(rendered.contains("   10 | Line 10"));
        assert!(rendered.contains("<!-- ... [L11..L39 elided] -->"));
        assert!(rendered.contains("+  41 | Line 41"));
    }

    #[test]
    fn large_file_with_overlapping_and_post_head_hunks_never_duplicates_lines() {
        let mut post_lines = Vec::new();
        for i in 1..=60 {
            post_lines.push(format!("let x_{i} = {i};"));
        }
        let post_patch = post_lines.join("\n");
        let patch = "\
@@ -3,3 +3,3 @@
 let x_3 = 3;
-let x_4 = 0;
+let x_4 = 4;
 let x_5 = 5;
@@ -45,3 +45,3 @@
 let x_45 = 45;
-let x_46 = 0;
+let x_46 = 46;
 let x_47 = 47;";

        let lines = expand_diff_patch_lines(DiffHeadExpandInput {
            path: "src/app.rs",
            patch,
            post_patch_content: Some(&post_patch),
            budget: DiffHeadBudget(10),
        });
        let rendered = render_exhibit_lines(&lines);

        // Line 4 must appear exactly once (as the added line).
        let line_4_count = lines.iter().filter(|l| l.target_line == Some(4)).count();
        assert_eq!(
            line_4_count, 1,
            "line 4 must not be duplicated:\n{rendered}"
        );
        assert!(rendered.contains("    1 | let x_1 = 1;"));
        assert!(rendered.contains("+   4 | let x_4 = 4;"));
        assert!(rendered.contains("   10 | let x_10 = 10;"));
        assert!(rendered.contains("// ... [L11..L44 elided]"));
        assert!(rendered.contains("+  46 | let x_46 = 46;"));
    }

    #[test]
    fn large_file_with_only_head_hunk_emits_trailing_elision_marker() {
        let mut post_lines = Vec::new();
        for i in 1..=100 {
            post_lines.push(format!("line {i}"));
        }
        let post_patch = post_lines.join("\n");
        let patch = "\
@@ -2,3 +2,3 @@
 line 2
-old 3
+line 3
 line 4";

        let rendered = format_diff_patch_with_head_context(DiffHeadExpandInput {
            path: "src/mod.rs",
            patch,
            post_patch_content: Some(&post_patch),
            budget: DiffHeadBudget(10),
        });

        assert!(rendered.contains("    1 | line 1"));
        assert!(rendered.contains("+   3 | line 3"));
        assert!(rendered.contains("   10 | line 10"));
        assert!(rendered.contains("// ... [L11..L100 elided]"));
    }

    #[test]
    fn minified_single_line_file_exceeding_char_cap_falls_back_to_raw_hunk() {
        let huge_line = "a".repeat(MAX_HEAD_CONTEXT_CHARS + 500);
        let patch = "@@ -1,1 +1,1 @@\n-old\n+new";

        let expanded = format_diff_patch_with_head_context(DiffHeadExpandInput {
            path: "dist/bundle.min.js",
            patch,
            post_patch_content: Some(&huge_line),
            budget: DiffHeadBudget(80),
        });
        let raw = render_exhibit_lines(&parse_unified_patch_lines(patch));
        assert_eq!(expanded, raw);
    }

    #[test]
    fn zero_budget_disables_head_context_expansion() {
        let post_patch = "line 1\nline 2\nline 3\nline 4\nline 5";
        let patch = "@@ -4,2 +4,2 @@\n-old 4\n+line 4\n line 5";

        let disabled = format_diff_patch_with_head_context(DiffHeadExpandInput {
            path: "src/a.rs",
            patch,
            post_patch_content: Some(post_patch),
            budget: DiffHeadBudget(0),
        });
        let raw = render_exhibit_lines(&parse_unified_patch_lines(patch));
        assert_eq!(disabled, raw);
        assert!(!disabled.contains("line 1"));
    }

    #[test]
    fn missing_post_patch_content_falls_back_to_raw_hunk() {
        let patch = "@@ -10,2 +10,2 @@\n-old 10\n+new 10\n line 11";
        let fallback = format_diff_patch_with_head_context(DiffHeadExpandInput {
            path: "src/a.rs",
            patch,
            post_patch_content: None,
            budget: DiffHeadBudget(80),
        });
        let raw = render_exhibit_lines(&parse_unified_patch_lines(patch));
        assert_eq!(fallback, raw);
    }
}
