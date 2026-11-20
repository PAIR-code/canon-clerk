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

//! Composable Clap argument groups for Caseload DAG stage subcommands.

use crate::caseload::{Caseload, ThinkingLevel, WorkspaceState};
use crate::cli::CliError;
use crate::cli::github_args::{resolve_filing_source, resolve_filing_source_with_fetcher};
use crate::cli::io::{read_caseload_operand, write_caseload_file};
use crate::configure::{ConfigureError, StoredConfig, parse_thinking_level};
use crate::github::GithubPullRequestFetcher;
use crate::intake::IntakeOptions;
use crate::provider::{
    DEFAULT_MAX_BACKOFF_MS, DEFAULT_MAX_RETRIES, ENV_MAX_BACKOFF_MS, ENV_MAX_RETRIES, RetryPolicy,
};
use clap::{Args, ValueEnum};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;

/// CLI value enum for `--workspace-state <head|base>`.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceStateArg {
    /// Workspace files on disk already contain the filing's post-patch state (default).
    Head,
    /// Workspace files on disk reflect the pre-patch base branch (e.g., `pull_request_target` CI).
    Base,
}

impl From<WorkspaceStateArg> for WorkspaceState {
    fn from(value: WorkspaceStateArg) -> Self {
        match value {
            WorkspaceStateArg::Head => Self::Head,
            WorkspaceStateArg::Base => Self::Base,
        }
    }
}

/// Composable CLI argument group for filing sources and pull request metadata (Branch A).
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct FilingSourceArgs {
    /// Target file paths, directories, globs, or `-` to read newline-delimited paths from stdin.
    #[arg(
        value_name = "TARGETS",
        help = "Target file paths, directories, globs, or '-' for newline-delimited stdin paths"
    )]
    pub targets: Vec<String>,

    /// Read unified diff patch from a file path or `-` for standard input.
    #[arg(
        long = "diff",
        value_name = "PATH|-",
        conflicts_with = "github_pr",
        help_heading = "Filing Sources",
        help = "Ingest unified diff patch from file or '-' for stdin"
    )]
    pub diff: Option<String>,

    /// Ingest GitHub pull request diff and metadata natively (omit number to read `$GITHUB_EVENT_PATH`).
    #[arg(
        long = "github-pr",
        value_name = "NUMBER",
        num_args = 0..=1,
        conflicts_with = "diff",
        help_heading = "Filing Sources",
        help = "Ingest GitHub PR diff and metadata natively (omit NUMBER to read from GITHUB_EVENT_PATH)"
    )]
    pub github_pr: Option<Option<u64>>,

    /// Target GitHub repository in `<owner>/<repo>` format (fallback env: `GITHUB_REPOSITORY`).
    #[arg(
        long = "github-repo",
        value_name = "OWNER/REPO",
        help_heading = "GitHub Context",
        help = "Target GitHub repository '<owner>/<repo>' (fallback env: GITHUB_REPOSITORY)"
    )]
    pub github_repo: Option<String>,

    /// Workspace state on disk relative to `--diff` or `--github-pr` (`head` default, or `base` for pre-patch base checkouts).
    #[arg(
        long = "workspace-state",
        value_name = "head|base",
        value_enum,
        help_heading = "Filing Sources",
        help = "Workspace state on disk relative to --diff ('head' = post-patch default, 'base' = pre-patch base branch)"
    )]
    pub workspace_state: Option<WorkspaceStateArg>,

    /// Prospective architectural design intent or plan description.
    #[arg(
        long = "intent",
        value_name = "TEXT",
        help_heading = "Filing Sources",
        help = "Ingest prospective architectural design intent or plan description"
    )]
    pub intent: Option<String>,

    /// Ingest all repository files as target artifacts (`scope: "all-targets"`).
    #[arg(
        long = "all-targets",
        help_heading = "Filing Sources",
        help = "Ingest all repository files as target artifacts"
    )]
    pub all_targets: bool,

    /// Pull request title or commit subject line.
    #[arg(
        long = "pr-title",
        value_name = "TEXT",
        help_heading = "Pull Request Metadata",
        help = "Ingest pull request title or commit subject text"
    )]
    pub pr_title: Option<String>,

    /// Pull request Markdown description or commit body.
    #[arg(
        long = "pr-body",
        value_name = "TEXT",
        conflicts_with = "pr_body_file",
        help_heading = "Pull Request Metadata",
        help = "Ingest pull request Markdown description text"
    )]
    pub pr_body: Option<String>,

    /// Read pull request Markdown description from a file path or `-` for stdin.
    #[arg(
        long = "pr-body-file",
        value_name = "PATH",
        conflicts_with = "pr_body",
        help_heading = "Pull Request Metadata",
        help = "Ingest pull request Markdown description from a file"
    )]
    pub pr_body_file: Option<String>,
}

