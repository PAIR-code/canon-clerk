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
use crate::cli::commit_messages::resolve_commit_message_operands;
use crate::cli::io::{read_operand_source, resolve_target_operands};
use crate::cli::linked_issues::resolve_linked_issue_operands;
use crate::github::{
    DEFAULT_GITHUB_BYPASS_LABEL, DEFAULT_GITHUB_CHECK_NAME, ENV_CANON_CLERK_GITHUB_BYPASS_LABEL,
    GithubCheckTarget, GithubChecksClient, GithubIssueFetcher, GithubPullRequestFetcher,
    fetch_linked_issues_for_pr_body, is_pull_request_target_event,
    resolve_github_check_target_with_fallback_sha, resolve_github_issue_inquiry,
    resolve_github_pr_filing,
};
use crate::intake::IntakeOptions;
use clap::Args;
use std::collections::BTreeMap;
use std::io::Read;
use std::num::NonZeroU64;

/// Resolved filing options alongside optional PR metadata fetched via `--github-pr`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFilingSource {
    /// Pure in-memory domain [`IntakeOptions`] descriptor passed to `src/intake/`.
    pub intake: IntakeOptions,
    /// Optional pull request head commit SHA resolved from `$GITHUB_EVENT_PATH` or the GitHub PR API.
    pub fetched_pr_head_sha: Option<String>,
    /// Optional pull request label names resolved from `$GITHUB_EVENT_PATH` or the GitHub PR API.
    pub fetched_pr_labels: Option<Vec<String>>,
    /// Optional event actor login resolved from `$GITHUB_EVENT_PATH` (`sender.login`).
    pub fetched_pr_actor: Option<String>,
}

/// Composable CLI argument group for opt-in GitHub Issue inquiry ingestion (`intake` and `apprise` only).
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct GithubIssueArgs {
    /// Ingest a GitHub Issue as prospective design intent (omit NUMBER to read `$GITHUB_EVENT_PATH`).
    #[arg(
        long = "github-issue",
        value_name = "NUMBER",
        num_args = 0..=1,
        conflicts_with = "github_pr",
        help_heading = "Filing Sources",
        help = "Ingest a GitHub Issue as prospective design intent (omit NUMBER to read from GITHUB_EVENT_PATH)"
    )]
    pub github_issue: Option<Option<NonZeroU64>>,
}

/// Hidden Clap capture rejecting `--github-issue` on stages outside `intake` and `apprise`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct RejectedGithubIssueArgs {
    /// Hidden capture rejecting `--github-issue` with an actionable stage-specific `Hint:`.
    #[arg(
        long = "github-issue",
        hide = true,
        num_args = 0..=1,
        allow_hyphen_values = true
    )]
    pub rejected_github_issue: Option<Option<String>>,
}

impl RejectedGithubIssueArgs {
    /// Rejects `--github-issue` on contentious-track stages (`docket`, `admit`, `vet`, `audit`) with exit code `2`.
    pub fn validate_contentious(&self, stage: &str) -> Result<(), CliError> {
        if self.rejected_github_issue.is_some() {
            return Err(CliError::usage(format!(
                "Flag `--github-issue` is not supported on `canon-clerk {stage}` (contentious track).\n  \
                 Hint: Pass `--linked-issue <PATH|->` (or `--github-pr [<NUMBER>]`) to attach linked issue context to a PR/diff audit, \
                 or use `canon-clerk apprise --github-issue [<NUMBER>]` for prospective issue screening."
            )));
        }
        Ok(())
    }

    /// Rejects `--github-issue` on intermediate stages (`discover`, `validate`) with exit code `2`.
    pub fn validate_intermediate(&self, stage: &str) -> Result<(), CliError> {
        if self.rejected_github_issue.is_some() {
            return Err(CliError::usage(format!(
                "Flag `--github-issue` is scoped to `intake` and `apprise`, not `canon-clerk {stage}`.\n  \
                 Hint: Run `canon-clerk apprise --github-issue [<NUMBER>]` for prospective issue screening, \
                 or pipe `canon-clerk intake --github-issue [<NUMBER>] --json | canon-clerk {stage} --caseload -`."
            )));
        }
        Ok(())
    }
}

