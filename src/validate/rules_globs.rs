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

//! Static glob hygiene lint rules for Canon frontmatter path lists (`docs/architecture/nodes/validate.md` §4).

use crate::canon::{FrontmatterKeySpan, SourceSpan};
use crate::caseload::{AnnotationLevel, CodeAnnotation};
use crate::discover::GlobPattern;
use std::collections::{BTreeMap, BTreeSet};

/// Evaluates `prefer-brace-alternation`, `no-duplicate-globs`, and `no-subsumed-globs`
/// across a sequence of frontmatter glob patterns (`triggers`, `exists`, `references`).
pub(crate) fn check_glob_list_hygiene(
    path: &str,
    entry: &FrontmatterKeySpan,
    patterns: &[(usize, &str)],
    out: &mut Vec<CodeAnnotation>,
) {
    if patterns.len() < 2 {
        return;
    }

    check_prefer_brace_alternation(path, entry, patterns, out);
    check_duplicate_globs(path, entry, patterns, out);
    check_subsumed_globs(path, entry, patterns, out);
}

fn check_prefer_brace_alternation(
    path: &str,
    entry: &FrontmatterKeySpan,
    patterns: &[(usize, &str)],
    out: &mut Vec<CodeAnnotation>,
) {
    // 1. Primary: Extension-glob grouping (prefix ending in '*', e.g. "**/*.rs" -> ("**/*.","rs")).
    let mut ext_groups: BTreeMap<&str, Vec<(usize, &str)>> = BTreeMap::new();
    let mut ext_matched_indices: BTreeSet<usize> = BTreeSet::new();

    for &(idx, pat) in patterns {
        if pat.starts_with('!') || pat.contains('{') || pat.contains('}') {
            continue;
        }
        if let Some((prefix, ext)) = split_star_extension_glob(pat) {
            ext_groups.entry(prefix).or_default().push((idx, ext));
        }
    }

    for (prefix, items) in ext_groups {
        let unique_exts: BTreeSet<&str> = items.iter().map(|(_, ext)| *ext).collect();
        if unique_exts.len() >= 2 {
            for &(idx, _) in &items {
                ext_matched_indices.insert(idx);
            }
            let first_idx = items[0].0;
            let span = item_or_value_span(entry, first_idx);
            let joined: Vec<&str> = unique_exts.into_iter().collect();
            let suggestion = format!("{prefix}{{{}}}", joined.join(","));
            out.push(make_warning(
                path,
                span,
                "prefer-brace-alternation",
                format!(
                    "Uncollapsed sibling extension globs in '{}' share prefix '{prefix}'; collapse into '{suggestion}'.",
                    entry.key
                ),
            ));
        }
    }

    // 2. Secondary: Single-segment sibling grouping (threshold >= 3) sharing (parent_dir, suffix).
    let mut sibling_groups: BTreeMap<(&str, &str), Vec<(usize, &str)>> = BTreeMap::new();

    for &(idx, pat) in patterns {
        if ext_matched_indices.contains(&idx)
            || pat.starts_with('!')
            || pat.contains('{')
            || pat.contains('}')
            || pat.contains('*')
            || pat.contains('?')
            || pat.contains('[')
        {
            continue;
        }
        if let Some((parent, stem, suffix)) = split_literal_sibling(pat) {
            sibling_groups
                .entry((parent, suffix))
                .or_default()
                .push((idx, stem));
        }
    }

    for ((parent, suffix), items) in sibling_groups {
        let unique_stems: BTreeSet<&str> = items.iter().map(|(_, stem)| *stem).collect();
        if unique_stems.len() >= 3 {
            let first_idx = items[0].0;
            let span = item_or_value_span(entry, first_idx);
            let joined: Vec<&str> = unique_stems.into_iter().collect();
            let suggestion = format!("{parent}{{{}}}{suffix}", joined.join(","));
            out.push(make_warning(
                path,
                span,
                "prefer-brace-alternation",
                format!(
                    "Uncollapsed sibling paths in '{}' can be collapsed into '{suggestion}'.",
                    entry.key
                ),
            ));
        }
    }
}

fn split_star_extension_glob(pattern: &str) -> Option<(&str, &str)> {
    let dot_pos = pattern.rfind('.')?;
    let before_dot = &pattern[..dot_pos];
    let ext = &pattern[dot_pos + 1..];
    if !before_dot.ends_with('*')
        || ext.is_empty()
        || !ext.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return None;
    }
    Some((&pattern[..=dot_pos], ext))
}

fn split_literal_sibling(pattern: &str) -> Option<(&str, &str, &str)> {
    let (parent, filename) = match pattern.rfind('/') {
        Some(pos) => (&pattern[..=pos], &pattern[pos + 1..]),
        None => ("", pattern),
    };
    let dot_pos = filename.rfind('.')?;
    let stem = &filename[..dot_pos];
    let suffix = &filename[dot_pos..];
    if stem.is_empty() || suffix.len() < 2 {
        return None;
    }
    Some((parent, stem, suffix))
}