impl FilingSourceArgs {
    /// Resolves flags, file paths, stdin streams, and optional `--github-pr` inputs using process env.
    pub fn resolve_intake_options(&self, stdin: &mut dyn Read) -> Result<IntakeOptions, CliError> {
        let env: BTreeMap<String, String> = std::env::vars().collect();
        self.resolve_intake_options_with_env(stdin, &env)
            .map(|resolved| resolved.intake)
    }

    /// Resolves flags and optional `--github-pr` inputs against an explicit environment map.
    pub fn resolve_intake_options_with_env(
        &self,
        stdin: &mut dyn Read,
        env: &BTreeMap<String, String>,
    ) -> Result<ResolvedFilingSource, CliError> {
        resolve_filing_source(self, stdin, env)
    }

    /// Resolves flags and optional `--github-pr` inputs against an explicit environment map and fetcher.
    pub fn resolve_intake_options_with_env_and_fetcher(
        &self,
        stdin: &mut dyn Read,
        env: &BTreeMap<String, String>,
        fetcher: &dyn GithubPullRequestFetcher,
    ) -> Result<ResolvedFilingSource, CliError> {
        resolve_filing_source_with_fetcher(self, stdin, env, fetcher)
    }
}

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
    /// Returns `true` if embedded `builtin:*` standard canons should be included (default: `true`).
    pub fn include_builtins(&self) -> bool {
        !self.no_builtins
    }

    /// Returns `true` if canon selection is unconstrained (no explicit `--all-canons`, `--canon`, `--builtins`, or `--no-builtins`).
    pub fn is_empty(&self) -> bool {
        !self.all_canons && self.canon.is_empty() && !self.builtins && !self.no_builtins
    }
}

/// Composable CLI argument group for active provider selection, reasoning effort, and provider credentials (Branch B).
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderConfigArgs {
    /// Active default provider slug (`google`, `typesafe`; env: `CANON_CLERK_PROVIDER`; config key: `defaults.provider`).
    #[arg(
        long = "provider",
        value_name = "SLUG",
        help_heading = "Model & Reasoning Options",
        help = "Default model provider slug: google or typesafe (fallback env: CANON_CLERK_PROVIDER)"
    )]
    pub provider: Option<String>,

    /// Qualitative reasoning effort tier (`minimal`, `low`, `medium`, `high`) for `Generator`-bound nodes (env: `CANON_CLERK_THINKING_LEVEL`).
    #[arg(
        long = "thinking-level",
        value_name = "LEVEL",
        help_heading = "Model & Reasoning Options",
        help = "Qualitative reasoning effort tier: minimal, low, medium, or high (fallback env: CANON_CLERK_THINKING_LEVEL)"
    )]
    pub thinking_level: Option<String>,

    /// Maximum reasoning budget in tokens for extended thinking on `Generator`-bound nodes (env: `CANON_CLERK_REASONING_BUDGET`).
    #[arg(
        long = "reasoning-budget",
        value_name = "TOKENS",
        help_heading = "Model & Reasoning Options",
        help = "Maximum reasoning budget tokens (fallback env: CANON_CLERK_REASONING_BUDGET)"
    )]
    pub reasoning_budget: Option<u32>,

    /// Google Gemini API key override (fallback env: `GEMINI_API_KEY`; never persisted to Caseload JSON).
    #[arg(
        long = "gemini-api-key",
        value_name = "KEY",
        help_heading = "Provider Credentials",
        help = "Google Gemini API key (fallback env: GEMINI_API_KEY)"
    )]
    pub gemini_api_key: Option<String>,

    /// TypeSafe AI API key override for Jev models (fallback env: `TYPESAFE_API_KEY`; never persisted to Caseload JSON).
    #[arg(
        long = "typesafe-api-key",
        value_name = "KEY",
        help_heading = "Provider Credentials",
        help = "TypeSafe AI API key for Jev models (fallback env: TYPESAFE_API_KEY)"
    )]
    pub typesafe_api_key: Option<String>,

    /// Google Generative Language API base URL override (fallback env: `CANON_CLERK_GOOGLE_BASE_URL`, `GEMINI_BASE_URL`).
    #[arg(
        long = "gemini-base-url",
        value_name = "URL",
        help_heading = "Provider Endpoints",
        help = "Google Generative Language API base URL (fallback env: GEMINI_BASE_URL)"
    )]
    pub gemini_base_url: Option<String>,

    /// TypeSafe AI System One API base URL override (fallback env: `CANON_CLERK_TYPESAFE_BASE_URL`, `TYPESAFE_BASE_URL`).
    #[arg(
        long = "typesafe-base-url",
        value_name = "URL",
        help_heading = "Provider Endpoints",
        help = "TypeSafe AI System One API base URL (fallback env: TYPESAFE_BASE_URL)"
    )]
    pub typesafe_base_url: Option<String>,

    /// Hidden capture rejecting retired `--screener-model` flag (`configure.md` §5).
    #[arg(long = "screener-model", hide = true, allow_hyphen_values = true)]
    pub rejected_screener_model: Option<String>,

    /// Hidden capture rejecting retired `--auditor-model` flag (`configure.md` §5).
    #[arg(long = "auditor-model", hide = true, allow_hyphen_values = true)]
    pub rejected_auditor_model: Option<String>,
}

