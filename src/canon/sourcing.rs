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

//! Multi-realm sourcing, monorepo scope resolution, and path containment (`SPEC.md` §3).

use super::lexer::CanonParseError;
use serde::{Deserialize, Serialize};

/// Sourcing realm for a Canon rule (`SPEC.md` §3.1).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "kind", content = "pack", rename_all = "snake_case")]
pub enum CanonSourceKind {
    /// Local workspace canon authored in the repository (`.canons/...` or `<scope>/.canons/...`).
    Workspace,
    /// Built-in standard canon bundled in the binary (`builtin:<path>`).
    Builtin,
    /// External library canon pack (`<pack-alias>:<path>`).
    Pack(String),
}

/// Newtype wrapper representing a monorepo package scope prefix (e.g. `"packages/ui"`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CanonScope(pub String);

impl CanonScope {
    /// Returns the scope prefix string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Resolved sourcing provenance and path components for a Canon (`SPEC.md` §3.1, §4.2.1).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CanonSourceInfo {
    /// Source-qualified reference (`<workspace-path>`, `builtin:<path>`, or `<pack>:<path>`).
    pub source_ref: String,
    /// Sourcing tier classification.
    pub source_kind: CanonSourceKind,
    /// Workspace-relative POSIX file path when `source_kind` is [`CanonSourceKind::Workspace`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// Path relative to the nearest enclosing `.canons/` directory or pack root (without `.md`).
    pub diagnostic_path: String,
    /// File stem excluding directory components and `.md` extension.
    pub file_stem: String,
    /// Monorepo package scope when the canon resides under `<scope>/.canons/`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<CanonScope>,
}

fn strip_md_ext(path: &str) -> &str {
    if path.len() >= 3 && path[path.len() - 3..].eq_ignore_ascii_case(".md") {
        &path[..path.len() - 3]
    } else {
        path
    }
}

fn is_valid_pack_namespace(prefix: &str) -> bool {
    prefix.len() >= 2
        && !prefix.contains('/')
        && prefix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn extract_relative_to_canons(internal_path: &str) -> (Option<CanonScope>, String) {
    let clean = internal_path
        .trim_start_matches("./")
        .trim_start_matches('/');
    if let Some(idx) = clean.rfind("/.canons/") {
        let scope_part = clean[..idx].trim_start_matches("./").trim_matches('/');
        let rel = &clean[idx + "/.canons/".len()..];
        let scope = (!scope_part.is_empty() && scope_part != ".")
            .then(|| CanonScope(scope_part.to_string()));
        (scope, rel.to_string())
    } else if let Some(rel) = clean.strip_prefix(".canons/") {
        (None, rel.to_string())
    } else {
        (None, clean.to_string())
    }
}

/// Resolves sourcing tier, scope, diagnostic path, and file stem from a source reference string.
pub fn resolve_source_info(raw_ref: &str) -> Option<CanonSourceInfo> {
    let trimmed = raw_ref.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.replace('\\', "/");

    let (source_kind, source_ref, file_path, internal_path) =
        if let Some(rest) = normalized.strip_prefix("builtin:") {
            let clean_rest = rest.trim_start_matches('/');
            (
                CanonSourceKind::Builtin,
                format!("builtin:{clean_rest}"),
                None,
                clean_rest.to_string(),
            )
        } else if let Some((prefix, rest)) = normalized.split_once(':')
            && is_valid_pack_namespace(prefix)
        {
            let clean_rest = rest.trim_start_matches('/');
            (
                CanonSourceKind::Pack(prefix.to_string()),
                format!("{prefix}:{clean_rest}"),
                None,
                clean_rest.to_string(),
            )
        } else {
            let clean_ws = normalized
                .strip_prefix("./")
                .unwrap_or(&normalized)
                .to_string();
            (
                CanonSourceKind::Workspace,
                clean_ws.clone(),
                Some(clean_ws.clone()),
                clean_ws,
            )
        };

    let (raw_scope, rel_path) = extract_relative_to_canons(&internal_path);
    let scope = match source_kind {
        CanonSourceKind::Workspace => raw_scope,
        CanonSourceKind::Builtin | CanonSourceKind::Pack(_) => None,
    };

    let diagnostic_path = strip_md_ext(&rel_path).trim_matches('/').to_string();
    let file_name = rel_path.rsplit('/').next().unwrap_or(&rel_path);
    let file_stem = strip_md_ext(file_name).to_string();

    Some(CanonSourceInfo {
        source_ref,
        source_kind,
        file_path,
        diagnostic_path,
        file_stem,
        scope,
    })
}

/// Derives the monorepo scope prefix (`<scope>`) when a workspace path is under `<scope>/.canons/`.
pub fn derive_scope(file_path: Option<&str>) -> Option<String> {
    file_path
        .and_then(resolve_source_info)
        .and_then(|info| info.scope.map(|s| s.0))
}

/// Derives the diagnostic path relative to the enclosing `.canons/` directory or pack root.
pub fn derive_diagnostic_path(source_ref: Option<&str>) -> Option<String> {
    source_ref
        .and_then(resolve_source_info)
        .map(|info| info.diagnostic_path)
        .filter(|p| !p.is_empty())
}

/// Returns `true` if a path or glob pattern contains a `..` parent-directory traversal segment.
pub fn has_directory_traversal(pattern: &str) -> bool {
    pattern
        .split(['/', '\\'])
        .any(|segment| segment.trim() == "..")
}

/// Normalizes and prefixes a relative path/glob pattern with its enclosing monorepo `<scope>/`.
///
/// Rejects patterns containing `..` directory traversal outside `<scope>/` (`SPEC.md` §3.2).
pub fn scope_pattern(pattern: &str, scope: &CanonScope) -> Result<String, CanonParseError> {
    if has_directory_traversal(pattern) {
        return Err(CanonParseError::new(
            format!(
                "Pattern '{}' attempts directory traversal outside scope '{}'",
                pattern, scope.0
            ),
            None,
        ));
    }

    let mut clean = pattern.replace('\\', "/");
    while let Some(stripped) = clean.strip_prefix("./") {
        clean = stripped.to_string();
    }
    clean = clean.trim_start_matches('/').to_string();

    let scope_prefix = format!("{}/", scope.0);
    if clean == scope.0 || clean.starts_with(&scope_prefix) {
        Ok(clean)
    } else if clean.is_empty() {
        Ok(format!("{}/**", scope.0))
    } else {
        Ok(format!("{}/{}", scope.0, clean))
    }
}
