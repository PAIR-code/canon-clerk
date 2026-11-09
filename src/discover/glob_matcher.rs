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

//! Pure POSIX path normalization, scope containment, and glob matching (`SPEC.md` §3.2, §4.2).

use crate::canon::CanonScope;
use std::fmt;

/// Default noise directories excluded from workspace traversal and discovery.
pub const DEFAULT_IGNORES: &[&str] = &["node_modules", "dist", ".bare", ".git", ".turbo", "target"];

/// Default glob pattern used to discover repository Canon rule files.
pub const DEFAULT_CANON_GLOB: &str = "**/.canons/**/*.md";

/// Syntax error encountered while compiling a glob pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobSyntaxError {
    /// Raw pattern string that failed validation.
    pub pattern: String,
    /// Human-readable description of the syntax error.
    pub message: String,
}

impl fmt::Display for GlobSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Invalid glob pattern `{}`: {}",
            self.pattern, self.message
        )
    }
}

impl std::error::Error for GlobSyntaxError {}

/// Compiled glob pattern supporting `**`, `*`, `?`, `[...]`, and `{a,b}` brace expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobPattern {
    raw: String,
    variants: Vec<Vec<String>>,
}

impl GlobPattern {
    /// Parses and validates a glob pattern string.
    pub fn parse(raw: &str) -> Result<Self, GlobSyntaxError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(GlobSyntaxError {
                pattern: raw.to_string(),
                message: "glob pattern must not be empty".to_string(),
            });
        }

        validate_delimiters(trimmed)?;
        let normalized = normalize_glob_str(trimmed);
        let expanded = expand_braces(&normalized);
        let variants = expanded
            .into_iter()
            .map(|variant| {
                let clean = normalize_glob_str(&variant);
                if clean == "**/*" || clean == "**" {
                    vec!["**".to_string()]
                } else {
                    clean
                        .split('/')
                        .filter(|s| !s.is_empty())
                        .map(ToOwned::to_owned)
                        .collect()
                }
            })
            .collect();

        Ok(Self {
            raw: trimmed.to_string(),
            variants,
        })
    }

    /// Returns the original trimmed pattern string.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Evaluates whether a workspace-relative POSIX path matches this glob pattern (with dotfile support).
    pub fn is_match(&self, path: &str) -> bool {
        let clean_path = normalize_glob_str(path);
        if clean_path.is_empty() || clean_path == "." {
            return false;
        }
        let path_segments: Vec<&str> = clean_path.split('/').filter(|s| !s.is_empty()).collect();
        if path_segments.is_empty() {
            return false;
        }

        self.variants
            .iter()
            .any(|variant| match_segments(variant, &path_segments))
    }
}

/// Returns `true` if the pattern contains glob wildcard or expansion metacharacters (`*`, `?`, `[`, `{`).
pub fn is_wildcard_pattern(pattern: &str) -> bool {
    pattern.contains(['*', '?', '[', '{'])
}

/// Returns `true` if any path component of `rel_path` matches [`DEFAULT_IGNORES`].
pub fn is_ignored_path(rel_path: &str) -> bool {
    let posix = rel_path.replace('\\', "/");
    posix
        .split('/')
        .filter(|seg| !seg.is_empty() && *seg != ".")
        .any(|seg| DEFAULT_IGNORES.contains(&seg))
}

/// Normalizes a workspace-relative path to POSIX separators and rejects `..` traversal escaping the workspace root.
pub fn normalize_workspace_path(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("path must not be empty".to_string());
    }
    let posix = trimmed.replace('\\', "/");
    if posix.starts_with('/') {
        return Err(format!("absolute path `{raw}` is not workspace-relative"));
    }

    let mut stack: Vec<&str> = Vec::new();
    for part in posix.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(format!("path `{raw}` escapes workspace root"));
                }
            }
            seg => stack.push(seg),
        }
    }

    if stack.is_empty() {
        Ok(".".to_string())
    } else {
        Ok(stack.join("/"))
    }
}

/// Checks if `target_path` resides within `scope` (or anywhere in the workspace when `scope` is `None`),
/// returning the scope-relative path when in scope.
pub fn scope_relative_path(target_path: &str, scope: Option<&CanonScope>) -> Option<String> {
    let clean_target = normalize_glob_str(target_path);
    if clean_target.is_empty() {
        return None;
    }

    match scope {
        None => Some(clean_target),
        Some(CanonScope(scope_prefix)) => {
            let clean_scope = scope_prefix.trim_matches('/');
            if clean_scope.is_empty() || clean_scope == "." {
                return Some(clean_target);
            }
            let prefix_with_slash = format!("{clean_scope}/");
            clean_target
                .strip_prefix(&prefix_with_slash)
                .filter(|rel| !rel.is_empty())
                .map(ToOwned::to_owned)
        }
    }
}

fn normalize_glob_str(raw: &str) -> String {
    let posix = raw.trim().replace('\\', "/");
    let mut s = posix.as_str();
    while let Some(rest) = s.strip_prefix("./") {
        s = rest;
    }
    s.trim_matches('/').to_string()
}