impl ProviderConfigArgs {
    /// Rejects retired tier flags (`--screener-model`, `--auditor-model`) and validates `--thinking-level`
    /// syntax and mutual exclusivity against `--reasoning-budget`.
    pub fn validate_thinking_flags(&self) -> Result<Option<ThinkingLevel>, CliError> {
        if self.rejected_screener_model.is_some() {
            return Err(CliError::usage(
                "Retired flag `--screener-model` is no longer supported.\n  \
                 Hint: Pass `--provider <google|typesafe>` to select a default provider, or pin per-node \
                 `<provider>:<model>` specifiers via `--apprise-model`, `--docket-model`, `--admit-model`, or `--vet-model`.",
            ));
        }
        if self.rejected_auditor_model.is_some() {
            return Err(CliError::usage(
                "Retired flag `--auditor-model` is no longer supported.\n  \
                 Hint: Pass `--audit-model <PROVIDER:MODEL>` (for example, `--audit-model google:gemini-4-carbon`).",
            ));
        }
        let parsed_level = match self.thinking_level.as_deref() {
            Some(raw) => {
                Some(parse_thinking_level(raw).map_err(|err| CliError::usage(err.to_string()))?)
            }
            None => None,
        };
        if parsed_level.is_some() && self.reasoning_budget.is_some() {
            return Err(CliError::usage(
                ConfigureError::ConflictingThinkingOptions {
                    source: "CLI flags (`--thinking-level` and `--reasoning-budget`)".to_string(),
                }
                .to_string(),
            ));
        }
        Ok(parsed_level)
    }
}

/// Composable CLI argument group for pooled HTTP transport retry and per-origin rate-limit cooldown
/// bounds across heuristic evaluation subcommands (`apprise`, `docket`, `admit`, `vet`, `audit`).
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderTransportArgs {
    /// Maximum retry attempts per model call on transient `429`/`5xx`/timeout errors (`0` disables retries; default: `2`; env: `CANON_CLERK_MAX_RETRIES`).
    #[arg(
        long = "max-retries",
        value_name = "COUNT",
        allow_hyphen_values = true,
        help_heading = "Provider Transport & Retry",
        help = "Maximum retry attempts per model call on transient 429/5xx/timeout errors; 0 disables retries (default: 2; fallback env: CANON_CLERK_MAX_RETRIES)"
    )]
    pub max_retries: Option<String>,

    /// Maximum per-wait rate-limit cooldown or retry backoff in milliseconds (`0` fails fast on `429` waits; default: `90000`; env: `CANON_CLERK_MAX_BACKOFF_MS`).
    #[arg(
        long = "max-backoff-ms",
        value_name = "MS",
        allow_hyphen_values = true,
        help_heading = "Provider Transport & Retry",
        help = "Maximum per-wait rate-limit cooldown or retry backoff in milliseconds; 0 fails fast on 429 waits (default: 90000; fallback env: CANON_CLERK_MAX_BACKOFF_MS)"
    )]
    pub max_backoff_ms: Option<String>,
}

