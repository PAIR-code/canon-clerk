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

//! Strategy D pan-source breadcrumb skeleton builder for `audit` (`docs/architecture/nodes/audit.md` §3–§4).

use super::{
    ExhibitLine, SourceKind, is_preamble_line, is_toml_section_header, leading_indent,
    markdown_heading_levels,
};
use std::fmt::Write as _;

const MAX_CODE_PREAMBLE_LINES: usize = 40;
const MAX_MARKDOWN_PREAMBLE_LINES: usize = 30;
const MAX_CONFIG_PREAMBLE_LINES: usize = 20;
const MIN_ELIDED_RUN_LINES: usize = 3;

/// Renders a Strategy D breadcrumb skeleton for `lines` given 1-based inclusive `hot_spans`
/// (`(start_line, end_line)`), preserving exact 1-based target-file line numbers and eliding cold
/// regions with `// ... [La..Lb elided]` (`<!-- ... [La..Lb elided] -->` for Markdown).
///
/// Falls back to rendering all lines verbatim if `hot_spans` is empty or matches zero lines.
pub(crate) fn render_breadcrumb_skeleton(
    path: &str,
    lines: &[ExhibitLine],
    hot_spans: &[(usize, usize)],
) -> String {
    if lines.is_empty() {
        return String::new();
    }
    if hot_spans.is_empty() {
        return render_all_lines(lines);
    }

    let mut keep = vec![false; lines.len()];
    let mut hot_starts = Vec::new();
    let mut in_hot = false;

    for (idx, line) in lines.iter().enumerate() {
        let matched = line_in_hot_spans(line, hot_spans);
        if matched {
            keep[idx] = true;
            if !in_hot {
                hot_starts.push(idx);
                in_hot = true;
            }
        } else {
            in_hot = false;
        }
    }

    if hot_starts.is_empty() {
        return render_all_lines(lines);
    }

    let kind = SourceKind::from_path(path);
    mark_top_of_file_preamble(lines, kind, &mut keep);
    mark_ancestor_spines(lines, kind, &hot_starts, &mut keep);
    mark_enclosing_hunk_headers(lines, &mut keep);

    emit_skeleton_lines(lines, kind, &keep)
}

fn line_in_hot_spans(line: &ExhibitLine, hot_spans: &[(usize, usize)]) -> bool {
    let coord = line.target_line.unwrap_or(line.anchor_line);
    hot_spans
        .iter()
        .any(|&(start, end)| coord >= start && coord <= end)
}

fn render_all_lines(lines: &[ExhibitLine]) -> String {
    let mut out = String::with_capacity(lines.len().saturating_mul(48));
    for line in lines {
        let _ = writeln!(out, "{}", line.rendered);
    }
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

fn mark_top_of_file_preamble(lines: &[ExhibitLine], kind: SourceKind, keep: &mut [bool]) {
    match kind {
        SourceKind::Code => mark_code_preamble(lines, keep),
        SourceKind::Markdown => mark_markdown_preamble(lines, keep),
        SourceKind::Toml | SourceKind::Yaml => mark_config_preamble(lines, keep),
    }
}

fn mark_code_preamble(lines: &[ExhibitLine], keep: &mut [bool]) {
    let limit = lines.len().min(MAX_CODE_PREAMBLE_LINES);
    for (idx, line) in lines.iter().take(limit).enumerate() {
        if line.is_hunk_header || line.is_deletion {
            continue;
        }
        let trimmed = line.text.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.starts_with("#!")
            || trimmed.starts_with("use ")
            || trimmed.starts_with("pub use ")
            || trimmed.starts_with("import ")
            || trimmed.starts_with("from ")
            || trimmed.starts_with("extern crate ")
            || trimmed.starts_with("#include ")
            || trimmed.starts_with("package ")
            || (trimmed.starts_with("mod ") && trimmed.ends_with(';'))
        {
            keep[idx] = true;
        } else {
            break;
        }
    }
}

fn mark_markdown_preamble(lines: &[ExhibitLine], keep: &mut [bool]) {
    let limit = lines.len().min(MAX_MARKDOWN_PREAMBLE_LINES);
    let mut in_frontmatter = false;
    for (idx, line) in lines.iter().take(limit).enumerate() {
        if line.is_hunk_header || line.is_deletion {
            continue;
        }
        let trimmed = line.text.trim();
        if idx == 0 && trimmed == "---" {
            in_frontmatter = true;
            keep[idx] = true;
            continue;
        }
        if in_frontmatter {
            keep[idx] = true;
            if trimmed == "---" {
                in_frontmatter = false;
            }
            continue;
        }
        if trimmed.is_empty() {
            keep[idx] = true;
            continue;
        }
        if trimmed.starts_with("# ") {
            keep[idx] = true;
        }
        break;
    }
}

fn mark_config_preamble(lines: &[ExhibitLine], keep: &mut [bool]) {
    let limit = lines.len().min(MAX_CONFIG_PREAMBLE_LINES);
    for (idx, line) in lines.iter().take(limit).enumerate() {
        if line.is_hunk_header || line.is_deletion {
            continue;
        }
        let trimmed = line.text.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            keep[idx] = true;
        } else if is_toml_section_header(trimmed) {
            break;
        } else {
            keep[idx] = true;
        }
    }
}

