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

//! Stage-agnostic GitHub REST API outbound adapter (Pull Request ingestion and Checks publication).

mod client;
mod linked_issues;
mod pull_request;

use crate::caseload::{AnnotationLevel, CodeAnnotation};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

pub use client::GithubChecksClient;
pub use linked_issues::fetch_linked_issues_for_pr_body;
pub use pull_request::{
    ENV_GITHUB_ACTOR, ENV_GITHUB_EVENT_NAME, GithubBypassMatch, GithubPullRequestEventContext,
    GithubPullRequestFetcher, GithubPullRequestMetadata, GithubPullRequestTarget,
    ResolvedGithubPullRequest, detect_github_bypass_label, is_pull_request_target_event,
    parse_pr_context_from_event_path, parse_pr_event_json, resolve_github_pr_filing,
};

/// Default GitHub REST API base URL (overridable via `GITHUB_API_URL` for GHES or mock servers).
pub const DEFAULT_GITHUB_API_URL: &str = "https://api.github.com";

/// Default GitHub Check Run name published by `canon-clerk audit`.
pub const DEFAULT_GITHUB_CHECK_NAME: &str = "Canon Clerk Audit";

/// Default GitHub pull request label name used when `--github-bypass-label` is passed without a value.
pub const DEFAULT_GITHUB_BYPASS_LABEL: &str = "skip-audit";

/// Environment variable supplying a fallback GitHub PR bypass label name when `--github-bypass-label` is omitted.
pub const ENV_CANON_CLERK_GITHUB_BYPASS_LABEL: &str = "CANON_CLERK_GITHUB_BYPASS_LABEL";

/// Environment variable overriding the GitHub REST API base URL.
pub const ENV_GITHUB_API_URL: &str = "GITHUB_API_URL";

/// Primary environment variable supplying the GitHub API authentication token.
pub const ENV_GITHUB_TOKEN: &str = "GITHUB_TOKEN";

/// Fallback environment variable supplying the GitHub API authentication token.
pub const ENV_GH_TOKEN: &str = "GH_TOKEN";

/// Environment variable supplying the default `<owner>/<repo>` slug.
pub const ENV_GITHUB_REPOSITORY: &str = "GITHUB_REPOSITORY";

/// Environment variable supplying the fallback commit SHA.
pub const ENV_GITHUB_SHA: &str = "GITHUB_SHA";

/// Environment variable pointing to the GitHub Actions event payload JSON file.
pub const ENV_GITHUB_EVENT_PATH: &str = "GITHUB_EVENT_PATH";

/// Maximum number of annotations accepted by GitHub in a single Check Run `POST` or `PATCH` call.
pub const GITHUB_CHECKS_MAX_ANNOTATIONS_PER_REQUEST: usize = 50;

/// Maximum character length accepted by GitHub for Check Run `output.summary` and `output.text`.
pub const GITHUB_CHECKS_MAX_OUTPUT_CHARS: usize = 65_535;

/// Default wire timeout in milliseconds for each GitHub Checks API HTTP request.
pub const DEFAULT_GITHUB_API_TIMEOUT_MS: u64 = 30_000;

const TRUNCATION_NOTICE: &str =
    "\n\n...(truncated to fit GitHub's 65,535-character Check Run output limit)";

/// Terminal conclusion recorded on a published GitHub Check Run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckRunConclusion {
    /// All evaluated canons passed (or zero trials were required).
    Success,
    /// One or more canon violations or static validation errors occurred.
    Failure,
    /// Neutral advisory result.
    Neutral,
    /// Evaluation was skipped (e.g., unavailable provider credentials).
    Skipped,
}

impl CheckRunConclusion {
    /// Returns the canonical GitHub Checks API wire string (`"success"`, `"failure"`, `"neutral"`, `"skipped"`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Neutral => "neutral",
            Self::Skipped => "skipped",
        }
    }
}

/// Stage-agnostic Check Run payload consumed by [`CheckRunPublisher`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckRunReport {
    /// Check Run name displayed in the GitHub Checks UI.
    pub name: String,
    /// Terminal Check Run conclusion.
    pub conclusion: CheckRunConclusion,
    /// Short headline for the Check Run output block.
    pub title: String,
    /// Markdown summary rendered at the top of the Check Run (e.g., Funnel Provenance and decree table).
    pub summary: String,
    /// Optional Markdown detail body rendered below the summary (e.g., complete findings list).
    pub text: Option<String>,
    /// Complete list of inline source annotations (batched in chunks of 50 by the publisher).
    pub annotations: Vec<CodeAnnotation>,
}

