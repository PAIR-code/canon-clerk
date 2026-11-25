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

//! Composable CLI argument group for the runtime diff exhibit head-context budget
//! (`--diff-head-lines <LINES>`, `docs/architecture/overview.md` §9).

use crate::cli::CliError;
use crate::configure::StoredConfig;
use crate::spans::{DEFAULT_DIFF_HEAD_LINES, ENV_DIFF_HEAD_LINES};
use clap::Args;
use std::collections::BTreeMap;

/// Composable CLI argument group for diff exhibit head-context expansion across
/// exhibit-rendering subcommands (`admit`, `vet`, `audit`).
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffContextArgs {
    /// Post-image head lines included on modified diff exhibits (`0` disables; default: `80`; env: `CANON_CLERK_DIFF_HEAD_LINES`; config: `defaults.diffHeadLines`).
    #[arg(
        long = "diff-head-lines",
        value_name = "LINES",
        allow_hyphen_values = true,
        help_heading = "Diff Exhibit Context",
        help = format!(
            "Post-image head lines included on modified diff exhibits; 0 disables expansion (default: {DEFAULT_DIFF_HEAD_LINES}; fallback env: {ENV_DIFF_HEAD_LINES})"
        )
    )]
    pub diff_head_lines: Option<String>,
}

impl DiffContextArgs {
    /// Resolves the effective diff head-context line budget following standard specificity precedence:
    /// `CLI --diff-head-lines` -> `CANON_CLERK_DIFF_HEAD_LINES` -> `config.json defaults.diffHeadLines` -> `80`.
    pub fn resolve_diff_head_lines(
        &self,
        env: &BTreeMap<String, String>,
        stored: Option<&StoredConfig>,
    ) -> Result<usize, CliError> {
        if let Some(raw_flag) = self.diff_head_lines.as_deref() {
            return raw_flag.trim().parse::<usize>().map_err(|_| {
                CliError::usage(format!(
                    "Invalid `--diff-head-lines` value `{raw_flag}`: expected a non-negative integer in lines (`>= 0`).\n  \
                     Hint: Pass `--diff-head-lines <LINES>` (default: `{DEFAULT_DIFF_HEAD_LINES}`; pass `0` to disable head-context expansion) or set `{ENV_DIFF_HEAD_LINES}=<LINES>`."
                ))
            });
        }

        if let Some(raw_env) = env
            .get(ENV_DIFF_HEAD_LINES)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            return raw_env.parse::<usize>().map_err(|_| {
                CliError::usage(format!(
                    "Invalid diff head-context budget `{raw_env}` in `{ENV_DIFF_HEAD_LINES}`: expected a non-negative integer in lines (`>= 0`).\n  \
                     Hint: Set `{ENV_DIFF_HEAD_LINES}={DEFAULT_DIFF_HEAD_LINES}` (or `0` to disable) or override via `--diff-head-lines <LINES>`."
                ))
            });
        }

        if let Some(stored_val) = stored.and_then(|s| s.defaults.diff_head_lines) {
            return Ok(stored_val);
        }

        Ok(DEFAULT_DIFF_HEAD_LINES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure::StoredDefaultsConfig;

    #[test]
    fn resolve_diff_head_lines_follows_standard_precedence() {
        let empty_env = BTreeMap::new();
        assert_eq!(
            DiffContextArgs::default()
                .resolve_diff_head_lines(&empty_env, None)
                .expect("default"),
            DEFAULT_DIFF_HEAD_LINES
        );

        let stored = StoredConfig {
            defaults: StoredDefaultsConfig {
                diff_head_lines: Some(64),
                ..StoredDefaultsConfig::default()
            },
            ..StoredConfig::default()
        };
        assert_eq!(
            DiffContextArgs::default()
                .resolve_diff_head_lines(&empty_env, Some(&stored))
                .expect("stored"),
            64
        );

        let mut env = BTreeMap::new();
        env.insert(ENV_DIFF_HEAD_LINES.to_string(), "120".to_string());
        assert_eq!(
            DiffContextArgs::default()
                .resolve_diff_head_lines(&env, Some(&stored))
                .expect("env"),
            120
        );

        let cli_zero = DiffContextArgs {
            diff_head_lines: Some("0".to_string()),
        };
        assert_eq!(
            cli_zero
                .resolve_diff_head_lines(&env, Some(&stored))
                .expect("cli zero"),
            0
        );
    }

    #[test]
    fn resolve_diff_head_lines_rejects_negative_or_non_integer_values() {
        let empty_env = BTreeMap::new();
        let bad_flag = DiffContextArgs {
            diff_head_lines: Some("-5".to_string()),
        };
        let err = bad_flag
            .resolve_diff_head_lines(&empty_env, None)
            .expect_err("negative flag must fail");
        assert!(err.to_string().contains("--diff-head-lines"));
        assert!(err.to_string().contains("Hint:"));

        let mut bad_env = BTreeMap::new();
        bad_env.insert(ENV_DIFF_HEAD_LINES.to_string(), "abc".to_string());
        let env_err = DiffContextArgs::default()
            .resolve_diff_head_lines(&bad_env, None)
            .expect_err("non-integer env must fail");
        assert!(env_err.to_string().contains(ENV_DIFF_HEAD_LINES));
        assert!(env_err.to_string().contains("Hint:"));
    }

    #[test]
    fn diff_head_lines_flag_is_accepted_on_admit_vet_audit_and_rejected_elsewhere() {
        use crate::cli::{Cli, Commands};
        use clap::Parser;

        for subcmd in ["admit", "vet", "audit"] {
            let parsed = Cli::try_parse_from([
                "canon-clerk",
                subcmd,
                "--diff-head-lines",
                "40",
                "src/lib.rs",
            ])
            .unwrap_or_else(|e| panic!("`{subcmd}` should accept --diff-head-lines: {e}"));
            match parsed.command {
                Some(Commands::Admit(a)) => {
                    assert_eq!(a.diff_context.diff_head_lines.as_deref(), Some("40"));
                }
                Some(Commands::Vet(v)) => {
                    assert_eq!(v.diff_context.diff_head_lines.as_deref(), Some("40"));
                }
                Some(Commands::Audit(a)) => {
                    assert_eq!(a.diff_context.diff_head_lines.as_deref(), Some("40"));
                }
                other => panic!("unexpected command variant: {other:?}"),
            }
        }

        for rejected in ["intake", "discover", "validate", "docket"] {
            assert!(
                Cli::try_parse_from([
                    "canon-clerk",
                    rejected,
                    "--diff-head-lines",
                    "40",
                    "src/lib.rs",
                ])
                .is_err(),
                "`{rejected}` should reject --diff-head-lines"
            );
        }
    }
}