impl ProviderTransportArgs {
    /// Resolves [`RetryPolicy`] following standard specificity precedence (`CLI flag -> env -> config.json -> default`).
    pub fn resolve_retry_policy(
        &self,
        env: &BTreeMap<String, String>,
        stored: Option<&StoredConfig>,
    ) -> Result<RetryPolicy, CliError> {
        let max_retries = self.resolve_max_retries(env, stored)?;
        let max_backoff_ms = self.resolve_max_backoff_ms(env, stored)?;
        Ok(RetryPolicy::new(max_retries, max_backoff_ms))
    }

    fn resolve_max_retries(
        &self,
        env: &BTreeMap<String, String>,
        stored: Option<&StoredConfig>,
    ) -> Result<u32, CliError> {
        if let Some(raw_flag) = self.max_retries.as_deref() {
            return raw_flag.trim().parse::<u32>().map_err(|_| {
                CliError::usage(format!(
                    "Invalid `--max-retries` value `{raw_flag}`: expected a non-negative integer (`>= 0`).\n  \
                     Hint: Pass `--max-retries <COUNT>` (default: `{DEFAULT_MAX_RETRIES}`; pass `0` to disable retries) or set `{ENV_MAX_RETRIES}=<COUNT>`."
                ))
            });
        }

        if let Some(raw_env) = env
            .get(ENV_MAX_RETRIES)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            return raw_env.parse::<u32>().map_err(|_| {
                CliError::usage(format!(
                    "Invalid retry count `{raw_env}` in `{ENV_MAX_RETRIES}`: expected a non-negative integer (`>= 0`).\n  \
                     Hint: Set `{ENV_MAX_RETRIES}={DEFAULT_MAX_RETRIES}` (or `0` to disable retries) or override via `--max-retries <COUNT>`."
                ))
            });
        }

        Ok(stored
            .and_then(|s| s.defaults.max_retries)
            .unwrap_or(DEFAULT_MAX_RETRIES))
    }

    fn resolve_max_backoff_ms(
        &self,
        env: &BTreeMap<String, String>,
        stored: Option<&StoredConfig>,
    ) -> Result<u64, CliError> {
        if let Some(raw_flag) = self.max_backoff_ms.as_deref() {
            return raw_flag.trim().parse::<u64>().map_err(|_| {
                CliError::usage(format!(
                    "Invalid `--max-backoff-ms` value `{raw_flag}`: expected a non-negative integer in milliseconds (`>= 0`).\n  \
                     Hint: Pass `--max-backoff-ms <MS>` (default: `{DEFAULT_MAX_BACKOFF_MS}`; pass `0` to fail fast on rate limits) or set `{ENV_MAX_BACKOFF_MS}=<MS>`."
                ))
            });
        }

        if let Some(raw_env) = env
            .get(ENV_MAX_BACKOFF_MS)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            return raw_env.parse::<u64>().map_err(|_| {
                CliError::usage(format!(
                    "Invalid backoff cap `{raw_env}` in `{ENV_MAX_BACKOFF_MS}`: expected a non-negative integer in milliseconds (`>= 0`).\n  \
                     Hint: Set `{ENV_MAX_BACKOFF_MS}={DEFAULT_MAX_BACKOFF_MS}` (or `0` to fail fast on rate limits) or override via `--max-backoff-ms <MS>`."
                ))
            });
        }

        Ok(stored
            .and_then(|s| s.defaults.max_backoff_ms)
            .unwrap_or(DEFAULT_MAX_BACKOFF_MS))
    }
}

/// Composable CLI argument group for Caseload envelope I/O across all stage subcommands.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct CaseloadIoArgs {
    /// Read an existing upstream `Caseload` JSON from a file path or `-` for stdin.
    #[arg(
        long = "caseload",
        value_name = "PATH|-",
        help_heading = "Caseload & Output",
        help = "Ingest existing upstream Caseload JSON from file or '-' for stdin"
    )]
    pub caseload: Option<String>,

    /// Write the cumulative `Caseload` JSON to a file path independently of stdout.
    #[arg(
        long = "output-caseload",
        value_name = "PATH",
        help_heading = "Caseload & Output",
        help = "Write cumulative Caseload JSON to disk independently of stdout"
    )]
    pub output_caseload: Option<PathBuf>,

    /// Emit the enriched `Caseload` as JSON to standard output.
    #[arg(
        long = "json",
        help_heading = "Caseload & Output",
        help = "Emit enriched Caseload JSON to stdout"
    )]
    pub json: bool,
}

