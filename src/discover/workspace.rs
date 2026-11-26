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

//! Workspace file tree and Canon corpus snapshot materializer for `discover`.

use super::DiscoverError;
use super::glob_matcher::{
    DEFAULT_CANON_GLOB, GlobPattern, is_ignored_path, is_wildcard_pattern, normalize_workspace_path,
};
use crate::builtin::{builtin_canons, find_builtin_canon};
use crate::canon::{
    CANON_FILE_EXT, Canon, CanonSourceKind, DEFAULT_GLOBAL_TRIGGER_PATTERN, DEFAULT_INSPECT_PLANES,
    ParseCanonOptions, derive_id, derive_triggers, id_to_title_case, parse_canon,
    resolve_source_info,
};
use crate::policy::is_direct_policy_file_path;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// In-memory snapshot of workspace file paths, optional UTF-8 target file contents, and discovered [`Canon`] definitions.
///
/// Decouples filesystem traversal from pure domain evaluation (`discover`, `validate`, `apprise`, `docket`, `admit`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    /// Stably sorted workspace-relative POSIX file paths present in the workspace.
    pub existing_files: BTreeSet<String>,
    /// Discovered [`Canon`] entities keyed by workspace-relative POSIX path or source reference.
    pub canons: BTreeMap<String, Canon>,
    /// Optional materialized UTF-8 file contents keyed by workspace-relative POSIX path (populated JIT for target-path sweeps).
    pub file_contents: BTreeMap<String, String>,
    /// Raw UTF-8 contents of discovered `<scope>/.canons/policy-chains.yaml` policy files keyed by workspace-relative POSIX path.
    pub policy_files: BTreeMap<String, String>,
    /// Workspace-relative POSIX paths whose `--workspace-state base` diff overlay has already been applied (ensuring idempotency).
    pub applied_base_diffs: BTreeSet<String>,
}

impl WorkspaceSnapshot {
    /// Creates an empty workspace snapshot.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a workspace-relative file path into the snapshot's existing file tree.
    pub fn with_file(mut self, path: impl Into<String>) -> Self {
        if let Ok(normalized) = normalize_workspace_path(&path.into())
            && normalized != "."
        {
            self.existing_files.insert(normalized);
        }
        self
    }

    /// Inserts a workspace-relative file path alongside its UTF-8 text content into the snapshot.
    pub fn with_file_content(
        mut self,
        path: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        if let Ok(normalized) = normalize_workspace_path(&path.into())
            && normalized != "."
        {
            self.existing_files.insert(normalized.clone());
            self.file_contents.insert(normalized, content.into());
        }
        self
    }

    /// Inserts a `<scope>/.canons/policy-chains.yaml` policy file and its raw UTF-8 YAML content into the snapshot.
    pub fn with_policy_file(
        mut self,
        path: impl Into<String>,
        raw_yaml: impl Into<String>,
    ) -> Self {
        if let Ok(normalized) = normalize_workspace_path(&path.into())
            && normalized != "."
        {
            self.existing_files.insert(normalized.clone());
            if is_direct_policy_file_path(&normalized) {
                self.policy_files.insert(normalized, raw_yaml.into());
            }
        }
        self
    }

    /// Populates UTF-8 `file_contents` on disk for the specified workspace-relative `paths`
    /// when not already cached, skipping missing, directory, or non-UTF-8 binary files.
    pub fn populate_file_contents_from_disk(&mut self, workspace_root: &Path, paths: &[String]) {
        for raw_path in paths {
            let Ok(normalized) = normalize_workspace_path(raw_path) else {
                continue;
            };
            if normalized == "."
                || self.file_contents.contains_key(&normalized)
                || self.applied_base_diffs.contains(&normalized)
            {
                continue;
            }
            let full_path = workspace_root.join(&normalized);
            if full_path.is_file()
                && let Ok(content) = fs::read_to_string(&full_path)
            {
                self.existing_files.insert(normalized.clone());
                self.file_contents.insert(normalized, content);
            }
        }
    }

    /// Inserts a pre-parsed [`Canon`] keyed by its workspace-relative path or source reference.
    ///
    /// Only local workspace canon paths are inserted into `existing_files`; virtual `builtin:*`
    /// and pack references remain isolated from the target file tree.
    pub fn with_canon(mut self, source_ref: impl Into<String>, canon: Canon) -> Self {
        let key = source_ref.into();
        if is_workspace_source_ref(&key) {
            self.existing_files.insert(key.clone());
        }
        self.canons.insert(key, canon);
        self
    }