/// Resolved GitHub repository, commit SHA, API endpoint, and authentication token for Check Run publication.
#[derive(Clone, PartialEq, Eq)]
pub struct GithubCheckTarget {
    /// GitHub REST API base URL without trailing slash (e.g., `https://api.github.com`).
    pub api_base_url: String,
    /// Repository owner or organization login.
    pub owner: String,
    /// Repository name.
    pub repo: String,
    /// Commit SHA to attach the Check Run to.
    pub head_sha: String,
    /// Bearer token read strictly from `GITHUB_TOKEN` / `GH_TOKEN`.
    pub token: String,
}

impl fmt::Debug for GithubCheckTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GithubCheckTarget")
            .field("api_base_url", &self.api_base_url)
            .field("owner", &self.owner)
            .field("repo", &self.repo)
            .field("head_sha", &self.head_sha)
            .field("token", &"<redacted>")
            .finish()
    }
}

/// Metadata returned after successfully publishing a GitHub Check Run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedCheckRun {
    /// Numeric Check Run identifier assigned by GitHub.
    pub id: u64,
    /// Optional web URL for the created Check Run.
    pub html_url: Option<String>,
    /// Total number of inline annotations published across all `POST` and `PATCH` batches.
    pub annotations_published: usize,
    /// Total number of HTTP requests sent (`1` for `POST`, plus `PATCH` batches).
    pub requests_sent: usize,
}

/// Errors returned while resolving GitHub Checks configuration or publishing a Check Run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubChecksError {
    /// Missing or invalid GitHub Check target configuration (token, repository, or head SHA).
    Config(String),
    /// Non-2xx HTTP status returned by the GitHub Checks API.
    Http {
        /// HTTP status code returned by GitHub.
        status: u16,
        /// Target endpoint URL.
        url: String,
        /// Diagnostic message extracted from the response body.
        message: String,
    },
    /// Network, timeout, or runtime transport failure communicating with GitHub.
    Transport {
        /// Target endpoint URL.
        url: String,
        /// Diagnostic transport error message.
        message: String,
    },
    /// Unexpected or malformed response payload from the GitHub Checks API.
    Protocol(String),
}

impl fmt::Display for GithubChecksError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(msg) => write!(f, "{msg}"),
            Self::Http {
                status,
                url,
                message,
            } => write!(
                f,
                "GitHub Checks API returned HTTP {status} for {url}: {message}"
            ),
            Self::Transport { url, message } => {
                write!(f, "GitHub Checks API request to {url} failed: {message}")
            }
            Self::Protocol(msg) => write!(f, "GitHub Checks API protocol error: {msg}"),
        }
    }
}

impl std::error::Error for GithubChecksError {}

/// Outbound port for publishing a stage-agnostic [`CheckRunReport`] to GitHub Checks.
pub trait CheckRunPublisher: Send + Sync {
    /// Publishes `report` to `target`, creating the Check Run and streaming any remaining annotations in batches.
    fn publish_check_run(
        &self,
        target: &GithubCheckTarget,
        report: &CheckRunReport,
    ) -> Result<PublishedCheckRun, GithubChecksError>;
}

/// Sanitized GitHub Checks API annotation wire payload.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub(crate) struct GithubCheckAnnotation {
    /// Workspace-relative POSIX file path.
    pub path: String,
    /// 1-based starting line number (`>= 1`).
    pub start_line: usize,
    /// 1-based ending line number (`>= start_line`).
    pub end_line: usize,
    /// Optional 1-based starting column (omitted whenever `start_line != end_line` per GitHub API rules).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_column: Option<usize>,
    /// Optional 1-based ending column (omitted whenever `start_line != end_line` per GitHub API rules).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<usize>,
    /// Wire annotation level (`"failure"`, `"warning"`, or `"notice"`).
    pub annotation_level: &'static str,
    /// Diagnostic explanation displayed inline on the PR diff.
    pub message: String,
    /// Optional short diagnostic title (e.g., canon ID or path).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Converts a domain [`CodeAnnotation`] into a GitHub Checks API wire annotation,
