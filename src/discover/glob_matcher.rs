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
use globset::{GlobBuilder, GlobMatcher};
use std::fmt;

/// Default noise directories excluded from workspace traversal and discovery.
pub const DEFAULT_IGNORES: &[&str] = &["node_modules", "dist", ".bare", ".git", ".turbo", "target"];

/// Default glob pattern used to discover repository Canon rule files.
pub const DEFAULT_CANON_GLOB: &str = "**/.canons/**/*.canon.md";

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
#[derive(Debug, Clone)]
pub struct GlobPattern {
    raw: String,
    matcher: GlobMatcher,
}

impl PartialEq for GlobPattern {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl Eq for GlobPattern {}

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

        let normalized = normalize_glob_str(trimmed);
        let matcher = GlobBuilder::new(&normalized)
            .literal_separator(true)
            .build()
            .map_err(|err| GlobSyntaxError {
                pattern: raw.to_string(),
                message: err.kind().to_string(),
            })?
            .compile_matcher();

        Ok(Self {
            raw: trimmed.to_string(),
            matcher,
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
        self.matcher.is_match(&clean_path)
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
    let trimmed = s.trim_matches('/');
    if trimmed.contains("//") {
        trimmed
            .split('/')
            .filter(|seg| !seg.is_empty())
            .collect::<Vec<_>>()
            .join("/")
    } else {
        trimmed.to_string()
    }
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

        let dotfile_glob = GlobPattern::parse(DEFAULT_CANON_GLOB).expect("valid default glob");
        assert!(dotfile_glob.is_match(".canons/rule.canon.md"));
        assert!(dotfile_glob.is_match("pkg/.canons/sub/rule.canon.md"));
        assert!(!dotfile_glob.is_match("src/lib.rs"));
    }

    #[test]
    fn test_glob_syntax_error_on_unclosed_delimiters() {
        assert!(GlobPattern::parse("src/[unclosed.rs").is_err());
        assert!(GlobPattern::parse("src/{a,b.rs").is_err());
        assert!(GlobPattern::parse("src/extra}.rs").is_err());
        assert!(GlobPattern::parse("   ").is_err());
    }
}