fn check_duplicate_globs(
    path: &str,
    entry: &FrontmatterKeySpan,
    patterns: &[(usize, &str)],
    out: &mut Vec<CodeAnnotation>,
) {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut flagged_entries: BTreeSet<usize> = BTreeSet::new();

    for &(idx, pat) in patterns {
        let expanded = expand_simple_braces(pat);
        let mut duplicates = Vec::new();
        let mut local_seen = BTreeSet::new();

        for exp in expanded {
            let normalized = normalize_pattern_for_compare(&exp);
            if !local_seen.insert(normalized.clone()) || seen.contains_key(&normalized) {
                duplicates.push(normalized);
            } else {
                seen.insert(normalized, idx);
            }
        }

        if !duplicates.is_empty() && flagged_entries.insert(idx) {
            let span = item_or_value_span(entry, idx);
            out.push(make_warning(
                path,
                span,
                "no-duplicate-globs",
                format!(
                    "Duplicate glob pattern '{}' in '{}' (duplicates: {}).",
                    pat,
                    entry.key,
                    duplicates.join(", ")
                ),
            ));
        }
    }
}

fn check_subsumed_globs(
    path: &str,
    entry: &FrontmatterKeySpan,
    patterns: &[(usize, &str)],
    out: &mut Vec<CodeAnnotation>,
) {
    // Collect unambiguous catch-all patterns: "**", "**/*", "<dir>/**", "<dir>/**/*".
    let mut catch_alls: Vec<(usize, &str, Option<String>)> = Vec::new();
    for &(idx, pat) in patterns {
        if pat.starts_with('!') {
            continue;
        }
        let norm = normalize_pattern_for_compare(pat);
        if norm == "**" || norm == "**/*" {
            catch_alls.push((idx, pat, None));
        } else if let Some(prefix) = norm
            .strip_suffix("/**/*")
            .or_else(|| norm.strip_suffix("/**"))
            && !prefix.contains(['*', '?', '[', '{'])
        {
            catch_alls.push((idx, pat, Some(format!("{prefix}/"))));
        }
    }

    if catch_alls.is_empty() {
        return;
    }

    for &(idx, pat) in patterns {
        if pat.starts_with('!') {
            continue;
        }
        let norm = normalize_pattern_for_compare(pat);
        for &(broad_idx, broad_pat, ref broad_prefix) in &catch_alls {
            if broad_idx == idx {
                continue;
            }
            let broad_norm = normalize_pattern_for_compare(broad_pat);
            if broad_norm == norm {
                continue;
            }
            let is_subsumed = match broad_prefix {
                None => true,
                Some(prefix) => norm.starts_with(prefix.as_str()),
            };
            if is_subsumed && GlobPattern::parse(pat).is_ok() {
                let span = item_or_value_span(entry, idx);
                out.push(make_warning(
                    path,
                    span,
                    "no-subsumed-globs",
                    format!(
                        "Glob pattern '{pat}' in '{}' is already subsumed by broader pattern '{broad_pat}'.",
                        entry.key
                    ),
                ));
                break;
            }
        }
    }
}

/// Expands single-level `{a,b}` brace groups into concrete pattern strings for duplicate detection.
/// Bounded to avoid combinatorial explosion on deeply nested or huge brace expressions.
fn expand_simple_braces(pattern: &str) -> Vec<String> {
    const MAX_EXPANSIONS: usize = 64;
    let mut current = vec![pattern.to_string()];

    loop {
        let mut next = Vec::new();
        let mut expanded_any = false;

        for item in &current {
            if let Some((open, close)) = find_innermost_brace_pair(item) {
                let prefix = &item[..open];
                let body = &item[open + 1..close];
                let suffix = &item[close + 1..];
                let alts: Vec<&str> = body.split(',').collect();
                if alts.len() >= 2 && next.len() + alts.len() <= MAX_EXPANSIONS {
                    expanded_any = true;
                    for alt in alts {
                        next.push(format!("{prefix}{alt}{suffix}"));
                    }
                    continue;
                }
            }
            next.push(item.clone());
        }

        current = next;
        if !expanded_any || current.len() >= MAX_EXPANSIONS {
            break;
        }
    }

    current
}

fn find_innermost_brace_pair(s: &str) -> Option<(usize, usize)> {
    let mut last_open = None;
    for (i, ch) in s.char_indices() {
        match ch {
            '{' => last_open = Some(i),
            '}' => {
                if let Some(open) = last_open {
                    return Some((open, i));
                }
            }
            _ => {}
        }
    }
    None
}

fn normalize_pattern_for_compare(raw: &str) -> String {
    let posix = raw.trim().replace('\\', "/");
    let mut s = posix.as_str();
    while let Some(rest) = s.strip_prefix("./") {
        s = rest;
    }
    s.trim_matches('/').to_string()
}

fn item_or_value_span(entry: &FrontmatterKeySpan, index: usize) -> SourceSpan {
    entry
        .item_spans
        .get(index)
        .copied()
        .unwrap_or(entry.value_span)
}

fn make_warning(path: &str, span: SourceSpan, title: &str, message: String) -> CodeAnnotation {
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