impl CaseloadIoArgs {
    /// Reads incoming Caseload JSON from `--caseload` if specified.
    pub fn read_caseload(&self, stdin: &mut dyn Read) -> Result<Option<Caseload>, CliError> {
        read_caseload_operand(self.caseload.as_deref(), stdin)
    }

    /// Writes the cumulative Caseload to `--output-caseload` if specified.
    pub fn write_output_caseload(&self, caseload: &Caseload) -> Result<(), CliError> {
        if let Some(path) = &self.output_caseload {
            write_caseload_file(path, caseload)?;
        }
        Ok(())
    }
}

pub use crate::cli::github_args::{GithubCheckArgs, ResolvedFilingSource};

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_canon_selection_args_is_empty() {
        let default_args = CanonSelectionArgs::default();
        assert!(default_args.is_empty());
        assert!(default_args.include_builtins());

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
    fn test_filing_source_args_resolve_intake_options() {
        let args = FilingSourceArgs {
            targets: vec!["src/lib.rs".to_string()],
            pr_title: Some("feat: test".to_string()),
            pr_body: Some("body".to_string()),
            intent: Some("intent".to_string()),
            all_targets: true,
            ..FilingSourceArgs::default()
        };
        let mut stdin = Cursor::new(b"");
        let opts = args
            .resolve_intake_options(&mut stdin)
            .expect("resolve intake options");
        assert_eq!(opts.pr_title.as_deref(), Some("feat: test"));
        assert_eq!(opts.pr_body.as_deref(), Some("body"));
        assert_eq!(opts.intent.as_deref(), Some("intent"));
        assert!(opts.all_targets);
        assert_eq!(opts.target_paths, Some(vec!["src/lib.rs".to_string()]));
    }

    #[test]
    fn test_provider_transport_args_specificity_cascade_and_actionable_validation_hints() {
        let empty_env = BTreeMap::new();
        let default_policy = ProviderTransportArgs::default()
            .resolve_retry_policy(&empty_env, None)
            .expect("default retry policy");
        assert_eq!(default_policy.max_retries, 2);
        assert_eq!(default_policy.max_backoff_ms, 90_000);

        let mut stored = StoredConfig::default();
        stored.defaults.max_retries = Some(4);
        stored.defaults.max_backoff_ms = Some(45_000);
        let from_store = ProviderTransportArgs::default()
            .resolve_retry_policy(&empty_env, Some(&stored))
            .expect("stored retry policy");
        assert_eq!(from_store, RetryPolicy::new(4, 45_000));

        let mut env = BTreeMap::new();
        env.insert(ENV_MAX_RETRIES.to_string(), "1".to_string());
        env.insert(ENV_MAX_BACKOFF_MS.to_string(), "15000".to_string());
        let from_env = ProviderTransportArgs::default()
            .resolve_retry_policy(&env, Some(&stored))
            .expect("env overrides stored");
        assert_eq!(from_env, RetryPolicy::new(1, 15_000));

        let explicit_zero = ProviderTransportArgs {
            max_retries: Some("0".to_string()),
            max_backoff_ms: Some("0".to_string()),
        }
        .resolve_retry_policy(&env, Some(&stored))
        .expect("CLI overrides env and accepts 0");
        assert_eq!(explicit_zero, RetryPolicy::new(0, 0));

        let bad_flag = ProviderTransportArgs {
            max_retries: Some("-1".to_string()),
            max_backoff_ms: None,
        }
        .resolve_retry_policy(&empty_env, None)
        .expect_err("negative --max-retries must fail");
        assert_eq!(bad_flag.exit_code(), 2);
        assert!(bad_flag.to_string().contains("Hint:"));

        let mut bad_env = BTreeMap::new();
        bad_env.insert(ENV_MAX_BACKOFF_MS.to_string(), "not-a-number".to_string());
        let bad_env_err = ProviderTransportArgs::default()
            .resolve_retry_policy(&bad_env, None)
            .expect_err("invalid CANON_CLERK_MAX_BACKOFF_MS must fail");
        assert_eq!(bad_env_err.exit_code(), 2);
        assert!(bad_env_err.to_string().contains("Hint:"));
    }
}
