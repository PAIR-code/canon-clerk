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

//! Pan-source topological span tiler and explicit lexical span fence renderer.

use super::{
    ExhibitLine, SourceKind, is_preamble_line, is_toml_section_header, leading_indent,
    markdown_heading_levels,
};
use std::fmt::Write as _;

const TARGET_MIN_SPAN_LINES: usize = 40;
const TARGET_IDEAL_SPAN_LINES: usize = 52;
const TARGET_MAX_SPAN_LINES: usize = 65;
const FALLBACK_OVERLAP_LINES: usize = 5;
const MIN_PREAMBLE_FLOOR_OFFSET: usize = 10;

/// One topologically tiled span within a file exhibit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TiledSpan {
    /// Inclusive start index into the exhibit's `ExhibitLine` slice.
    pub(crate) start_idx: usize,
    /// Exclusive end index into the exhibit's `ExhibitLine` slice.
    pub(crate) end_idx: usize,
    /// Inclusive 1-based post-image target-file start line (`>= 1`).
    pub(crate) start_line: usize,
    /// Inclusive 1-based post-image target-file end line (`>= start_line`).
    pub(crate) end_line: usize,
}

/// Tiles `lines` into ~40–65 line spans using blank-line, dedent, Markdown `#` heading, and
/// TOML `[section]` cut scoring, gluing contiguous preambles to the declaration below them,
/// with fixed-cap overlap fallback on unspaced blocks.
pub(crate) fn tile_exhibit_lines(path: &str, lines: &[ExhibitLine]) -> Vec<TiledSpan> {
    if lines.is_empty() {
        return Vec::new();
    }
    if lines.len() <= TARGET_MAX_SPAN_LINES {
        let (start_line, end_line) = span_line_bounds(lines);
        return vec![TiledSpan {
            start_idx: 0,
            end_idx: lines.len(),
            start_line,
            end_line,
        }];
    }

    let kind = SourceKind::from_path(path);
    let md_headings = if kind == SourceKind::Markdown {
        markdown_heading_levels(lines)
    } else {
        Vec::new()
    };

    let mut spans = Vec::new();
    let mut pos = 0usize;

    while pos < lines.len() {
        if lines.len().saturating_sub(pos) <= TARGET_MAX_SPAN_LINES {
            let (start_line, end_line) = span_line_bounds(&lines[pos..]);
            spans.push(TiledSpan {
                start_idx: pos,
                end_idx: lines.len(),
                start_line,
                end_line,
            });
            break;
        }

        let (cut_idx, used_overlap_fallback) = choose_cut_index(lines, pos, kind, &md_headings);
        let (start_line, end_line) = span_line_bounds(&lines[pos..cut_idx]);
        spans.push(TiledSpan {
            start_idx: pos,
            end_idx: cut_idx,
            start_line,
            end_line,
        });

        pos = if used_overlap_fallback {
            cut_idx.saturating_sub(FALLBACK_OVERLAP_LINES).max(pos + 1)
        } else {
            cut_idx
        };
    }

    spans
}

fn choose_cut_index(
    lines: &[ExhibitLine],
    pos: usize,
    kind: SourceKind,
    md_headings: &[Option<usize>],
) -> (usize, bool) {
    let min_cut = (pos + TARGET_MIN_SPAN_LINES).min(lines.len().saturating_sub(1));
    let max_cut = (pos + TARGET_MAX_SPAN_LINES).min(lines.len().saturating_sub(1));

    let mut best_cut = max_cut;
    let mut best_score = 0i32;

    for cut in min_cut..=max_cut {
        let score = score_cut_candidate(lines, pos, cut, kind, md_headings);
        if score > best_score {
            best_score = score;
            best_cut = cut;
        }
    }

    if best_score == 0 {
        return (max_cut, true);
    }

    let glued = glue_preamble_backward(lines, pos, best_cut, kind);
    (glued, false)
}

fn score_cut_candidate(
    lines: &[ExhibitLine],
    pos: usize,
    cut: usize,
    kind: SourceKind,
    md_headings: &[Option<usize>],
) -> i32 {
    let cur = &lines[cut];
    let prev = &lines[cut - 1];
    let cur_trimmed = cur.text.trim();
    let prev_trimmed = prev.text.trim();

    let mut score = 0i32;
    if cur.is_hunk_header {
        score += 100;
    }
    if kind == SourceKind::Markdown && md_headings.get(cut).copied().flatten().is_some() {
        score += 90;
    }
    if kind == SourceKind::Toml && !cur.is_deletion && is_toml_section_header(cur_trimmed) {
        score += 90;
    }
    if prev_trimmed.is_empty() && !cur_trimmed.is_empty() {
        score += 50;
    }
    if !cur.is_deletion && !cur_trimmed.is_empty() {
        let cur_indent = leading_indent(&cur.text);
        if cur_indent == 0 && !is_closing_only(cur_trimmed) {
            score += 40;
        } else if let Some(prev_indent) = previous_non_blank_indent(lines, cut)
            && cur_indent < prev_indent
        {
            score += 20;
        }
    }

    if score > 0 {
        let ideal = pos + TARGET_IDEAL_SPAN_LINES;
        let dist = cut.abs_diff(ideal) as i32;
        score += (6 - (dist / 4)).max(0);
    }
    score
}

fn previous_non_blank_indent(lines: &[ExhibitLine], cut: usize) -> Option<usize> {
    let mut idx = cut;
    while idx > 0 {
        idx -= 1;
        let line = &lines[idx];
        if !line.is_deletion && !line.is_hunk_header && !line.text.trim().is_empty() {
            return Some(leading_indent(&line.text));
        }
    }
    None
}