    /// Parses raw Canon Markdown and inserts it into the snapshot under `options.source_ref`.
    ///
    /// Only local workspace canon paths are inserted into `existing_files`; virtual `builtin:*`
    /// and pack references remain isolated from the target file tree.
    pub fn with_canon_markdown(mut self, raw_markdown: &str, options: &ParseCanonOptions) -> Self {
        let key = options
            .source_ref
            .clone()
            .unwrap_or_else(|| format!(".canons/unnamed{CANON_FILE_EXT}"));
        let canon = parse_canon_or_fallback(raw_markdown, options);
        if is_workspace_source_ref(&key) {
            self.existing_files.insert(key.clone());
        }
        self.canons.insert(key, canon);
        self
    }

    /// Inserts all compile-time embedded `builtin:*` standard canons into `self.canons`
    /// without polluting `self.existing_files`.
    pub fn with_builtins(mut self) -> Self {
        for entry in builtin_canons() {
            let opts = ParseCanonOptions::with_source_ref(entry.source_ref);
            let canon = parse_canon_or_fallback(entry.raw_markdown, &opts);
            self.canons.insert(entry.source_ref.to_string(), canon);
        }
        self
    }

    /// Parses an ephemeral inline canon (`inline:<name>`) and inserts it into `self.canons`
    /// without polluting `self.existing_files`.
    pub fn insert_inline_canon(&mut self, source_ref: impl Into<String>, raw_markdown: &str) {
        let key = source_ref.into();
        let opts = ParseCanonOptions::with_source_ref(key.clone());
        let canon = parse_canon_or_fallback(raw_markdown, &opts);
        self.canons.insert(key, canon);
    }

    /// Builder variant of [`Self::insert_inline_canon`].
    pub fn with_inline_canon(mut self, source_ref: impl Into<String>, raw_markdown: &str) -> Self {
        self.insert_inline_canon(source_ref, raw_markdown);
        self
    }

    /// Backfills ephemeral `inline:*` canons recorded on `caseload.discovery.inline_canons`
    /// into `self.canons` when not already present.
    pub fn backfill_inline_canons(&mut self, inline_canons: &BTreeMap<String, String>) {
        for (source_ref, raw_markdown) in inline_canons {
            if !self.canons.contains_key(source_ref) {
                self.insert_inline_canon(source_ref.clone(), raw_markdown);
            }
        }
    }

    /// Backfills a single candidate canon reference into `self.canons` when not already loaded,
    /// resolving `builtin:*` references from the embedded registry and workspace paths from disk.
    pub fn backfill_canon_ref(&mut self, workspace_root: &Path, canon_ref: &str) {
        if self.canons.contains_key(canon_ref) {
            return;
        }
        if let Some(entry) = find_builtin_canon(canon_ref) {
            let opts = ParseCanonOptions::with_source_ref(entry.source_ref);
            let canon = parse_canon_or_fallback(entry.raw_markdown, &opts);
            self.canons.insert(entry.source_ref.to_string(), canon);
            return;
        }
        if !is_workspace_source_ref(canon_ref) {
            return;
        }
        let full_path = workspace_root.join(canon_ref);
        if full_path.is_file()
            && let Ok(raw_content) = fs::read_to_string(&full_path)
        {
            let opts = ParseCanonOptions::with_source_ref(canon_ref.to_string());
            let canon = parse_canon_or_fallback(&raw_content, &opts);
            self.existing_files.insert(canon_ref.to_string());
            self.canons.insert(canon_ref.to_string(), canon);
        }
    }

    /// Materializes a [`WorkspaceSnapshot`] by traversing `workspace_root` on disk and loading
    /// embedded `builtin:*` standard canons by default.
    pub fn from_disk(
        workspace_root: &Path,
        canon_patterns: &[String],
    ) -> Result<Self, DiscoverError> {
        Self::from_disk_with_builtins(workspace_root, canon_patterns, true)
    }

