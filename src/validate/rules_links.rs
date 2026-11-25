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

//! Static Markdown body link coverage lint rule (`links-declared-in-references`) for Canon files (`docs/architecture/nodes/validate.md` §4).

use super::rules_body::mask_inline_code;
use crate::canon::{CanonSourceInfo, CanonToken, CanonTokenKind};
use crate::caseload::{AnnotationLevel, CodeAnnotation};
use crate::discover::GlobPattern;

/// Flags relative workspace Markdown links in canon body prose (outside fenced code blocks and inline code spans)
/// that are not covered by frontmatter `references:`.
pub(crate) fn check_links_declared_in_references(
    path: &str,
    source_info: Option<&CanonSourceInfo>,
    scope: Option<&str>,
    references: &[String],
    tokens: &[CanonToken],
    out: &mut Vec<CodeAnnotation>,
) {
    let compiled_refs: Vec<GlobPattern> = references
        .iter()
        .filter_map(|r| GlobPattern::parse(r).ok())
        .collect();

    let canon_dir = source_info
        .and_then(|info| info.file_path.as_deref())
        .and_then(|fp| fp.rsplit_once('/').map(|(dir, _)| dir));

    for token in tokens {
        if !matches!(
            token.kind,
            CanonTokenKind::Text { .. } | CanonTokenKind::Directive { .. }
        ) {
            continue;
        }
        let masked = mask_inline_code(&token.raw);
        for (start_col, end_col, raw_target) in extract_markdown_link_targets(&masked) {
            let Some(rel_target) = normalize_relative_link_target(&raw_target) else {
                continue;
            };
            let resolved = resolve_link_against_workspace(&rel_target, canon_dir, scope);
            let is_covered = compiled_refs
                .iter()
                .any(|glob| glob.is_match(&resolved) || glob.is_match(&rel_target));

            if !is_covered {
                out.push(CodeAnnotation {
                    path: path.to_string(),
                    start_line: token.span.start_line,
                    end_line: token.span.start_line,
                    start_column: Some(start_col),
                    end_column: Some(end_col),
                    annotation_level: AnnotationLevel::Warning,
                    message: format!(
                        "Relative Markdown link '{raw_target}' is not covered by frontmatter 'references:'; add '{resolved}' to 'references:' so the auditor receives the linked document."
                    ),
                    title: Some("links-declared-in-references".to_string()),
                });
            }
        }
    }
}

/// Extracts `(start_col, end_col, target)` for non-image Markdown inline links `[label](target)`.
fn extract_markdown_link_targets(masked_line: &str) -> Vec<(usize, usize, String)> {
    let chars: Vec<char> = masked_line.chars().collect();
    let mut results = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '[' && (i == 0 || chars[i - 1] != '!') {
            let link_start = i;
            let mut j = i + 1;
            let mut depth = 1usize;
            while j < chars.len() && depth > 0 {
                match chars[j] {
                    '[' => depth += 1,
                    ']' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            if depth == 0 && j < chars.len() && chars[j] == '(' {
                let target_start = j + 1;
                let mut k = target_start;
                let mut paren_depth = 1usize;
                while k < chars.len() && paren_depth > 0 {
                    match chars[k] {
                        '(' => paren_depth += 1,
                        ')' => paren_depth -= 1,
                        _ => {}
                    }
                    k += 1;
                }
                if paren_depth == 0 {
                    let raw_target: String = chars[target_start..k - 1].iter().collect();
                    results.push((link_start + 1, k, raw_target));
                    i = k;
                    continue;
                }
            }
        }
        i += 1;
    }

    results
}

fn normalize_relative_link_target(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let unwrapped = trimmed
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(trimmed);
    let url_part = unwrapped.split_whitespace().next()?;
    let without_frag = url_part.split('#').next().unwrap_or(url_part);
    let without_query = without_frag.split('?').next().unwrap_or(without_frag);
    if without_query.is_empty() {
        return None;
    }
    let lower = without_query.to_ascii_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("ftp://")
        || lower.starts_with("tel:")
        || lower.starts_with("data:")
    {
        return None;
    }
    // Reject any other URI scheme like `file:` or `vscode:` before a slash.
    if let Some(colon_pos) = without_query.find(':')
        && !without_query[..colon_pos].contains('/')
    {
        return None;
    }
    Some(without_query.to_string())
}

fn resolve_link_against_workspace(
    rel_target: &str,
    canon_dir: Option<&str>,
    scope: Option<&str>,
) -> String {
    let combined = if rel_target.starts_with("./") || rel_target.starts_with("../") {
        match canon_dir {
            Some(dir) if !dir.is_empty() => format!("{dir}/{rel_target}"),
            _ => rel_target.to_string(),
        }
    } else {
        rel_target.trim_start_matches('/').to_string()
    };

    let mut stack: Vec<&str> = Vec::new();
    for seg in combined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            other => stack.push(other),
        }
    }
    let normalized = stack.join("/");

    if let Some(sc) = scope {
        let clean_scope = sc.trim_matches('/');
        if !clean_scope.is_empty()
            && let Some(stripped) = normalized.strip_prefix(&format!("{clean_scope}/"))
        {
            return stripped.to_string();
        }
    }

    normalized
}
