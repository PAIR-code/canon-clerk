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

//! GitHub Pull Request context resolution, event payload parsing, and REST API fetching.

use super::{
    DEFAULT_GITHUB_API_URL, ENV_GH_TOKEN, ENV_GITHUB_API_URL, ENV_GITHUB_EVENT_PATH,
    ENV_GITHUB_REPOSITORY, ENV_GITHUB_TOKEN, GithubChecksError, env_non_empty,
    parse_github_repository,
};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

/// Environment variable naming the GitHub Actions trigger event (e.g., `"pull_request_target"`).
pub const ENV_GITHUB_EVENT_NAME: &str = "GITHUB_EVENT_NAME";

/// Resolved GitHub repository, pull request number, API endpoint, and bearer token.
#[derive(Clone, PartialEq, Eq)]
pub struct GithubPullRequestTarget {
    /// GitHub REST API base URL without trailing slash (e.g., `https://api.github.com`).
    pub api_base_url: String,
    /// Repository owner or organization login.
    pub owner: String,
    /// Repository name.
    pub repo: String,
    /// Positive 1-based pull request number.
    pub pull_number: u64,
    /// Bearer token read strictly from `GITHUB_TOKEN` / `GH_TOKEN`.
    pub token: String,
}

impl fmt::Debug for GithubPullRequestTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GithubPullRequestTarget")
            .field("api_base_url", &self.api_base_url)
            .field("owner", &self.owner)
            .field("repo", &self.repo)
            .field("pull_number", &self.pull_number)
            .field("token", &"<redacted>")
            .finish()
    }
}

/// Pull request metadata extracted from `$GITHUB_EVENT_PATH` or `GET /repos/{owner}/{repo}/pulls/{pull_number}`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GithubPullRequestMetadata {
    /// Pull request title, if non-empty.
    pub title: Option<String>,
    /// Pull request Markdown body, if non-empty.
    pub body: Option<String>,
    /// Pull request head commit SHA, if present.
    pub head_sha: Option<String>,
}

/// Pull request context extracted from a GitHub Actions event JSON file (`$GITHUB_EVENT_PATH`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubPullRequestEventContext {
    /// Positive pull request number (`pull_request.number`).
    pub number: u64,
    /// Metadata fields extracted from `pull_request`.
    pub metadata: GithubPullRequestMetadata,
}

/// Complete resolved pull request filing payload returned by [`resolve_github_pr_filing`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedGithubPullRequest {
    /// Resolved target coordinates and credentials.
    pub target: GithubPullRequestTarget,
    /// Resolved PR metadata (`title`, `body`, `head_sha`).
    pub metadata: GithubPullRequestMetadata,
    /// Unified diff text fetched via `Accept: application/vnd.github.v3.diff`.
    pub diff: String,
}

/// Outbound port for fetching GitHub Pull Request metadata and unified diffs.
pub trait GithubPullRequestFetcher: Send + Sync {
    /// Fetches pull request metadata (`title`, `body`, `head.sha`) via `Accept: application/vnd.github+json`.
    fn fetch_pull_request_metadata(
        &self,
        target: &GithubPullRequestTarget,
    ) -> Result<GithubPullRequestMetadata, GithubChecksError>;

    /// Fetches pull request unified diff via `Accept: application/vnd.github.v3.diff`.
    fn fetch_pull_request_diff(
        &self,
        target: &GithubPullRequestTarget,
    ) -> Result<String, GithubChecksError>;
}

/// Returns `true` when `GITHUB_EVENT_NAME` equals `"pull_request_target"`.
pub fn is_pull_request_target_event(env: &BTreeMap<String, String>) -> bool {
    env_non_empty(env, ENV_GITHUB_EVENT_NAME) == Some("pull_request_target")
}

/// Parses `pull_request.number`, `title`, `body`, and `head.sha` from a GitHub Actions event JSON string.
pub fn parse_pr_event_json(
    event_json: &str,
) -> Result<GithubPullRequestEventContext, GithubChecksError> {
    let value: serde_json::Value = serde_json::from_str(event_json).map_err(|err| {
        GithubChecksError::Config(format!(
            "Malformed GitHub event payload JSON in `{ENV_GITHUB_EVENT_PATH}`: {err}\n  \
             Hint: Ensure `{ENV_GITHUB_EVENT_PATH}` points to a valid GitHub Actions pull request event JSON file, or pass an explicit PR number via `--github-pr <NUMBER>`."
        ))
    })?;

    let pr = value.get("pull_request").ok_or_else(|| {
        GithubChecksError::Config(format!(
            "GitHub event payload in `{ENV_GITHUB_EVENT_PATH}` does not contain a `pull_request` object.\n  \
             Hint: Run bare `--github-pr` on a pull_request / pull_request_target event, or specify an explicit PR number via `--github-pr <NUMBER>`."
        ))
    })?;

    let number = pr
        .get("number")
        .and_then(|v| v.as_u64())
        .filter(|n| *n >= 1)
        .ok_or_else(|| {
            GithubChecksError::Config(format!(
                "GitHub event payload in `{ENV_GITHUB_EVENT_PATH}` is missing a valid positive integer `pull_request.number`.\n  \
                 Hint: Pass an explicit PR number via `--github-pr <NUMBER>`."
            ))
        })?;

    let title = pr
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);

    let body = pr
        .get("body")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(ToOwned::to_owned);

    let head_sha = pr
        .get("head")
        .and_then(|h| h.get("sha"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);

    Ok(GithubPullRequestEventContext {
        number,
        metadata: GithubPullRequestMetadata {
            title,
            body,
            head_sha,
        },
    })
}