    /// Materializes a [`WorkspaceSnapshot`] by traversing `workspace_root` on disk, optionally
    /// loading compile-time embedded `builtin:*` standard canons.
    pub fn from_disk_with_builtins(
        workspace_root: &Path,
        canon_patterns: &[String],
        include_builtins: bool,
    ) -> Result<Self, DiscoverError> {
        let default_glob = GlobPattern::parse(DEFAULT_CANON_GLOB)?;
        let explicit_matchers = compile_canon_filter_globs(canon_patterns)?;

        let mut existing_files = BTreeSet::new();
        walk_workspace_files(workspace_root, workspace_root, &mut existing_files)?;

        let mut canons = BTreeMap::new();
        let mut policy_files = BTreeMap::new();
        for rel_path in &existing_files {
            if is_direct_policy_file_path(rel_path) {
                let full_path = workspace_root.join(rel_path);
                let raw_yaml = fs::read_to_string(&full_path).map_err(|err| DiscoverError::Io {
                    path: rel_path.clone(),
                    message: err.to_string(),
                })?;
                policy_files.insert(rel_path.clone(), raw_yaml);
                continue;
            }
            if !rel_path.ends_with(CANON_FILE_EXT) {
                continue;
            }
            let matches_canon_domain = if explicit_matchers.is_empty() {
                default_glob.is_match(rel_path)
            } else {
                explicit_matchers.iter().any(|m| m.is_match(rel_path))
            };

            if matches_canon_domain {
                let full_path = workspace_root.join(rel_path);
                let raw_content =
                    fs::read_to_string(&full_path).map_err(|err| DiscoverError::Io {
                        path: rel_path.clone(),
                        message: err.to_string(),
                    })?;
                let opts = ParseCanonOptions::with_source_ref(rel_path.clone());
                let canon = parse_canon_or_fallback(&raw_content, &opts);
                canons.insert(rel_path.clone(), canon);
            }
        }

        if include_builtins {
            for entry in builtin_canons() {
                let matches_builtin = explicit_matchers.is_empty()
                    || explicit_matchers
                        .iter()
                        .any(|m| m.is_match(entry.source_ref));
                if matches_builtin {
                    let opts = ParseCanonOptions::with_source_ref(entry.source_ref);
                    let canon = parse_canon_or_fallback(entry.raw_markdown, &opts);
                    canons.insert(entry.source_ref.to_string(), canon);
                }
            }
        }

        Ok(Self {
            existing_files,
            canons,
            file_contents: BTreeMap::new(),
            policy_files,
            applied_base_diffs: BTreeSet::new(),
        })
    }
}

fn is_workspace_source_ref(source_ref: &str) -> bool {
    resolve_source_info(source_ref)
        .is_some_and(|info| info.source_kind == CanonSourceKind::Workspace)
}