/// Resolves [`FilingSourceArgs`] and optional `--github-issue` into a pure [`IntakeOptions`] descriptor
/// and optional fetched PR head SHA, executing GitHub REST API calls strictly at the CLI adapter boundary.
pub(crate) fn resolve_filing_source_with_issue_and_fetchers(
    args: &FilingSourceArgs,
    github_issue: Option<Option<NonZeroU64>>,
    stdin: &mut dyn Read,
    env: &BTreeMap<String, String>,
    pr_fetcher: &dyn GithubPullRequestFetcher,
    issue_fetcher: &dyn GithubIssueFetcher,
) -> Result<ResolvedFilingSource, CliError> {
    if args.diff.is_some() && args.github_pr.is_some() {
        return Err(CliError::usage(
            "Flags `--diff` and `--github-pr` are mutually exclusive.\n  \
             Hint: Pass either `--diff <PATH|->` for a local patch or `--github-pr [<NUMBER>]` to fetch the PR diff from GitHub.",
        ));
    }
    if args.github_pr.is_some() && github_issue.is_some() {
        return Err(CliError::usage(
            "Flags `--github-pr` and `--github-issue` are mutually exclusive.\n  \
             Hint: Pass `--github-pr [<NUMBER>]` for pull request audits or `--github-issue [<NUMBER>]` for prospective issue inquiry.",
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
    let explicit_linked_issues = resolve_linked_issue_operands(&args.linked_issues, stdin)?;
    let explicit_commit_messages =
        resolve_commit_message_operands(&args.commit_messages, &args.commit_messages_files, stdin)?;

    if let Some(pr_number_override) = args.github_pr {
        return resolve_github_pr_source(
            args,
            pr_number_override,
            explicit_pr_body,
            target_paths,
            explicit_linked_issues,
            explicit_commit_messages,
            env,
            pr_fetcher,
        );
    }

    let intent = if let Some(issue_number_override) = github_issue {
        let resolved_issue = resolve_github_issue_inquiry(
            issue_number_override,
            args.github_repo.as_deref(),
            env,
            issue_fetcher,
        )
        .map_err(|err| CliError::usage(err.to_string()))?;
        args.intent
            .clone()
            .or_else(|| Some(resolved_issue.format_as_intent()))
    } else {
        args.intent.clone()
    };

    let patch_content = match &args.diff {
        Some(source) => Some(read_operand_source(source, "--diff", stdin)?),
        None => None,
    };

    Ok(ResolvedFilingSource {
        intake: IntakeOptions {
            pr_title: args.pr_title.clone(),
            pr_body: explicit_pr_body,
            intent,
            target_paths,
            patch_content,
            all_targets: args.all_targets,
            workspace_state: args.workspace_state.map(WorkspaceState::from),
            linked_issues: explicit_linked_issues.filter(|v| !v.is_empty()),
            commit_messages: explicit_commit_messages.filter(|v| !v.is_empty()),
        },
        fetched_pr_head_sha: None,
        fetched_pr_labels: None,
        fetched_pr_actor: None,
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "Internal filing resolver helper separating --github-pr resolution from local/issue sources"
)]
fn resolve_github_pr_source(
    args: &FilingSourceArgs,
    pr_number_override: Option<u64>,
    explicit_pr_body: Option<String>,
    target_paths: Option<Vec<String>>,
    explicit_linked_issues: Option<Vec<crate::caseload::LinkedIssueContext>>,
    explicit_commit_messages: Option<Vec<crate::caseload::CommitMessageContext>>,
    env: &BTreeMap<String, String>,
    pr_fetcher: &dyn GithubPullRequestFetcher,
) -> Result<ResolvedFilingSource, CliError> {
    let resolved = resolve_github_pr_filing(
        pr_number_override,
        args.github_repo.as_deref(),
        env,
        pr_fetcher,
    )
    .map_err(|err| CliError::usage(err.to_string()))?;

    let pr_title = args.pr_title.clone().or(resolved.metadata.title);
    let pr_body = explicit_pr_body.or(resolved.metadata.body);
    let linked_issues = match explicit_linked_issues {
        Some(explicit) => Some(explicit).filter(|v| !v.is_empty()),
        None => {
            let fetched =
                fetch_linked_issues_for_pr_body(&resolved.target, pr_body.as_deref(), pr_fetcher);
            Some(fetched).filter(|v| !v.is_empty())
        }
    };
    let commit_messages = match explicit_commit_messages {
        Some(explicit) => Some(explicit).filter(|v| !v.is_empty()),
        None => {
            let fetched = pr_fetcher
                .fetch_pull_request_commits(&resolved.target)
                .map_err(|err| CliError::usage(err.to_string()))?;
            Some(fetched).filter(|v| !v.is_empty())
        }
    };
    let workspace_state = args.workspace_state.map(WorkspaceState::from).or_else(|| {
        if is_pull_request_target_event(env) {
            Some(WorkspaceState::Base)
        } else {
            None
        }
    });

    Ok(ResolvedFilingSource {
        intake: IntakeOptions {
            pr_title,
            pr_body,
            intent: args.intent.clone(),
            target_paths,
            patch_content: Some(resolved.diff),
            all_targets: args.all_targets,
            workspace_state,
            linked_issues,
            commit_messages,
        },
        fetched_pr_head_sha: resolved.metadata.head_sha,
        fetched_pr_labels: Some(resolved.metadata.labels),
        fetched_pr_actor: resolved.metadata.actor,
    })
}

/// Resolves [`FilingSourceArgs`] into a pure [`IntakeOptions`] descriptor and optional fetched PR head SHA,
/// executing GitHub REST API calls via `fetcher` strictly at the CLI adapter boundary when `--github-pr` is active.
pub(crate) fn resolve_filing_source_with_fetcher(
    args: &FilingSourceArgs,
    stdin: &mut dyn Read,
    env: &BTreeMap<String, String>,
    fetcher: &dyn GithubPullRequestFetcher,
) -> Result<ResolvedFilingSource, CliError> {
    let client = GithubChecksClient::new();
    resolve_filing_source_with_issue_and_fetchers(args, None, stdin, env, fetcher, &client)
}

/// Resolves [`FilingSourceArgs`] and optional `--github-issue` using the live [`GithubChecksClient`].
pub(crate) fn resolve_filing_source_with_issue(
    args: &FilingSourceArgs,
    github_issue: Option<Option<NonZeroU64>>,
    stdin: &mut dyn Read,
    env: &BTreeMap<String, String>,
) -> Result<ResolvedFilingSource, CliError> {
    let client = GithubChecksClient::new();
    resolve_filing_source_with_issue_and_fetchers(args, github_issue, stdin, env, &client, &client)
}

/// Resolves [`FilingSourceArgs`] using the live [`GithubChecksClient`] when `--github-pr` is active.
pub(crate) fn resolve_filing_source(
    args: &FilingSourceArgs,
    stdin: &mut dyn Read,
    env: &BTreeMap<String, String>,
) -> Result<ResolvedFilingSource, CliError> {
    resolve_filing_source_with_issue(args, None, stdin, env)
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

    /// Pull request label that bypasses audit evaluation and publishes a `skipped` Check Run (bare flag defaults to `"skip-audit"`; fallback env: `CANON_CLERK_GITHUB_BYPASS_LABEL`).
    #[arg(
        long = "github-bypass-label",
        value_name = "LABEL",
        num_args = 0..=1,
        default_missing_value = DEFAULT_GITHUB_BYPASS_LABEL,
        help_heading = "GitHub Checks Publication",
        help = "PR label that bypasses audit and publishes a skipped Check Run (bare flag defaults to 'skip-audit'; fallback env: CANON_CLERK_GITHUB_BYPASS_LABEL)"
    )]
    pub github_bypass_label: Option<String>,
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

    /// Resolves the optional PR bypass label (`--github-bypass-label` > `CANON_CLERK_GITHUB_BYPASS_LABEL`).
    pub fn resolve_bypass_label(
        &self,
        env: &BTreeMap<String, String>,
    ) -> Result<Option<String>, CliError> {
        if let Some(raw) = &self.github_bypass_label {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(CliError::usage(format!(
                    "Invalid empty value for `--github-bypass-label`.\n  \
                     Hint: Pass a non-empty GitHub PR label name (e.g., `--github-bypass-label {DEFAULT_GITHUB_BYPASS_LABEL}`) or omit the flag."
                )));
            }
            return Ok(Some(trimmed.to_string()));
        }

        let from_env = env
            .get(ENV_CANON_CLERK_GITHUB_BYPASS_LABEL)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);
        Ok(from_env)
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
                labels: vec!["skip-audit".to_string()],
                actor: None,
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

    mod bypass_label_cli_args {
        use super::*;

        #[test]
        fn test_bypass_label_flag_defaults_overrides_and_env_fallback() {
            let bare = Cli::try_parse_from([
                "canon-clerk",
                "audit",
                "--github-pr",
                "--publish-github-check",
                "--github-bypass-label",
            ])
            .expect("bare --github-bypass-label must parse");
            let empty_env = BTreeMap::new();
            match bare.command {
                Some(Commands::Audit(args)) => {
                    assert_eq!(
                        args.github_check
                            .resolve_bypass_label(&empty_env)
                            .expect("resolve bare"),
                        Some(DEFAULT_GITHUB_BYPASS_LABEL.to_string())
                    );
                }
                _ => panic!("expected Audit"),
            }

            let explicit = Cli::try_parse_from([
                "canon-clerk",
                "audit",
                "--github-pr",
                "--github-bypass-label",
                "emergency-bypass",
            ])
            .expect("explicit --github-bypass-label must parse");
            let mut env_with_fallback = BTreeMap::new();
            env_with_fallback.insert(
                ENV_CANON_CLERK_GITHUB_BYPASS_LABEL.to_string(),
                "env-bypass".to_string(),
            );
            match explicit.command {
                Some(Commands::Audit(args)) => {
                    assert_eq!(
                        args.github_check
                            .resolve_bypass_label(&env_with_fallback)
                            .expect("flag overrides env"),
                        Some("emergency-bypass".to_string())
                    );
                }
                _ => panic!("expected Audit"),
            }

            let omitted = Cli::try_parse_from(["canon-clerk", "audit", "--github-pr"])
                .expect("omitted flag must parse");
            match omitted.command {
                Some(Commands::Audit(args)) => {
                    assert_eq!(
                        args.github_check
                            .resolve_bypass_label(&empty_env)
                            .expect("disabled by default"),
                        None
                    );
                    assert_eq!(
                        args.github_check
                            .resolve_bypass_label(&env_with_fallback)
                            .expect("env fallback"),
                        Some("env-bypass".to_string())
                    );
                }
                _ => panic!("expected Audit"),
            }

            let empty_flag = GithubCheckArgs {
                github_bypass_label: Some("   ".to_string()),
                ..GithubCheckArgs::default()
            };
            let err = empty_flag
                .resolve_bypass_label(&empty_env)
                .expect_err("whitespace-only bypass label must fail fast");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("Hint:"));
        }
    }

    mod linked_issues_cli_args {
        use super::*;
        use crate::caseload::LinkedIssueContext;
        use std::sync::Mutex;

        struct TrackingIssueFetcher {
            metadata: GithubPullRequestMetadata,
            issues: BTreeMap<u64, LinkedIssueContext>,
            issue_calls: Mutex<Vec<u64>>,
        }

        impl GithubPullRequestFetcher for TrackingIssueFetcher {
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
                Ok("diff --git a/src/lib.rs b/src/lib.rs\n".to_string())
            }

            fn fetch_issue(
                &self,
                _target: &GithubPullRequestTarget,
                issue_number: u64,
            ) -> Result<Option<LinkedIssueContext>, GithubChecksError> {
                self.issue_calls.lock().unwrap().push(issue_number);
                Ok(self.issues.get(&issue_number).cloned())
            }
        }

        #[test]
        fn test_github_pr_discovers_and_fetches_linked_issues_and_cli_overrides() {
            let mut env = BTreeMap::new();
            env.insert(ENV_GITHUB_TOKEN.to_string(), "tok".to_string());
            env.insert(ENV_GITHUB_REPOSITORY.to_string(), "o/r".to_string());

            let mut issues = BTreeMap::new();
            issues.insert(
                383,
                LinkedIssueContext {
                    number: 383,
                    title: Some("Ingest linked_issues".to_string()),
                    author: Some("jimbojw".to_string()),
                    labels: vec!["type: feat".to_string()],
                    body: Some("Issue 383 body".to_string()),
                    url: Some("https://github.com/o/r/issues/383".to_string()),
                },
            );

            let fetcher = TrackingIssueFetcher {
                metadata: GithubPullRequestMetadata {
                    title: Some("feat: linked issues".to_string()),
                    body: Some("Closes #383".to_string()),
                    head_sha: Some("sha123".to_string()),
                    labels: Vec::new(),
                    actor: None,
                },
                issues,
                issue_calls: Mutex::new(Vec::new()),
            };

            // 1. Auto-fetch from resolved PR body
            let args = FilingSourceArgs {
                github_pr: Some(Some(99)),
                ..FilingSourceArgs::default()
            };
            let resolved =
                resolve_filing_source_with_fetcher(&args, &mut Cursor::new(b""), &env, &fetcher)
                    .expect("resolve with auto-fetched linked issues");
            assert_eq!(*fetcher.issue_calls.lock().unwrap(), vec![383]);
            let fetched = resolved.intake.linked_issues.expect("linked_issues");
            assert_eq!(fetched.len(), 1);
            assert_eq!(fetched[0].number, 383);
            assert_eq!(fetched[0].author.as_deref(), Some("jimbojw"));

            // 2. Explicit `--linked-issue -` overrides auto-fetch and skips GitHub Issues API
            fetcher.issue_calls.lock().unwrap().clear();
            let override_args = FilingSourceArgs {
                github_pr: Some(Some(99)),
                linked_issues: vec!["-".to_string()],
                ..FilingSourceArgs::default()
            };
            let override_json =
                r#"{"number":7,"title":"CLI override","author":{"login":"override-bot"}}"#;
            let resolved_override = resolve_filing_source_with_fetcher(
                &override_args,
                &mut Cursor::new(override_json),
                &env,
                &fetcher,
            )
            .expect("resolve with explicit linked issue override");
            assert!(
                fetcher.issue_calls.lock().unwrap().is_empty(),
                "explicit --linked-issue must skip GitHub Issues API calls"
            );
            let overridden = resolved_override
                .intake
                .linked_issues
                .expect("overridden linked_issues");
            assert_eq!(overridden.len(), 1);
            assert_eq!(overridden[0].number, 7);
            assert_eq!(overridden[0].author.as_deref(), Some("override-bot"));

            // 3. Local CLI `--linked-issue` without `--github-pr`
            let local_parsed = Cli::try_parse_from([
                "canon-clerk",
                "intake",
                "--linked-issue",
                "a.json",
                "--linked-issue",
                "b.json",
            ])
            .expect("repeatable --linked-issue parses");
            match local_parsed.command {
                Some(Commands::Intake(cmd)) => {
                    assert_eq!(cmd.filing.linked_issues, vec!["a.json", "b.json"]);
                }
                _ => panic!("expected Intake"),
            }
        }
    }

    mod commit_messages_cli_args {
        use super::*;
        use crate::caseload::CommitMessageContext;
        use crate::cli::Cli;
        use crate::cli::io::ensure_single_stdin_source;
        use crate::github::{GithubChecksError, GithubPullRequestTarget};
        use clap::Parser;
        use std::io::Cursor;
        use std::sync::Mutex;

        struct TrackingCommitsFetcher {
            commits: Result<Vec<CommitMessageContext>, GithubChecksError>,
            commit_calls: Mutex<usize>,
        }

        impl GithubPullRequestFetcher for TrackingCommitsFetcher {
            fn fetch_pull_request_metadata(
                &self,
                _target: &GithubPullRequestTarget,
            ) -> Result<GithubPullRequestMetadata, GithubChecksError> {
                Ok(GithubPullRequestMetadata::default())
            }

            fn fetch_pull_request_diff(
                &self,
                _target: &GithubPullRequestTarget,
            ) -> Result<String, GithubChecksError> {
                Ok("diff --git a/src/lib.rs b/src/lib.rs\n".to_string())
            }

            fn fetch_pull_request_commits(
                &self,
                _target: &GithubPullRequestTarget,
            ) -> Result<Vec<CommitMessageContext>, GithubChecksError> {
                *self.commit_calls.lock().unwrap() += 1;
                self.commits.clone()
            }
        }

        #[test]
        fn test_github_pr_fetches_commits_and_cli_override_skips_api_and_fails_fast() {
            let mut env = BTreeMap::new();
            env.insert(ENV_GITHUB_TOKEN.to_string(), "tok".to_string());
            env.insert(ENV_GITHUB_REPOSITORY.to_string(), "o/r".to_string());

            let fetcher = TrackingCommitsFetcher {
                commits: Ok(vec![CommitMessageContext {
                    sha: Some("1234567".to_string()),
                    message: "feat(cli): api commit".to_string(),
                }]),
                commit_calls: Mutex::new(0),
            };

            // 1. Auto-fetch commits on --github-pr
            let args = FilingSourceArgs {
                github_pr: Some(Some(384)),
                ..FilingSourceArgs::default()
            };
            let resolved =
                resolve_filing_source_with_fetcher(&args, &mut Cursor::new(b""), &env, &fetcher)
                    .expect("auto-fetch commits");
            assert_eq!(*fetcher.commit_calls.lock().unwrap(), 1);
            assert_eq!(resolved.intake.commit_messages.as_ref().unwrap().len(), 1);

            // 2. Explicit --commit-message overrides and skips GitHub PR Commits API
            *fetcher.commit_calls.lock().unwrap() = 0;
            let override_args = FilingSourceArgs {
                github_pr: Some(Some(384)),
                commit_messages: vec!["fix(core): cli override".to_string()],
                ..FilingSourceArgs::default()
            };
            let resolved_override = resolve_filing_source_with_fetcher(
                &override_args,
                &mut Cursor::new(b""),
                &env,
                &fetcher,
            )
            .expect("override commits");
            assert_eq!(*fetcher.commit_calls.lock().unwrap(), 0);
            assert_eq!(
                resolved_override.intake.commit_messages.unwrap()[0].message,
                "fix(core): cli override"
            );

            // 3. Fail fast on GitHub Commits API error
            let failing_fetcher = TrackingCommitsFetcher {
                commits: Err(GithubChecksError::Http {
                    status: 500,
                    url: "https://api.github.com/repos/o/r/pulls/384/commits".to_string(),
                    message: "Internal error\n  Hint: Retry".to_string(),
                }),
                commit_calls: Mutex::new(0),
            };
            let err = resolve_filing_source_with_fetcher(
                &args,
                &mut Cursor::new(b""),
                &env,
                &failing_fetcher,
            )
            .expect_err("API failure must fail fast");
            assert_eq!(err.exit_code(), 2);

            // 4. Clap conflict between --commit-message and --commit-messages-file
            assert!(
                Cli::try_parse_from([
                    "canon-clerk",
                    "intake",
                    "--commit-message",
                    "feat: a",
                    "--commit-messages-file",
                    "commits.json",
                ])
                .is_err()
            );

            // 5. Single-stdin guard on --commit-messages-file -
            let conflict_stdin = FilingSourceArgs {
                diff: Some("-".to_string()),
                commit_messages_files: vec!["-".to_string()],
                ..FilingSourceArgs::default()
            };
            assert!(ensure_single_stdin_source(&conflict_stdin, None).is_err());
        }
    }
}
