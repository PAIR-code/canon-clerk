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

use crate::caseload::{ThinkingLevel, WorkspaceState};
use crate::cli::CliError;
use crate::cli::github_args::{
    resolve_filing_source, resolve_filing_source_with_fetcher, resolve_filing_source_with_issue,
};
use crate::configure::{ConfigureError, parse_thinking_level};
use crate::github::GithubPullRequestFetcher;
use crate::intake::IntakeOptions;
use clap::{Args, ValueEnum};
use std::collections::BTreeMap;
use std::io::Read;
use std::num::NonZeroU64;

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

    /// Repeatable JSON file path or `-` for stdin supplying linked issue metadata (`SPEC.md` §4.2).
    #[arg(
        long = "linked-issue",
        value_name = "PATH|-",
        help_heading = "Pull Request Metadata",
        help = "Ingest linked issue JSON (single object or array; native, gh, or GitHub REST shape) from file or '-' for stdin"
    )]
    pub linked_issues: Vec<String>,

    /// Repeatable literal commit message text (`SPEC.md` §4.2; mutually exclusive with `--commit-messages-file`).
    #[arg(
        long = "commit-message",
        value_name = "TEXT",
        conflicts_with = "commit_messages_files",
        help_heading = "Pull Request Metadata",
        help = "Ingest a commit message literal (repeatable; preserves chronological order)"
    )]
    pub commit_messages: Vec<String>,

    /// Repeatable JSON file path or `-` for stdin supplying commit messages (`SPEC.md` §4.2; mutually exclusive with `--commit-message`).
    #[arg(
        long = "commit-messages-file",
        value_name = "PATH|-",
        conflicts_with = "commit_messages",
        help_heading = "Pull Request Metadata",
        help = "Ingest commit messages JSON (string/array, native, GitHub REST, or gh CLI shape) from file or '-' for stdin"
    )]
    pub commit_messages_files: Vec<String>,
}

impl FilingSourceArgs {
    /// Resolves flags, file paths, stdin streams, and optional `--github-pr` inputs using process env.
    pub fn resolve_intake_options(&self, stdin: &mut dyn Read) -> Result<IntakeOptions, CliError> {
        let env: BTreeMap<String, String> = std::env::vars().collect();
        self.resolve_intake_options_with_env(stdin, &env)
            .map(|resolved| resolved.intake)
    }

    /// Resolves flags and optional `--github-pr` / `--github-issue` inputs against an explicit environment map.
    ///
    /// - When `github_issue` is `None`, GitHub Issue inquiry ingestion is disabled.
    /// - When `github_issue` is `Some(None)` (bare `--github-issue`), reads issue metadata from `$GITHUB_EVENT_PATH` with zero API calls.
    /// - When `github_issue` is `Some(Some(number))`, fetches the issue from GitHub's REST API using `GITHUB_TOKEN`/`GH_TOKEN` and `--github-repo`/`GITHUB_REPOSITORY`.
    ///
    /// # Errors
    /// Returns [`CliError`] (usage exit code `2`) on conflicting filing flags (`--diff` with `--github-pr`, or
    /// `--github-pr` with `--github-issue`), unreadable operand files/stdin, or invalid/failed GitHub PR or Issue resolution.
    pub fn resolve_intake_options_with_issue_and_env(
        &self,
        github_issue: Option<Option<NonZeroU64>>,
        stdin: &mut dyn Read,
        env: &BTreeMap<String, String>,
    ) -> Result<ResolvedFilingSource, CliError> {
        resolve_filing_source_with_issue(self, github_issue, stdin, env)
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

pub use crate::cli::canon_args::CanonSelectionArgs;

/// Composable CLI argument group for active provider selection, reasoning effort, and provider credentials (Branch B).
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderConfigArgs {
    /// Active default provider slug (`google`, `typesafe`, `local`, or custom; env: `CANON_CLERK_PROVIDER`; config key: `defaults.provider`).
    #[arg(
        long = "provider",
        value_name = "SLUG",
        help_heading = "Model & Reasoning Options",
        help = "Default model provider slug: google, typesafe, local, or custom (fallback env: CANON_CLERK_PROVIDER)"
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

    /// Optional Bearer API key override for built-in local System One daemons (fallback env: `CANON_CLERK_LOCAL_API_KEY`, `LOCAL_API_KEY`).
    #[arg(
        long = "local-api-key",
        value_name = "KEY",
        help_heading = "Provider Credentials",
        help = "Optional Bearer API key for local System One daemons (fallback env: LOCAL_API_KEY)"
    )]
    pub local_api_key: Option<String>,

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

    /// Built-in local System One daemon base URL override (default: `http://127.0.0.1:8080/v1`; fallback env: `CANON_CLERK_LOCAL_BASE_URL`, `LOCAL_BASE_URL`).
    #[arg(
        long = "local-base-url",
        value_name = "URL",
        help_heading = "Provider Endpoints",
        help = "Local System One daemon base URL (default: http://127.0.0.1:8080/v1; fallback env: LOCAL_BASE_URL)"
    )]
    pub local_base_url: Option<String>,

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

pub use crate::cli::github_args::{
    GithubCheckArgs, GithubIssueArgs, RejectedGithubIssueArgs, ResolvedFilingSource,
};
pub use crate::cli::io::CaseloadIoArgs;
pub use crate::cli::transport_args::ProviderTransportArgs;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

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
}