fn is_closing_only(trimmed: &str) -> bool {
    matches!(trimmed, "}" | "};" | "}," | ")" | ");" | "]" | "];")
}

fn glue_preamble_backward(
    lines: &[ExhibitLine],
    pos: usize,
    mut cut: usize,
    kind: SourceKind,
) -> usize {
    let floor = pos + MIN_PREAMBLE_FLOOR_OFFSET;
    while cut > floor {
        let prev = &lines[cut - 1];
        if prev.is_hunk_header || !is_preamble_line(prev.text.trim(), kind) {
            break;
        }
        cut -= 1;
    }
    cut
}

fn span_line_bounds(slice: &[ExhibitLine]) -> (usize, usize) {
    let mut min_target: Option<usize> = None;
    let mut max_target: Option<usize> = None;
    for line in slice {
        if let Some(ln) = line.target_line {
            min_target = Some(min_target.map_or(ln, |m| m.min(ln)));
            max_target = Some(max_target.map_or(ln, |m| m.max(ln)));
        }
    }
    match (min_target, max_target) {
        (Some(start), Some(end)) => (start.max(1), end.max(start).max(1)),
        _ => {
            let anchor = slice.first().map_or(1, |l| l.anchor_line.max(1));
            (anchor, anchor)
        }
    }
}

/// Renders `spans` of `lines` wrapped in explicit lexical span fences
/// (`=== [SPAN Sk: <path> L<start>..L<end>] ===` … `=== [END SPAN Sk] ===`),
/// returning the rendered body and `(span_id, start_line, end_line)` descriptors.
pub(crate) fn render_fenced_spans(
    path: &str,
    lines: &[ExhibitLine],
    spans: &[TiledSpan],
    next_span_num: &mut usize,
) -> (String, Vec<(String, usize, usize)>) {
    let mut out = String::with_capacity(lines.len().saturating_mul(48));
    let mut descriptors = Vec::with_capacity(spans.len());

    for span in spans {
        let span_id = format!("S{}", *next_span_num);
        *next_span_num += 1;
        let _ = writeln!(
            out,
            "=== [SPAN {span_id}: {path} L{}..L{}] ===",
            span.start_line, span.end_line
        );
        for line in &lines[span.start_idx..span.end_idx] {
            let _ = writeln!(out, "{}", line.rendered);
        }
        let _ = writeln!(out, "=== [END SPAN {span_id}] ===");
        descriptors.push((span_id, span.start_line, span.end_line));
    }

    if out.ends_with('\n') {
        out.pop();
    }
    (out, descriptors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spans::{parse_snapshot_lines, parse_unified_patch_lines};

    #[test]
    fn test_tiler_glues_rust_attributes_and_doc_comments_to_following_fn() {
        let mut src = String::new();
        for i in 1..=47 {
            src.push_str(&format!("fn filler_{i}() {{ let x = {i}; }}\n"));
        }
        src.push('\n');
        src.push_str("/// Important doc comment.\n");
        src.push_str("#[cfg(test)]\n");
        src.push_str("#[allow(dead_code)]\n");
        src.push_str("fn target_decl() {\n");
        for i in 1..=40 {
            src.push_str(&format!("    let y_{i} = {i};\n"));
        }
        src.push_str("}\n");

        let lines = parse_snapshot_lines(&src);
        let spans = tile_exhibit_lines("src/lib.rs", &lines);
        assert_eq!(spans.len(), 2);
        // The second span must start at the doc comment (line 49), not after the attributes.
        let second_first = &lines[spans[1].start_idx].text;
        assert_eq!(second_first, "/// Important doc comment.");
        assert_eq!(spans[1].start_line, 49);
    }

    #[test]
    fn test_tiler_uses_overlap_fallback_on_unspaced_indented_block() {
        let mut src = String::new();
        for i in 1..=140 {
            src.push_str(&format!("    let unspaced_{i} = {i};\n"));
        }
        let lines = parse_snapshot_lines(&src);
        let spans = tile_exhibit_lines("src/dense.rs", &lines);
        assert!(spans.len() >= 3);
        // Adjacent fallback spans must overlap by FALLBACK_OVERLAP_LINES.
        assert_eq!(
            spans[0].end_idx - spans[1].start_idx,
            FALLBACK_OVERLAP_LINES
        );
    }

    #[test]
    fn test_tiler_anchors_deleted_only_diff_span_to_hunk_post_image_line() {
        let patch = "@@ -10,3 +12,0 @@ fn removed()\n-let a = 1;\n-let b = 2;\n-let c = 3;\n";
        let lines = parse_unified_patch_lines(patch);
        let spans = tile_exhibit_lines("src/removed.rs", &lines);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start_line, 12);
        assert_eq!(spans[0].end_line, 12);
    }

    #[test]
    fn test_render_fenced_spans_emits_explicit_lexical_fences() {
        let mut src = String::new();
        for i in 1..=90 {
            src.push_str(&format!("let line_{i} = {i};\n"));
        }
        let lines = parse_snapshot_lines(&src);
        let spans = tile_exhibit_lines("src/app.rs", &lines);
        let mut next_id = 1;
        let (fenced, descs) = render_fenced_spans("src/app.rs", &lines, &spans, &mut next_id);
        assert_eq!(descs.len(), 2);
        assert_eq!(descs[0].0, "S1");
        assert_eq!(descs[1].0, "S2");
        assert!(fenced.contains("=== [SPAN S1: src/app.rs L1..L"));
        assert!(fenced.contains("=== [END SPAN S1] ==="));
        assert!(fenced.contains("=== [SPAN S2: src/app.rs L"));
        assert!(fenced.contains("=== [END SPAN S2] ==="));
    }
}