/// normalizing 1-based line bounds and omitting columns on multi-line spans to prevent HTTP 422.
pub(crate) fn sanitize_annotation_for_github(ann: &CodeAnnotation) -> GithubCheckAnnotation {
    let start_line = ann.start_line.max(1);
    let end_line = ann.end_line.max(start_line);

    let (start_column, end_column) = if start_line == end_line {
        let sc = ann.start_column.filter(|c| *c >= 1);
        let ec = match (sc, ann.end_column.filter(|c| *c >= 1)) {
            (Some(s), Some(e)) => Some(e.max(s)),
            (None, Some(e)) => Some(e),
            (_, None) => None,
        };
        (sc, ec)
    } else {
        (None, None)
    };

    let annotation_level = match ann.annotation_level {
        AnnotationLevel::Failure => "failure",
        AnnotationLevel::Warning => "warning",
        AnnotationLevel::Notice => "notice",
    };

    GithubCheckAnnotation {
        path: ann.path.clone(),
        start_line,
        end_line,
        start_column,
        end_column,
        annotation_level,
        message: ann.message.clone(),
        title: ann.title.clone().filter(|t| !t.trim().is_empty()),
    }
}

/// Truncates `text` to at most `max_chars` Unicode scalar values, appending an explicit
/// truncation notice when truncation occurs.
pub(crate) fn truncate_github_output(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let notice_chars = TRUNCATION_NOTICE.chars().count();
    if max_chars <= notice_chars {
        return text.chars().take(max_chars).collect();
    }

    let keep = max_chars - notice_chars;
    let prefix: String = text.chars().take(keep).collect();
    format!("{prefix}{TRUNCATION_NOTICE}")
}

/// Parses and validates a GitHub `<owner>/<repo>` repository slug.
pub(crate) fn parse_github_repository(raw: &str) -> Result<(String, String), GithubChecksError> {
    let trimmed = raw.trim();
    let mut parts = trimmed.split('/');
    let owner = parts.next().unwrap_or("");
    let repo = parts.next().unwrap_or("");
    let extra = parts.next();

    if owner.is_empty()
        || repo.is_empty()
        || extra.is_some()
        || owner.chars().any(char::is_whitespace)
        || repo.chars().any(char::is_whitespace)
        || owner == "."
        || owner == ".."
        || repo == "."
        || repo == ".."
    {
        return Err(GithubChecksError::Config(format!(
            "Invalid GitHub repository `{trimmed}`: expected `<owner>/<repo>`.\n  \
             Hint: Pass `--github-repo <owner/repo>` or set `GITHUB_REPOSITORY`."
        )));
    }

    Ok((owner.to_string(), repo.to_string()))
}

/// Extracts `.pull_request.head.sha` from a GitHub Actions event JSON document, if present and non-empty.
fn extract_pr_head_sha_from_event_json(event_json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(event_json).ok()?;
    value
        .get("pull_request")?
        .get("head")?
        .get("sha")?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

/// Resolves and validates the [`GithubCheckTarget`] from explicit CLI overrides and environment variables
/// before any model spend occurs.
pub fn resolve_github_check_target(
    repo_flag: Option<&str>,
    head_sha_flag: Option<&str>,
    env: &BTreeMap<String, String>,
) -> Result<GithubCheckTarget, GithubChecksError> {
    resolve_github_check_target_with_fallback_sha(repo_flag, head_sha_flag, None, env)
}

/// Resolves and validates the [`GithubCheckTarget`], optionally falling back to a PR API `head.sha`
/// (between `GITHUB_EVENT_PATH` and `GITHUB_SHA`) when `--github-pr` was used.
pub fn resolve_github_check_target_with_fallback_sha(
    repo_flag: Option<&str>,
    head_sha_flag: Option<&str>,
    fallback_pr_head_sha: Option<&str>,
    env: &BTreeMap<String, String>,
) -> Result<GithubCheckTarget, GithubChecksError> {
    let token = env_non_empty(env, ENV_GITHUB_TOKEN)
        .or_else(|| env_non_empty(env, ENV_GH_TOKEN))
        .ok_or_else(|| {
            GithubChecksError::Config(
                "Missing GitHub authentication token for `--publish-github-check`.\n  \
                 Hint: Set `GITHUB_TOKEN` (or `GH_TOKEN`) with `checks: write` permission before running `canon-clerk`."
                    .to_string(),
            )
        })?;

    let raw_repo = repo_flag
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| env_non_empty(env, ENV_GITHUB_REPOSITORY))
        .ok_or_else(|| {
            GithubChecksError::Config(
                "Missing target GitHub repository for `--publish-github-check`.\n  \
                 Hint: Pass `--github-repo <owner/repo>` or set `GITHUB_REPOSITORY`."
                    .to_string(),
            )
        })?;
    let (owner, repo) = parse_github_repository(raw_repo)?;

    let head_sha = resolve_head_sha(head_sha_flag, fallback_pr_head_sha, env)?;

    let api_base_url = env_non_empty(env, ENV_GITHUB_API_URL)
        .unwrap_or(DEFAULT_GITHUB_API_URL)
        .trim_end_matches('/')
        .to_string();

    Ok(GithubCheckTarget {
        api_base_url,
        owner,
        repo,
        head_sha,
        token: token.to_string(),
    })
}

