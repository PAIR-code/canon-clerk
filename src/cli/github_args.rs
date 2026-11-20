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

//! Composable Clap argument group for opt-in GitHub Checks REST API publication
//! and CLI adapter helpers for native `--github-pr` filing ingestion.

use crate::caseload::WorkspaceState;
use crate::cli::CliError;
use crate::cli::args::FilingSourceArgs;
use crate::cli::io::{read_operand_source, resolve_target_operands};
use crate::github::{
    DEFAULT_GITHUB_CHECK_NAME, GithubCheckTarget, GithubChecksClient, GithubPullRequestFetcher,
    is_pull_request_target_event, resolve_github_check_target_with_fallback_sha,
    resolve_github_pr_filing,
};
use crate::intake::IntakeOptions;
use clap::Args;
use std::collections::BTreeMap;
use std::io::Read;

/// Resolved filing options alongside optional PR head commit SHA fetched via `--github-pr`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFilingSource {
    /// Pure in-memory domain [`IntakeOptions`] descriptor passed to `src/intake/`.
    pub intake: IntakeOptions,
    /// Optional pull request head commit SHA resolved from `$GITHUB_EVENT_PATH` or the GitHub PR API.
    pub fetched_pr_head_sha: Option<String>,
}

/// Resolves [`FilingSourceArgs`] into a pure [`IntakeOptions`] descriptor and optional fetched PR head SHA,
/// executing GitHub REST API calls via `fetcher` strictly at the CLI adapter boundary when `--github-pr` is active.
pub(crate) fn resolve_filing_source_with_fetcher(
    args: &FilingSourceArgs,
    stdin: &mut dyn Read,
    env: &BTreeMap<String, String>,
    fetcher: &dyn GithubPullRequestFetcher,
) -> Result<ResolvedFilingSource, CliError> {
    if args.diff.is_some() && args.github_pr.is_some() {
        return Err(CliError::usage(
            "Flags `--diff` and `--github-pr` are mutually exclusive.\n  \
             Hint: Pass either `--diff <PATH|->` for a local patch or `--github-pr [<NUMBER>]` to fetch the PR diff from GitHub.",
        ));
    }

    let explicit_pr_body = if let Some(body) = &args.pr_body {
        Some(body.clone())
    } else if let Some(path) = &args.pr_body_file {
        Some(read_operand_source(path, "--pr-body-file", stdin)?)
    } else {
        None
    };

    let target_paths = resolve_target_operands(&args.targets, stdin)?;

    if let Some(pr_number_override) = args.github_pr {
        let resolved = resolve_github_pr_filing(
            pr_number_override,
            args.github_repo.as_deref(),
            env,
            fetcher,
        )
        .map_err(|err| CliError::usage(err.to_string()))?;

        let pr_title = args.pr_title.clone().or(resolved.metadata.title);
        let pr_body = explicit_pr_body.or(resolved.metadata.body);
        let workspace_state = args.workspace_state.map(WorkspaceState::from).or_else(|| {
            if is_pull_request_target_event(env) {
                Some(WorkspaceState::Base)
            } else {
                None
            }
        });

        return Ok(ResolvedFilingSource {
            intake: IntakeOptions {
                pr_title,
                pr_body,
                intent: args.intent.clone(),
                target_paths,
                patch_content: Some(resolved.diff),
                all_targets: args.all_targets,
                workspace_state,
                linked_issues: None,
            },
            fetched_pr_head_sha: resolved.metadata.head_sha,
        });
    }

    let patch_content = match &args.diff {
        Some(source) => Some(read_operand_source(source, "--diff", stdin)?),
        None => None,
    };

    Ok(ResolvedFilingSource {
        intake: IntakeOptions {
            pr_title: args.pr_title.clone(),
            pr_body: explicit_pr_body,
            intent: args.intent.clone(),
            target_paths,
            patch_content,
            all_targets: args.all_targets,
            workspace_state: args.workspace_state.map(WorkspaceState::from),
            linked_issues: None,
        },
        fetched_pr_head_sha: None,
    })
}

/// Resolves [`FilingSourceArgs`] using the live [`GithubChecksClient`] when `--github-pr` is active.
pub(crate) fn resolve_filing_source(
    args: &FilingSourceArgs,
    stdin: &mut dyn Read,
    env: &BTreeMap<String, String>,
) -> Result<ResolvedFilingSource, CliError> {
    let client = GithubChecksClient::new();
    resolve_filing_source_with_fetcher(args, stdin, env, &client)
}