fn mark_ancestor_spines(
    lines: &[ExhibitLine],
    kind: SourceKind,
    hot_starts: &[usize],
    keep: &mut [bool],
) {
    match kind {
        SourceKind::Code | SourceKind::Yaml => {
            for &hot_start in hot_starts {
                mark_indentation_spine(lines, kind, hot_start, keep);
            }
        }
        SourceKind::Markdown => {
            let headings = markdown_heading_levels(lines);
            for &hot_start in hot_starts {
                mark_markdown_heading_spine(&headings, hot_start, keep);
            }
        }
        SourceKind::Toml => {
            for &hot_start in hot_starts {
                mark_toml_section_spine(lines, hot_start, keep);
            }
        }
    }
}

fn mark_indentation_spine(
    lines: &[ExhibitLine],
    kind: SourceKind,
    hot_start: usize,
    keep: &mut [bool],
) {
    let Some(mut cur_indent) = first_code_indent_at_or_after(lines, hot_start) else {
        return;
    };
    let mut idx = hot_start;
    while idx > 0 && cur_indent > 0 {
        idx -= 1;
        let line = &lines[idx];
        if line.is_deletion || line.is_hunk_header {
            continue;
        }
        let trimmed = line.text.trim();
        if trimmed.is_empty() || is_closing_brace_line(trimmed) {
            continue;
        }
        let ind = leading_indent(&line.text);
        if ind < cur_indent {
            keep[idx] = true;
            mark_attached_preamble_above(lines, kind, idx, ind, keep);
            cur_indent = ind;
        }
    }
}

fn first_code_indent_at_or_after(lines: &[ExhibitLine], start: usize) -> Option<usize> {
    for line in &lines[start..] {
        if !line.is_deletion && !line.is_hunk_header && !line.text.trim().is_empty() {
            return Some(leading_indent(&line.text));
        }
    }
    None
}

fn is_closing_brace_line(trimmed: &str) -> bool {
    matches!(trimmed, "}" | "};" | "}," | ")" | ");" | "]" | "];")
}

fn mark_attached_preamble_above(
    lines: &[ExhibitLine],
    kind: SourceKind,
    decl_idx: usize,
    decl_indent: usize,
    keep: &mut [bool],
) {
    let mut idx = decl_idx;
    while idx > 0 {
        idx -= 1;
        let line = &lines[idx];
        if line.is_deletion || line.is_hunk_header {
            break;
        }
        let trimmed = line.text.trim();
        if !is_preamble_line(trimmed, kind) || leading_indent(&line.text) != decl_indent {
            break;
        }
        keep[idx] = true;
    }
}

fn mark_markdown_heading_spine(headings: &[Option<usize>], hot_start: usize, keep: &mut [bool]) {
    let mut min_level = headings
        .get(hot_start)
        .copied()
        .flatten()
        .unwrap_or(usize::MAX);
    let mut idx = hot_start;
    while idx > 0 && min_level > 1 {
        idx -= 1;
        if let Some(level) = headings[idx]
            && level < min_level
        {
            keep[idx] = true;
            min_level = level;
        }
    }
}

fn mark_toml_section_spine(lines: &[ExhibitLine], hot_start: usize, keep: &mut [bool]) {
    let mut idx = hot_start + 1;
    while idx > 0 {
        idx -= 1;
        let line = &lines[idx];
        if line.is_deletion || line.is_hunk_header {
            continue;
        }
        if is_toml_section_header(line.text.trim()) {
            keep[idx] = true;
            mark_attached_preamble_above(lines, SourceKind::Toml, idx, 0, keep);
            break;
        }
    }
}