fn resolve_head_sha(
    head_sha_flag: Option<&str>,
    fallback_pr_head_sha: Option<&str>,
    env: &BTreeMap<String, String>,
) -> Result<String, GithubChecksError> {
    if let Some(sha) = head_sha_flag.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(sha.to_string());
    }

    if let Some(event_path) = env_non_empty(env, ENV_GITHUB_EVENT_PATH)
        && let Ok(contents) = fs::read_to_string(Path::new(event_path))
        && let Some(pr_sha) = extract_pr_head_sha_from_event_json(&contents)
    {
        return Ok(pr_sha);
    }

    if let Some(pr_sha) = fallback_pr_head_sha
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok(pr_sha.to_string());
    }

    if let Some(sha) = env_non_empty(env, ENV_GITHUB_SHA) {
        return Ok(sha.to_string());
    }

    Err(GithubChecksError::Config(
        "Missing target commit SHA for `--publish-github-check`.\n  \
         Hint: Pass `--github-head-sha <sha>`, ensure `GITHUB_EVENT_PATH` contains `pull_request.head.sha`, or set `GITHUB_SHA`."
            .to_string(),
    ))
}

pub(crate) fn env_non_empty<'a>(env: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    env.get(key).map(|v| v.trim()).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    mod annotation_sanitization_and_truncation {
        use super::*;

        #[test]
        fn test_single_line_preserves_columns_and_multiline_omits_columns() {
            let single = CodeAnnotation {
                path: "src/lib.rs".to_string(),
                start_line: 12,
                end_line: 12,
                start_column: Some(4),
                end_column: Some(18),
                annotation_level: AnnotationLevel::Failure,
                message: "Single-line issue".to_string(),
                title: Some("canon-a".to_string()),
            };
            let san_single = sanitize_annotation_for_github(&single);
            assert_eq!(san_single.start_line, 12);
            assert_eq!(san_single.end_line, 12);
            assert_eq!(san_single.start_column, Some(4));
            assert_eq!(san_single.end_column, Some(18));
            assert_eq!(san_single.annotation_level, "failure");

            let multi = CodeAnnotation {
                path: "src/lib.rs".to_string(),
                start_line: 12,
                end_line: 20,
                start_column: Some(4),
                end_column: Some(18),
                annotation_level: AnnotationLevel::Warning,
                message: "Multi-line issue".to_string(),
                title: Some("   ".to_string()),
            };
            let san_multi = sanitize_annotation_for_github(&multi);
            assert_eq!(san_multi.start_line, 12);
            assert_eq!(san_multi.end_line, 20);
            assert_eq!(san_multi.start_column, None);
            assert_eq!(san_multi.end_column, None);
            assert_eq!(san_multi.annotation_level, "warning");
            assert_eq!(san_multi.title, None);
        }

        #[test]
        fn test_clamps_zero_lines_and_inverted_bounds() {
            let weird = CodeAnnotation {
                path: "src/main.rs".to_string(),
                start_line: 0,
                end_line: 0,
                start_column: Some(0),
                end_column: Some(5),
                annotation_level: AnnotationLevel::Notice,
                message: "File-level notice".to_string(),
                title: None,
            };
            let san = sanitize_annotation_for_github(&weird);
            assert_eq!(san.start_line, 1);
            assert_eq!(san.end_line, 1);
            assert_eq!(san.start_column, None);
            assert_eq!(san.end_column, Some(5));
            assert_eq!(san.annotation_level, "notice");
        }

        #[test]
        fn test_truncate_github_output_respects_char_limit_and_appends_notice() {
            let exact = "a".repeat(GITHUB_CHECKS_MAX_OUTPUT_CHARS);
            assert_eq!(
                truncate_github_output(&exact, GITHUB_CHECKS_MAX_OUTPUT_CHARS),
                exact
            );

            let oversized = "é".repeat(GITHUB_CHECKS_MAX_OUTPUT_CHARS + 250);
            let truncated = truncate_github_output(&oversized, GITHUB_CHECKS_MAX_OUTPUT_CHARS);
            assert!(truncated.chars().count() <= GITHUB_CHECKS_MAX_OUTPUT_CHARS);
            assert!(truncated.contains("truncated to fit GitHub's 65,535-character"));
        }
    }

    mod target_resolution {
        use super::*;

        #[test]
        fn test_resolves_pr_head_sha_from_event_path_before_github_sha() {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let event_file = std::env::temp_dir().join(format!("cc-gh-event-{unique}.json"));
            fs::write(
                &event_file,
                r#"{"pull_request":{"head":{"sha":"pr-head-sha-123"}}}"#,
            )
            .expect("write event file");

            let mut env = BTreeMap::new();
            env.insert(ENV_GH_TOKEN.to_string(), "ghp_fallback_token".to_string());
            env.insert(
                ENV_GITHUB_REPOSITORY.to_string(),
                "PAIR-code/canon-clerk".to_string(),
            );
            env.insert(ENV_GITHUB_SHA.to_string(), "base-sha-999".to_string());
            env.insert(
                ENV_GITHUB_EVENT_PATH.to_string(),
                event_file.display().to_string(),
            );
            env.insert(
                ENV_GITHUB_API_URL.to_string(),
                "https://github.example.com/api/v3/".to_string(),
            );

            let target = resolve_github_check_target(None, None, &env).expect("resolve target");
            assert_eq!(target.owner, "PAIR-code");
            assert_eq!(target.repo, "canon-clerk");
            assert_eq!(target.head_sha, "pr-head-sha-123");
            assert_eq!(target.token, "ghp_fallback_token");
            assert_eq!(target.api_base_url, "https://github.example.com/api/v3");
            assert!(!format!("{target:?}").contains("ghp_fallback_token"));

            let _ = fs::remove_file(&event_file);
        }

        #[test]
        fn test_explicit_flags_override_env_and_missing_inputs_return_hints() {
            let mut env = BTreeMap::new();
            env.insert(ENV_GITHUB_TOKEN.to_string(), "tok_primary".to_string());
            env.insert(ENV_GH_TOKEN.to_string(), "tok_secondary".to_string());
            env.insert(
                ENV_GITHUB_REPOSITORY.to_string(),
                "org/env-repo".to_string(),
            );
            env.insert(ENV_GITHUB_SHA.to_string(), "env-sha".to_string());

            let target =
                resolve_github_check_target(Some("custom-org/custom-repo"), Some("flag-sha"), &env)
                    .expect("resolve with flag overrides");
            assert_eq!(target.owner, "custom-org");
            assert_eq!(target.repo, "custom-repo");
            assert_eq!(target.head_sha, "flag-sha");
            assert_eq!(target.token, "tok_primary");
            assert_eq!(target.api_base_url, DEFAULT_GITHUB_API_URL);

            let empty = BTreeMap::new();
            let err_token = resolve_github_check_target(None, None, &empty)
                .expect_err("missing token must fail");
            assert!(err_token.to_string().contains("Hint:"));

            let mut only_token = BTreeMap::new();
            only_token.insert(ENV_GITHUB_TOKEN.to_string(), "tok".to_string());
            let err_repo = resolve_github_check_target(None, None, &only_token)
                .expect_err("missing repo must fail");
            assert!(err_repo.to_string().contains("Hint:"));

            let err_bad_repo = resolve_github_check_target(Some("not-a-slug"), None, &only_token)
                .expect_err("malformed repo must fail");
            assert!(err_bad_repo.to_string().contains("Hint:"));

            let err_sha = resolve_github_check_target(Some("org/repo"), None, &only_token)
                .expect_err("missing sha must fail");
            assert!(err_sha.to_string().contains("Hint:"));
        }
    }
}