fn validate_delimiters(pattern: &str) -> Result<(), GlobSyntaxError> {
    let mut in_brackets = false;
    let mut brace_depth: usize = 0;
    let mut escaped = false;

    for ch in pattern.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        match ch {
            '[' if !in_brackets => in_brackets = true,
            ']' if in_brackets => in_brackets = false,
            '{' if !in_brackets => brace_depth += 1,
            '}' if !in_brackets => {
                if brace_depth == 0 {
                    return Err(GlobSyntaxError {
                        pattern: pattern.to_string(),
                        message: "unmatched closing brace `}`".to_string(),
                    });
                }
                brace_depth -= 1;
            }
            _ => {}
        }
    }

    if in_brackets {
        return Err(GlobSyntaxError {
            pattern: pattern.to_string(),
            message: "unclosed character class `[`".to_string(),
        });
    }
    if brace_depth > 0 {
        return Err(GlobSyntaxError {
            pattern: pattern.to_string(),
            message: "unclosed brace expansion `{`".to_string(),
        });
    }
    Ok(())
}

fn expand_braces(pattern: &str) -> Vec<String> {
    let Some(open_idx) = pattern.find('{') else {
        return vec![pattern.to_string()];
    };

    let mut depth = 0;
    let mut close_idx = None;
    let mut top_commas = Vec::new();

    for (offset, ch) in pattern[open_idx..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close_idx = Some(open_idx + offset);
                    break;
                }
            }
            ',' if depth == 1 => top_commas.push(open_idx + offset),
            _ => {}
        }
    }

    let Some(close) = close_idx else {
        return vec![pattern.to_string()];
    };
    if top_commas.is_empty() {
        let inner = &pattern[open_idx + 1..close];
        let combined = format!("{}{}{}", &pattern[..open_idx], inner, &pattern[close + 1..]);
        return expand_braces(&combined);
    }

    let prefix = &pattern[..open_idx];
    let suffix = &pattern[close + 1..];
    let mut results = Vec::new();
    let mut start = open_idx + 1;
    for comma in top_commas {
        let alt = &pattern[start..comma];
        let candidate = format!("{prefix}{alt}{suffix}");
        results.extend(expand_braces(&candidate));
        start = comma + 1;
    }
    let last_alt = &pattern[start..close];
    let candidate = format!("{prefix}{last_alt}{suffix}");
    results.extend(expand_braces(&candidate));
    results
}

fn match_segments(pat: &[String], path: &[&str]) -> bool {
    if pat.is_empty() {
        return path.is_empty();
    }
    if pat[0] == "**" {
        if pat.len() == 1 {
            return true;
        }
        if match_segments(&pat[1..], path) {
            return true;
        }
        return !path.is_empty() && match_segments(pat, &path[1..]);
    }
    if path.is_empty() {
        return false;
    }
    let p_chars: Vec<char> = pat[0].chars().collect();
    let t_chars: Vec<char> = path[0].chars().collect();
    match_segment_chars(&p_chars, &t_chars) && match_segments(&pat[1..], &path[1..])
}

fn match_segment_chars(pat: &[char], text: &[char]) -> bool {
    if pat.is_empty() {
        return text.is_empty();
    }
    match pat[0] {
        '*' => {
            let mut idx = 1;
            while idx < pat.len() && pat[idx] == '*' {
                idx += 1;
            }
            if match_segment_chars(&pat[idx..], text) {
                return true;
            }
            !text.is_empty() && match_segment_chars(&pat[idx - 1..], &text[1..])
        }
        '?' => !text.is_empty() && match_segment_chars(&pat[1..], &text[1..]),
        '[' => {
            if text.is_empty() {
                return false;
            }
            if let Some((matched, next_pat_idx)) = match_char_class(&pat[1..], text[0]) {
                matched && match_segment_chars(&pat[1 + next_pat_idx..], &text[1..])
            } else {
                pat[0] == text[0] && match_segment_chars(&pat[1..], &text[1..])
            }
        }
        c => !text.is_empty() && c == text[0] && match_segment_chars(&pat[1..], &text[1..]),
    }
}

fn match_char_class(pat_after_open: &[char], target: char) -> Option<(bool, usize)> {
    let close_pos = pat_after_open.iter().position(|&c| c == ']')?;
    let class_chars = &pat_after_open[..close_pos];
    let (negated, items) =
        if !class_chars.is_empty() && (class_chars[0] == '!' || class_chars[0] == '^') {
            (true, &class_chars[1..])
        } else {
            (false, class_chars)
        };

    let mut found = false;
    let mut i = 0;
    while i < items.len() {
        if i + 2 < items.len() && items[i + 1] == '-' {
            if target >= items[i] && target <= items[i + 2] {
                found = true;
            }
            i += 3;
        } else {
            if items[i] == target {
                found = true;
            }
            i += 1;
        }
    }

    let result = if negated { !found } else { found };
    Some((result, close_pos + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_pattern_matches_doublestar_braces_and_char_classes() {
        let brace_glob = GlobPattern::parse("src/**/*.{rs,toml}").expect("valid brace glob");
        assert!(brace_glob.is_match("src/lib.rs"));
        assert!(brace_glob.is_match("src/discover/glob_matcher.rs"));
        assert!(brace_glob.is_match("src/nested/Cargo.toml"));
        assert!(!brace_glob.is_match("docs/README.md"));

        let class_glob = GlobPattern::parse("tests/test_[0-9].rs").expect("valid class glob");
        assert!(class_glob.is_match("tests/test_4.rs"));
        assert!(!class_glob.is_match("tests/test_a.rs"));
    }

    #[test]
    fn test_glob_syntax_error_on_unclosed_delimiters() {
        assert!(GlobPattern::parse("src/[unclosed.rs").is_err());
        assert!(GlobPattern::parse("src/{a,b.rs").is_err());
        assert!(GlobPattern::parse("src/extra}.rs").is_err());
        assert!(GlobPattern::parse("   ").is_err());
    }
}