/// Composable CLI argument group for opt-in GitHub Checks REST API publication.
///
/// Shares `--github-repo` with [`FilingSourceArgs`] to avoid Clap argument collisions.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct GithubCheckArgs {
    /// Publish stage findings as a GitHub Check Run via the Checks REST API (batched 50 annotations/request).
    #[arg(
        long = "publish-github-check",
        help_heading = "GitHub Checks Publication",
        help = "Publish findings via the GitHub Checks REST API (requires GITHUB_TOKEN or GH_TOKEN with checks:write)"
    )]
    pub publish_github_check: bool,

    /// Target commit SHA to annotate (fallback: `pull_request.head.sha` in `GITHUB_EVENT_PATH`, fetched PR API `head.sha`, then `GITHUB_SHA`).
    #[arg(
        long = "github-head-sha",
        value_name = "SHA",
        help_heading = "GitHub Checks Publication",
        help = "Target commit SHA (fallback: pull_request.head.sha in GITHUB_EVENT_PATH, PR API head.sha, then GITHUB_SHA)"
    )]
    pub github_head_sha: Option<String>,

    /// Name of the published GitHub Check Run (default: `"Canon Clerk Audit"`).
    #[arg(
        long = "github-check-name",
        value_name = "NAME",
        help_heading = "GitHub Checks Publication",
        help = "GitHub Check Run name (default: 'Canon Clerk Audit')"
    )]
    pub github_check_name: Option<String>,
}

impl GithubCheckArgs {
    /// Validates and resolves the GitHub Checks publication target when `--publish-github-check` is active.
    pub fn resolve_target(
        &self,
        repo_flag: Option<&str>,
        fallback_pr_head_sha: Option<&str>,
        env: &BTreeMap<String, String>,
    ) -> Result<Option<GithubCheckTarget>, CliError> {
        if !self.publish_github_check {
            return Ok(None);
        }
        resolve_github_check_target_with_fallback_sha(
            repo_flag,
            self.github_head_sha.as_deref(),
            fallback_pr_head_sha,
            env,
        )
        .map(Some)
        .map_err(|err| CliError::usage(err.to_string()))
    }

    /// Returns the resolved Check Run name, defaulting to [`DEFAULT_GITHUB_CHECK_NAME`].
    pub fn resolved_check_name(&self) -> String {
        self.github_check_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_GITHUB_CHECK_NAME)
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::args::WorkspaceStateArg;
    use crate::cli::{Cli, Commands};
    use crate::github::{
        ENV_GITHUB_EVENT_NAME, ENV_GITHUB_REPOSITORY, ENV_GITHUB_SHA, ENV_GITHUB_TOKEN,
        GithubChecksError, GithubPullRequestMetadata, GithubPullRequestTarget,
    };
    use clap::Parser;
    use std::io::Cursor;

    struct StaticPrFetcher {
        metadata: GithubPullRequestMetadata,
        diff: String,
    }

    impl GithubPullRequestFetcher for StaticPrFetcher {
        fn fetch_pull_request_metadata(
            &self,
            _target: &GithubPullRequestTarget,
        ) -> Result<GithubPullRequestMetadata, GithubChecksError> {
            Ok(self.metadata.clone())
        }

        fn fetch_pull_request_diff(
            &self,
            _target: &GithubPullRequestTarget,
        ) -> Result<String, GithubChecksError> {
            Ok(self.diff.clone())
        }
    }