/// Compiles `--canon <path|glob>` patterns into [`GlobPattern`]s, expanding directory and `builtin:` domain prefixes.
pub(crate) fn compile_canon_filter_globs(
    canon_patterns: &[String],
) -> Result<Vec<GlobPattern>, DiscoverError> {
    let mut compiled = Vec::new();
    for raw in canon_patterns {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("builtin:") {
            let clean_rest = rest.trim_start_matches('/').trim_end_matches('/');
            if clean_rest.is_empty() || clean_rest == "**" || clean_rest == "*" {
                compiled.push(GlobPattern::parse("builtin:*/**")?);
                continue;
            }
            if clean_rest
                .replace('\\', "/")
                .split('/')
                .any(|seg| seg == "..")
            {
                return Err(DiscoverError::PathEscapesWorkspace { path: raw.clone() });
            }
            let normalized_rest = if !is_wildcard_pattern(clean_rest)
                && clean_rest.len() >= CANON_FILE_EXT.len()
                && clean_rest[clean_rest.len() - CANON_FILE_EXT.len()..]
                    .eq_ignore_ascii_case(CANON_FILE_EXT)
            {
                &clean_rest[..clean_rest.len() - CANON_FILE_EXT.len()]
            } else {
                clean_rest
            };
            let canonical = format!("builtin:{normalized_rest}");
            compiled.push(GlobPattern::parse(&canonical)?);
            if !is_wildcard_pattern(normalized_rest) && !normalized_rest.contains('/') {
                compiled.push(GlobPattern::parse(&format!("{canonical}/**"))?);
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("inline:") {
            let clean_rest = rest.trim_start_matches('/').trim_end_matches('/');
            if clean_rest.is_empty() || clean_rest == "**" || clean_rest == "*" {
                compiled.push(GlobPattern::parse("inline:**")?);
                continue;
            }
            if clean_rest
                .replace('\\', "/")
                .split('/')
                .any(|seg| seg == "..")
            {
                return Err(DiscoverError::PathEscapesWorkspace { path: raw.clone() });
            }
            let normalized_rest = if !is_wildcard_pattern(clean_rest)
                && clean_rest.len() >= CANON_FILE_EXT.len()
                && clean_rest[clean_rest.len() - CANON_FILE_EXT.len()..]
                    .eq_ignore_ascii_case(CANON_FILE_EXT)
            {
                &clean_rest[..clean_rest.len() - CANON_FILE_EXT.len()]
            } else {
                clean_rest
            };
            let canonical = format!("inline:{normalized_rest}");
            compiled.push(GlobPattern::parse(&canonical)?);
            continue;
        }

        let normalized = normalize_workspace_path(trimmed)
            .map_err(|_| DiscoverError::PathEscapesWorkspace { path: raw.clone() })?;
        compiled.push(GlobPattern::parse(&normalized)?);
        if !normalized.ends_with(CANON_FILE_EXT) && !is_wildcard_pattern(&normalized) {
            let dir_glob = if normalized == "." {
                DEFAULT_CANON_GLOB.to_string()
            } else {
                format!("{normalized}/**/*{CANON_FILE_EXT}")
            };
            compiled.push(GlobPattern::parse(&dir_glob)?);
        }
    }
    Ok(compiled)
}

fn walk_workspace_files(
    root: &Path,
    current_dir: &Path,
    out: &mut BTreeSet<String>,
) -> Result<(), DiscoverError> {
    let entries = match fs::read_dir(current_dir) {
        Ok(iter) => iter,
        Err(err) => {
            if current_dir == root {
                return Err(DiscoverError::Io {
                    path: current_dir.display().to_string(),
                    message: err.to_string(),
                });
            }
            return Ok(());
        }
    };

    let mut sorted_entries: Vec<_> = entries.filter_map(Result::ok).collect();
    sorted_entries.sort_by_key(|e| e.file_name());

    for entry in sorted_entries {
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let rel_posix = rel.to_string_lossy().replace('\\', "/");
        if rel_posix.is_empty() || is_ignored_path(&rel_posix) {
            continue;
        }

        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            walk_workspace_files(root, &path, out)?;
        } else if metadata.is_file() {
            out.insert(rel_posix);
        }
    }

    Ok(())
}

/// Parses a Canon file or constructs a fallback [`Canon`] when YAML frontmatter is malformed,
/// allowing `discover` to forward broken canons to downstream `validate` linting.
pub fn parse_canon_or_fallback(raw_content: &str, options: &ParseCanonOptions) -> Canon {
    if let Ok(canon) = parse_canon(raw_content, options) {
        return canon;
    }

    let source_info = options.source_ref.as_deref().and_then(resolve_source_info);
    let scope = source_info.as_ref().and_then(|s| s.scope.clone());
    let id = derive_id(None, source_info.as_ref());
    let title = id_to_title_case(&id);
    let triggers = derive_triggers(None, scope.as_ref())
        .unwrap_or_else(|_| vec![DEFAULT_GLOBAL_TRIGGER_PATTERN.to_string()]);
    let inspect = DEFAULT_INSPECT_PLANES.to_vec();

    Canon {
        id,
        title: title.clone(),
        triggers,
        exists: Vec::new(),
        inspect,
        tags: Vec::new(),
        references: Vec::new(),
        invariant: title,
        invariant_span: None,
        exceptions: Vec::new(),
        precedent: None,
        rationale: None,
        remediation: None,
        source_ref: source_info.as_ref().map(|s| s.source_ref.clone()),
        source_kind: source_info.as_ref().map(|s| s.source_kind.clone()),
        file_path: source_info.as_ref().and_then(|s| s.file_path.clone()),
        diagnostic_path: source_info
            .as_ref()
            .map(|s| s.diagnostic_path.clone())
            .filter(|p| !p.is_empty()),
        scope: scope.map(|s| s.0),
        raw_content: raw_content.to_string(),
        raw_body: raw_content.to_string(),
        raw_frontmatter: None,
        frontmatter_span: None,
        frontmatter_key_spans: Vec::new(),
        directive_spans: Vec::new(),
    }
}