fn mark_enclosing_hunk_headers(lines: &[ExhibitLine], keep: &mut [bool]) {
    let mut current_hunk: Option<usize> = None;
    for (idx, line) in lines.iter().enumerate() {
        if line.is_hunk_header {
            current_hunk = Some(idx);
        } else if keep[idx]
            && let Some(hunk_idx) = current_hunk
        {
            keep[hunk_idx] = true;
        }
    }
}

fn emit_skeleton_lines(lines: &[ExhibitLine], kind: SourceKind, keep: &[bool]) -> String {
    let mut out = String::with_capacity(lines.len().saturating_mul(32));
    let mut idx = 0usize;

    while idx < lines.len() {
        if keep[idx] {
            let _ = writeln!(out, "{}", lines[idx].rendered);
            idx += 1;
            continue;
        }

        let run_start = idx;
        while idx < lines.len() && !keep[idx] {
            idx += 1;
        }
        let run_len = idx - run_start;
        if run_len < MIN_ELIDED_RUN_LINES {
            for line in &lines[run_start..idx] {
                let _ = writeln!(out, "{}", line.rendered);
            }
        } else {
            let (elide_start, elide_end) = elided_line_bounds(&lines[run_start..idx]);
            match kind {
                SourceKind::Markdown => {
                    let _ = writeln!(out, "<!-- ... [L{elide_start}..L{elide_end} elided] -->");
                }
                _ => {
                    let _ = writeln!(out, "// ... [L{elide_start}..L{elide_end} elided]");
                }
            }
        }
    }

    if out.ends_with('\n') {
        out.pop();
    }
    out
}