    #[test]
    fn test_clap_parses_github_pr_and_shared_github_repo_without_collision() {
        let audit_bare = Cli::try_parse_from([
            "canon-clerk",
            "audit",
            "--github-pr",
            "--github-repo",
            "PAIR-code/canon-clerk",
            "--publish-github-check",
            "--on-missing-credentials",
            "skip",
        ])
        .expect("audit with bare --github-pr and shared --github-repo");
        match audit_bare.command {
            Some(Commands::Audit(args)) => {
                assert_eq!(args.filing.github_pr, Some(None));
                assert_eq!(
                    args.filing.github_repo.as_deref(),
                    Some("PAIR-code/canon-clerk")
                );
                assert!(args.github_check.publish_github_check);
            }
            _ => panic!("expected Audit"),
        }

        let intake_num = Cli::try_parse_from(["canon-clerk", "intake", "--github-pr", "358"])
            .expect("intake --github-pr 358");
        match intake_num.command {
            Some(Commands::Intake(args)) => assert_eq!(args.filing.github_pr, Some(Some(358))),
            _ => panic!("expected Intake"),
        }

        let vet_num = Cli::try_parse_from(["canon-clerk", "vet", "--github-pr", "42"])
            .expect("vet --github-pr 42");
        match vet_num.command {
            Some(Commands::Vet(args)) => assert_eq!(args.filing.github_pr, Some(Some(42))),
            _ => panic!("expected Vet"),
        }

        let conflict =
            Cli::try_parse_from(["canon-clerk", "intake", "--github-pr", "42", "--diff", "-"]);
        assert!(
            conflict.is_err(),
            "--github-pr and --diff must conflict in Clap"
        );
    }

    #[test]
    fn test_metadata_override_precedence_and_workspace_state_heuristic() {
        let mut env = BTreeMap::new();
        env.insert(ENV_GITHUB_TOKEN.to_string(), "tok".to_string());
        env.insert(ENV_GITHUB_REPOSITORY.to_string(), "o/r".to_string());
        env.insert(
            ENV_GITHUB_EVENT_NAME.to_string(),
            "pull_request_target".to_string(),
        );

        let fetcher = StaticPrFetcher {
            metadata: GithubPullRequestMetadata {
                title: Some("API title".to_string()),
                body: Some("API body".to_string()),
                head_sha: Some("pr-head-sha-123".to_string()),
            },
            diff: "diff --git a/src/lib.rs b/src/lib.rs\n".to_string(),
        };

        // 1. Default workspace_state under pull_request_target -> Base; CLI pr_title overrides API title.
        let args = FilingSourceArgs {
            github_pr: Some(Some(7)),
            pr_title: Some("CLI title".to_string()),
            ..FilingSourceArgs::default()
        };
        let resolved =
            resolve_filing_source_with_fetcher(&args, &mut Cursor::new(b""), &env, &fetcher)
                .expect("resolve");
        assert_eq!(resolved.intake.pr_title.as_deref(), Some("CLI title"));
        assert_eq!(resolved.intake.pr_body.as_deref(), Some("API body"));
        assert_eq!(resolved.intake.workspace_state, Some(WorkspaceState::Base));
        assert_eq!(
            resolved.fetched_pr_head_sha.as_deref(),
            Some("pr-head-sha-123")
        );

        // 2. Explicit --workspace-state head overrides pull_request_target heuristic.
        let explicit_head = FilingSourceArgs {
            github_pr: Some(Some(7)),
            workspace_state: Some(WorkspaceStateArg::Head),
            ..FilingSourceArgs::default()
        };
        let resolved_head = resolve_filing_source_with_fetcher(
            &explicit_head,
            &mut Cursor::new(b""),
            &env,
            &fetcher,
        )
        .expect("resolve head");
        assert_eq!(
            resolved_head.intake.workspace_state,
            Some(WorkspaceState::Head)
        );

        // 3. Non-pull_request_target event preserves default Head (None).
        env.insert(
            ENV_GITHUB_EVENT_NAME.to_string(),
            "pull_request".to_string(),
        );
        let resolved_pr = resolve_filing_source_with_fetcher(
            &FilingSourceArgs {
                github_pr: Some(Some(7)),
                ..FilingSourceArgs::default()
            },
            &mut Cursor::new(b""),
            &env,
            &fetcher,
        )
        .expect("resolve pull_request");
        assert_eq!(resolved_pr.intake.workspace_state, None);

        // 4. Check target falls back to fetched PR API head.sha before GITHUB_SHA.
        env.insert(ENV_GITHUB_SHA.to_string(), "base-branch-sha".to_string());
        let check_args = GithubCheckArgs {
            publish_github_check: true,
            ..GithubCheckArgs::default()
        };
        let target = check_args
            .resolve_target(None, resolved.fetched_pr_head_sha.as_deref(), &env)
            .expect("resolve check target")
            .expect("some target");
        assert_eq!(target.head_sha, "pr-head-sha-123");
    }
}