/// Reads and parses pull request context from `$GITHUB_EVENT_PATH`.
pub fn parse_pr_context_from_event_path(
    env: &BTreeMap<String, String>,
) -> Result<GithubPullRequestEventContext, GithubChecksError> {
    let event_path = env_non_empty(env, ENV_GITHUB_EVENT_PATH).ok_or_else(|| {
        GithubChecksError::Config(format!(
            "Missing `{ENV_GITHUB_EVENT_PATH}` for bare `--github-pr`.\n  \
             Hint: Run inside GitHub Actions with `{ENV_GITHUB_EVENT_PATH}` set, or pass an explicit PR number via `--github-pr <NUMBER>`."
        ))
    })?;

    let contents = fs::read_to_string(Path::new(event_path)).map_err(|err| {
        GithubChecksError::Config(format!(
            "Failed to read `{ENV_GITHUB_EVENT_PATH}` (`{event_path}`): {err}\n  \
             Hint: Verify `{ENV_GITHUB_EVENT_PATH}` points to a readable event JSON file, or pass an explicit PR number via `--github-pr <NUMBER>`."
        ))
    })?;

    parse_pr_event_json(&contents)
}

/// Resolves GitHub credentials, repository, PR metadata, and unified diff for `--github-pr [<NUMBER>]`.
pub fn resolve_github_pr_filing(
    pr_number_override: Option<u64>,
    repo_flag: Option<&str>,
    env: &BTreeMap<String, String>,
    fetcher: &dyn GithubPullRequestFetcher,
) -> Result<ResolvedGithubPullRequest, GithubChecksError> {
    let token = env_non_empty(env, ENV_GITHUB_TOKEN)
        .or_else(|| env_non_empty(env, ENV_GH_TOKEN))
        .ok_or_else(|| {
            GithubChecksError::Config(
                "Missing GitHub authentication token for `--github-pr`.\n  \
                 Hint: Set `GITHUB_TOKEN` (or `GH_TOKEN`) with `pull-requests: read` permission before running `canon-clerk`."
                    .to_string(),
            )
        })?;

    let raw_repo = repo_flag
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| env_non_empty(env, ENV_GITHUB_REPOSITORY))
        .ok_or_else(|| {
            GithubChecksError::Config(
                "Missing target GitHub repository for `--github-pr`.\n  \
                 Hint: Pass `--github-repo <owner/repo>` or set `GITHUB_REPOSITORY`."
                    .to_string(),
            )
        })?;
    let (owner, repo) = parse_github_repository(raw_repo)?;

    let api_base_url = env_non_empty(env, ENV_GITHUB_API_URL)
        .unwrap_or(DEFAULT_GITHUB_API_URL)
        .trim_end_matches('/')
        .to_string();

    let (pull_number, event_metadata) = match pr_number_override {
        Some(0) => {
            return Err(GithubChecksError::Config(
                "Invalid pull request number `0` for `--github-pr`.\n  \
                 Hint: Pass a positive pull request number (`>= 1`), or omit the number to read from `GITHUB_EVENT_PATH`."
                    .to_string(),
            ));
        }
        Some(number) => (number, None),
        None => {
            let ctx = parse_pr_context_from_event_path(env)?;
            (ctx.number, Some(ctx.metadata))
        }
    };

    let target = GithubPullRequestTarget {
        api_base_url,
        owner,
        repo,
        pull_number,
        token: token.to_string(),
    };

    let metadata = match event_metadata {
        Some(meta) => meta,
        None => fetcher.fetch_pull_request_metadata(&target)?,
    };
    let diff = fetcher.fetch_pull_request_diff(&target)?;

    Ok(ResolvedGithubPullRequest {
        target,
        metadata,
        diff,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct StubPrFetcher {
        metadata: GithubPullRequestMetadata,
        diff: String,
        metadata_calls: Mutex<usize>,
        diff_calls: Mutex<usize>,
    }

    impl StubPrFetcher {
        fn new(metadata: GithubPullRequestMetadata, diff: &str) -> Self {
            Self {
                metadata,
                diff: diff.to_string(),
                metadata_calls: Mutex::new(0),
                diff_calls: Mutex::new(0),
            }
        }
    }

    impl GithubPullRequestFetcher for StubPrFetcher {
        fn fetch_pull_request_metadata(
            &self,
            _target: &GithubPullRequestTarget,
        ) -> Result<GithubPullRequestMetadata, GithubChecksError> {
            *self.metadata_calls.lock().unwrap() += 1;
            Ok(self.metadata.clone())
        }

        fn fetch_pull_request_diff(
            &self,
            _target: &GithubPullRequestTarget,
        ) -> Result<String, GithubChecksError> {
            *self.diff_calls.lock().unwrap() += 1;
            Ok(self.diff.clone())
        }
    }

    #[test]
    fn test_bare_github_pr_reads_event_payload_and_skips_metadata_api_call() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let event_path = std::env::temp_dir().join(format!("cc-pr-event-{unique}.json"));
        fs::write(
            &event_path,
            r#"{"pull_request":{"number":358,"title":"feat(intake): native PR","body":"PR body text","head":{"sha":"abc123"}}}"#,
        )
        .expect("write event file");

        let mut env = BTreeMap::new();
        env.insert(ENV_GITHUB_TOKEN.to_string(), "tok-secret".to_string());
        env.insert(
            ENV_GITHUB_REPOSITORY.to_string(),
            "PAIR-code/canon-clerk".to_string(),
        );
        env.insert(
            ENV_GITHUB_EVENT_PATH.to_string(),
            event_path.display().to_string(),
        );
        env.insert(
            ENV_GITHUB_EVENT_NAME.to_string(),
            "pull_request_target".to_string(),
        );

        let fetcher =
            StubPrFetcher::new(GithubPullRequestMetadata::default(), "diff --git a/a b/a\n");
        let resolved = resolve_github_pr_filing(None, None, &env, &fetcher).expect("resolve PR");
        let _ = fs::remove_file(&event_path);

        assert_eq!(resolved.target.pull_number, 358);
        assert_eq!(resolved.target.owner, "PAIR-code");
        assert_eq!(resolved.target.repo, "canon-clerk");
        assert_eq!(
            resolved.metadata.title.as_deref(),
            Some("feat(intake): native PR")
        );
        assert_eq!(resolved.metadata.body.as_deref(), Some("PR body text"));
        assert_eq!(resolved.metadata.head_sha.as_deref(), Some("abc123"));
        assert_eq!(resolved.diff, "diff --git a/a b/a\n");
        assert_eq!(*fetcher.metadata_calls.lock().unwrap(), 0);
        assert_eq!(*fetcher.diff_calls.lock().unwrap(), 1);
        assert!(is_pull_request_target_event(&env));
    }

    #[test]
    fn test_numbered_github_pr_fetches_metadata_and_diff() {
        let mut env = BTreeMap::new();
        env.insert(ENV_GH_TOKEN.to_string(), "gh-tok".to_string());

        let fetcher = StubPrFetcher::new(
            GithubPullRequestMetadata {
                title: Some("API Title".to_string()),
                body: Some("API Body".to_string()),
                head_sha: Some("api-sha-999".to_string()),
            },
            "diff --git a/x b/x\n",
        );

        let resolved =
            resolve_github_pr_filing(Some(42), Some("octo/repo"), &env, &fetcher).expect("resolve");
        assert_eq!(resolved.target.pull_number, 42);
        assert_eq!(resolved.target.owner, "octo");
        assert_eq!(resolved.target.repo, "repo");
        assert_eq!(resolved.metadata.title.as_deref(), Some("API Title"));
        assert_eq!(resolved.metadata.head_sha.as_deref(), Some("api-sha-999"));
        assert_eq!(*fetcher.metadata_calls.lock().unwrap(), 1);
        assert_eq!(*fetcher.diff_calls.lock().unwrap(), 1);
        assert!(!format!("{:?}", resolved.target).contains("gh-tok"));
    }

    #[test]
    fn test_fail_fast_errors_include_actionable_hints() {
        let empty = BTreeMap::new();
        let fetcher = StubPrFetcher::new(GithubPullRequestMetadata::default(), "");

        let err_token =
            resolve_github_pr_filing(Some(1), Some("o/r"), &empty, &fetcher).unwrap_err();
        assert!(err_token.to_string().contains("Hint:"));

        let mut tok_only = BTreeMap::new();
        tok_only.insert(ENV_GITHUB_TOKEN.to_string(), "tok".to_string());
        let err_repo = resolve_github_pr_filing(Some(1), None, &tok_only, &fetcher).unwrap_err();
        assert!(err_repo.to_string().contains("Hint:"));

        let err_zero =
            resolve_github_pr_filing(Some(0), Some("o/r"), &tok_only, &fetcher).unwrap_err();
        assert!(err_zero.to_string().contains("Hint:"));

        let err_event =
            resolve_github_pr_filing(None, Some("o/r"), &tok_only, &fetcher).unwrap_err();
        assert!(err_event.to_string().contains("Hint:"));
    }
}