fn elided_line_bounds(run: &[ExhibitLine]) -> (usize, usize) {
    let start = run
        .iter()
        .find_map(|l| l.target_line)
        .or_else(|| run.first().map(|l| l.anchor_line))
        .unwrap_or(1);
    let end = run
        .iter()
        .rev()
        .find_map(|l| l.target_line)
        .or_else(|| run.last().map(|l| l.anchor_line))
        .unwrap_or(start);
    (start.max(1), end.max(start).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spans::{parse_snapshot_lines, parse_unified_patch_lines};

    #[test]
    fn test_rust_breadcrumb_skeleton_preserves_imports_cfg_test_mod_impl_fn_and_line_numbers() {
        let mut src = String::from(
            "use std::collections::BTreeMap;\n\nstruct Service;\n\nimpl Service {\n    fn clean_helper() -> usize {\n",
        );
        for i in 1..=20 {
            src.push_str(&format!("        let clean_{i} = {i};\n"));
        }
        src.push_str(
            "        0\n    }\n}\n\n#[cfg(test)]\nmod tests {\n    fn cold_test() {\n",
        );
        for i in 1..=20 {
            src.push_str(&format!("        let cold_{i} = {i};\n"));
        }
        src.push_str(
            "    }\n\n    fn hot_test() {\n        let violation = \"BAD_SENTINEL\";\n        assert!(!violation.is_empty());\n    }\n}\n",
        );

        let lines = parse_snapshot_lines(&src);
        let hot_line = lines
            .iter()
            .find(|l| l.text.contains("BAD_SENTINEL"))
            .and_then(|l| l.target_line)
            .expect("hot line");
        let rendered =
            render_breadcrumb_skeleton("src/service.rs", &lines, &[(hot_line, hot_line + 1)]);

        assert!(rendered.contains("   1 | use std::collections::BTreeMap;"));
        assert!(rendered.contains("#[cfg(test)]"));
        assert!(rendered.contains("mod tests {"));
        assert!(rendered.contains("fn hot_test() {"));
        assert!(rendered.contains(&format!("{hot_line:>4} |         let violation = \"BAD_SENTINEL\";")));
        assert!(rendered.contains("// ... [L"));
        assert!(!rendered.contains("let clean_10 = 10;"));
        assert!(!rendered.contains("let cold_10 = 10;"));
    }

    #[test]
    fn test_python_and_typescript_indentation_ancestor_spines() {
        let mut py = String::from("import os\n\nclass Handler:\n    def cold(self):\n");
        for i in 1..=15 {
            py.push_str(&format!("        x_{i} = {i}\n"));
        }
        py.push_str("    @staticmethod\n    def hot():\n        return eval('1+1')\n");
        let py_lines = parse_snapshot_lines(&py);
        let eval_ln = py_lines
            .iter()
            .find(|l| l.text.contains("eval"))
            .and_then(|l| l.target_line)
            .expect("eval line");
        let py_out = render_breadcrumb_skeleton("app/handler.py", &py_lines, &[(eval_ln, eval_ln)]);
        assert!(py_out.contains("   1 | import os"));
        assert!(py_out.contains("class Handler:"));
        assert!(py_out.contains("@staticmethod"));
        assert!(py_out.contains("def hot():"));
        assert!(py_out.contains("// ... [L"));
    }

    #[test]
    fn test_markdown_monotonic_heading_stack_ignores_fenced_code_block_hashes() {
        let md = "\
# Root Title

Intro paragraph.

## Section A

```bash
# Fake heading inside code fence
echo hello
echo 1
echo 2
echo 3
echo 4
```

### Subsection B

Cold line 1
Cold line 2
Cold line 3
Cold line 4
Cold line 5

HOT_VIOLATION_LINE
";
        let lines = parse_snapshot_lines(md);
        let hot_ln = lines
            .iter()
            .find(|l| l.text.contains("HOT_VIOLATION_LINE"))
            .and_then(|l| l.target_line)
            .expect("hot line");
        let rendered = render_breadcrumb_skeleton("docs/guide.md", &lines, &[(hot_ln, hot_ln)]);

        assert!(rendered.contains("   1 | # Root Title"));
        assert!(rendered.contains("## Section A"));
        assert!(rendered.contains("### Subsection B"));
        assert!(!rendered.contains("# Fake heading inside code fence"));
        assert!(rendered.contains("<!-- ... [L"));
        assert!(rendered.contains("HOT_VIOLATION_LINE"));
    }

    #[test]
    fn test_toml_and_yaml_section_breadcrumbs() {
        let toml = "\
# Top comment
[package]
name = \"demo\"
version = \"0.1.0\"
edition = \"2024\"
description = \"cold\"

[lints.rust]
cold_1 = \"warn\"
cold_2 = \"warn\"
cold_3 = \"warn\"
cold_4 = \"warn\"
unsafe_code = \"allow\"
";
        let toml_lines = parse_snapshot_lines(toml);
        let unsafe_ln = toml_lines
            .iter()
            .find(|l| l.text.contains("unsafe_code"))
            .and_then(|l| l.target_line)
            .expect("unsafe_code");
        let toml_out =
            render_breadcrumb_skeleton("Cargo.toml", &toml_lines, &[(unsafe_ln, unsafe_ln)]);
        assert!(toml_out.contains("[lints.rust]"));
        assert!(toml_out.contains("unsafe_code = \"allow\""));
        assert!(toml_out.contains("// ... [L"));

        let yaml = "\
services:
  api:
    env:
      COLD_A: 1
      COLD_B: 2
      COLD_C: 3
      COLD_D: 4
      SECRET_KEY: hardcoded
";
        let yaml_lines = parse_snapshot_lines(yaml);
        let secret_ln = yaml_lines
            .iter()
            .find(|l| l.text.contains("SECRET_KEY"))
            .and_then(|l| l.target_line)
            .expect("secret");
        let yaml_out =
            render_breadcrumb_skeleton("compose.yaml", &yaml_lines, &[(secret_ln, secret_ln)]);
        assert!(yaml_out.contains("services:"));
        assert!(yaml_out.contains("  api:"));
        assert!(yaml_out.contains("    env:"));
        assert!(yaml_out.contains("SECRET_KEY: hardcoded"));
    }

    #[test]
    fn test_diff_exhibit_preserves_hunk_header_and_ignores_deletions_in_spine() {
        let mut patch = String::from("@@ -1,4 +1,22 @@ fn process()\n fn process() {\n");
        patch.push_str("-    old_deleted_top_level_noise()\n");
        for i in 1..=15 {
            patch.push_str(&format!("+    let cold_{i} = {i};\n"));
        }
        patch.push_str("+    if cond {\n+        bad_call();\n+    }\n }\n");
        let lines = parse_unified_patch_lines(&patch);
        let bad_ln = lines
            .iter()
            .find(|l| l.text.contains("bad_call"))
            .and_then(|l| l.target_line)
            .expect("bad_call");
        let rendered = render_breadcrumb_skeleton("src/diff.rs", &lines, &[(bad_ln, bad_ln)]);
        assert!(rendered.contains("@@ -1,4 +1,22 @@ fn process()"));
        assert!(rendered.contains("fn process() {"));
        assert!(rendered.contains("if cond {"));
        assert!(rendered.contains(&format!("+{bad_ln:>4} |         bad_call();")));
        assert!(!rendered.contains("let cold_8 = 8;"));
    }
}
