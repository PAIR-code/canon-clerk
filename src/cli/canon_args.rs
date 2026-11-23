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

//! Composable Clap argument group for canon selection and ephemeral inline canons.

use crate::canon::normalize_inline_canon_name;
use crate::cli::CliError;
use crate::cli::io::read_operand_source;
use clap::Args;
use std::collections::BTreeMap;
use std::io::Read;

/// Composable CLI argument group for canon selection and discovery filtering.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonSelectionArgs {
    /// Discover or validate all canons across the repository corpus, bypassing path trigger matching.
    #[arg(
        long = "all-canons",
        help_heading = "Canon Selection",
        help = "Evaluate all repository canons, bypassing trigger matching"
    )]
    pub all_canons: bool,

    /// Explicitly restrict candidate canons to a specific file path or glob subset.
    #[arg(
        long = "canon",
        value_name = "PATH|GLOB",
        help_heading = "Canon Selection",
        help = "Explicitly restrict candidate canons to a specific subset"
    )]
    pub canon: Vec<String>,

    /// Ephemeral inline canon name (`inline:<name>`, optional `.canon.md` suffix stripped).
    #[arg(
        long = "canon-name",
        value_name = "NAME",
        help_heading = "Canon Selection",
        help = "Evaluate an ephemeral inline canon by name without creating a file on disk"
    )]
    pub canon_name: Option<String>,

    /// Optional inline canon Markdown body (with optional YAML frontmatter); requires `--canon-name`.
    #[arg(
        long = "canon-body",
        value_name = "TEXT",
        allow_hyphen_values = true,
        requires = "canon_name",
        conflicts_with = "canon_body_file",
        help_heading = "Canon Selection",
        help = "Inline Markdown body (with optional YAML frontmatter) for --canon-name"
    )]
    pub canon_body: Option<String>,

    /// Read inline canon Markdown body from a file path or `-` for stdin; requires `--canon-name`.
    #[arg(
        long = "canon-body-file",
        value_name = "PATH|-",
        requires = "canon_name",
        conflicts_with = "canon_body",
        help_heading = "Canon Selection",
        help = "Read inline canon Markdown body from a file or '-' for stdin (requires --canon-name)"
    )]
    pub canon_body_file: Option<String>,

    /// Include compile-time embedded `builtin:*` standard canons during discovery (default).
    #[arg(
        long = "builtins",
        overrides_with = "no_builtins",
        help_heading = "Canon Selection",
        help = "Include embedded builtin:* standard canons (default)"
    )]
    pub builtins: bool,

    /// Exclude compile-time embedded `builtin:*` standard canons during discovery.
    #[arg(
        long = "no-builtins",
        overrides_with = "builtins",
        help_heading = "Canon Selection",
        help = "Exclude embedded builtin:* standard canons"
    )]
    pub no_builtins: bool,
}

impl CanonSelectionArgs {
    /// Returns `true` if an ephemeral inline canon (`--canon-name`) was supplied.
    pub fn has_inline_canon(&self) -> bool {
        self.canon_name.is_some()
    }

    /// Returns `true` if embedded `builtin:*` standard canons should be included.
    ///
    /// Defaults to `true`, except when `--no-builtins` is passed or when `--canon-name` is
    /// supplied without explicit `--builtins`, `--canon`, or `--all-canons`.
    pub fn include_builtins(&self) -> bool {
        if self.no_builtins {
            return false;
        }
        if self.canon_name.is_some() && !self.builtins && !self.all_canons && self.canon.is_empty()
        {
            return false;
        }
        true
    }

    /// Returns `true` if local workspace `.canons/**` canons should be included during discovery.
    ///
    /// Defaults to `true`, except when `--canon-name` is supplied without explicit `--canon`
    /// or `--all-canons`.
    pub fn include_workspace_canons(&self) -> bool {
        if self.canon_name.is_some() {
            return self.all_canons || !self.canon.is_empty();
        }
        true
    }

    /// Returns `true` if canon selection is unconstrained (no explicit selection flags).
    pub fn is_empty(&self) -> bool {
        !self.all_canons
            && self.canon.is_empty()
            && !self.builtins
            && !self.no_builtins
            && self.canon_name.is_none()
            && self.canon_body.is_none()
            && self.canon_body_file.is_none()
    }

    /// Resolves any `--canon-name` + optional `--canon-body` / `--canon-body-file` flags into a
    /// canonical `inline:<name> -> raw Markdown` map.
    pub fn resolve_inline_canons(
        &self,
        stdin: &mut dyn Read,
    ) -> Result<BTreeMap<String, String>, CliError> {
        if self.canon_body.is_some() && self.canon_body_file.is_some() {
            return Err(CliError::usage(
                "Conflicting inline canon body flags: `--canon-body` and `--canon-body-file` are mutually exclusive.\n  \
                 Hint: Supply either `--canon-body <TEXT>` or `--canon-body-file <PATH|->`, not both."
                    .to_string(),
            ));
        }

        let Some(raw_name) = self.canon_name.as_deref() else {
            if self.canon_body.is_some() || self.canon_body_file.is_some() {
                return Err(CliError::usage(
                    "`--canon-body` / `--canon-body-file` requires `--canon-name <NAME>`.\n  \
                     Hint: Provide a kebab-case canon name such as `--canon-name no-eval`."
                        .to_string(),
                ));
            }
            return Ok(BTreeMap::new());
        };

        let source_ref = normalize_inline_canon_name(raw_name).map_err(|msg| {
            CliError::usage(format!(
                "Invalid `--canon-name`: {msg}\n  \
                 Hint: Provide a kebab-case rule name (e.g. `no-eval` or `security/no-eval`, with or without `.canon.md`)."
            ))
        })?;

        let raw_markdown = if let Some(body) = &self.canon_body {
            body.clone()
        } else if let Some(path) = &self.canon_body_file {
            read_operand_source(path, "--canon-body-file", stdin)?
        } else {
            String::new()
        };

        let mut map = BTreeMap::new();
        map.insert(source_ref, raw_markdown);
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canon_selection_args_is_empty() {
        let empty = CanonSelectionArgs::default();
        assert!(empty.is_empty());
        assert!(empty.include_builtins());

        let all_canons = CanonSelectionArgs {
            all_canons: true,
            ..CanonSelectionArgs::default()
        };
        assert!(!all_canons.is_empty());

        let specific_canons = CanonSelectionArgs {
            canon: vec!["foo.md".to_string()],
            ..CanonSelectionArgs::default()
        };
        assert!(!specific_canons.is_empty());

        let disabled_builtins = CanonSelectionArgs {
            no_builtins: true,
            ..CanonSelectionArgs::default()
        };
        assert!(!disabled_builtins.is_empty());
        assert!(!disabled_builtins.include_builtins());

        let explicit_builtins = CanonSelectionArgs {
            builtins: true,
            ..CanonSelectionArgs::default()
        };
        assert!(!explicit_builtins.is_empty());
        assert!(explicit_builtins.include_builtins());
    }

    #[test]
    fn test_canon_body_accepts_leading_frontmatter_hyphens() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            canons: CanonSelectionArgs,
        }

        let body = "---\ntriggers:\n  - \"**/*.rs\"\n---\nCode MUST avoid eval().";
        let parsed = TestCli::try_parse_from([
            "canon-clerk",
            "--canon-name",
            "avoid-eval",
            "--canon-body",
            body,
        ])
        .expect("clap must accept YAML frontmatter starting with ---");
        assert_eq!(parsed.canons.canon_name.as_deref(), Some("avoid-eval"));
        assert_eq!(parsed.canons.canon_body.as_deref(), Some(body));
    }
}
