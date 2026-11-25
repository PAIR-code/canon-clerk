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

//! GitHub Issue inquiry resolution (`--github-issue`), `$GITHUB_EVENT_PATH` parsing,
//! and deterministic `.intake.intent` formatting (`docs/architecture/github.md` §7).

use super::client::{GITHUB_API_ACCEPT, extract_github_error_message};
use super::{
    DEFAULT_GITHUB_API_URL, ENV_GH_TOKEN, ENV_GITHUB_API_URL, ENV_GITHUB_EVENT_PATH,
    ENV_GITHUB_REPOSITORY, ENV_GITHUB_TOKEN, GithubChecksClient, GithubChecksError, env_non_empty,
    parse_github_repository,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::num::NonZeroU64;
use std::path::Path;

/// Resolved GitHub repository, issue number, API endpoint, and bearer token for numbered `--github-issue <NUMBER>`.
#[derive(Clone, PartialEq, Eq)]
pub struct GithubIssueTarget {
    /// GitHub REST API base URL without trailing slash (e.g., `https://api.github.com`).
    pub api_base_url: String,
    /// Repository owner or organization login.
    pub owner: String,
    /// Repository name.
    pub repo: String,
    /// Positive 1-based issue number.
    pub issue_number: NonZeroU64,
    /// Bearer token read strictly from `GITHUB_TOKEN` / `GH_TOKEN`.
    pub token: String,
}

impl fmt::Debug for GithubIssueTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GithubIssueTarget")
            .field("api_base_url", &self.api_base_url)
            .field("owner", &self.owner)
            .field("repo", &self.repo)
            .field("issue_number", &self.issue_number)
            .field("token", &"<redacted>")
            .finish()
    }
}

/// Resolved GitHub Issue metadata used to populate prospective `.intake.intent` (`SPEC.md` §4.2).
///
/// Excludes issue comments and reaction trails to preserve Canon Clerk's noise and trust boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubIssueInquiry {
    /// Positive 1-based GitHub issue number.
    pub number: u64,
    /// Trimmed issue title, if non-empty.
    pub title: Option<String>,
    /// Issue author login (`user.login`), if non-empty.
    pub author: Option<String>,
    /// Trimmed non-empty label names (`labels[].name`).
    pub labels: Vec<String>,
    /// Issue Markdown body, if non-empty.
    pub body: Option<String>,
    /// Canonical issue HTML URL (`html_url`), if non-empty.
    pub url: Option<String>,
}

impl GithubIssueInquiry {
    /// Formats this issue deterministically into prospective `.intake.intent` text:
    /// `#<number>: <title>`, optional `Labels: <l1>, <l2>`, and optional Markdown body.
    pub fn format_as_intent(&self) -> String {
        let mut out = match self.title.as_deref() {
            Some(title) => format!("#{}: {title}", self.number),
            None => format!("#{}", self.number),
        };
        if !self.labels.is_empty() {
            out.push_str("\nLabels: ");
            out.push_str(&self.labels.join(", "));
        }
        if let Some(body) = self.body.as_deref() {
            out.push_str("\n\n");
            out.push_str(body.trim());
        }
        out
    }
}

/// Outbound port for fail-fast GitHub Issue fetching (`GET /repos/{owner}/{repo}/issues/{number}`).
pub trait GithubIssueFetcher: Send + Sync {
    /// Fetches a single GitHub Issue strictly, failing fast on HTTP 404/403, non-2xx statuses,
    /// transport/protocol failures, or Pull Request payloads (`"pull_request"` key present).
    fn fetch_issue_strict(
        &self,
        target: &GithubIssueTarget,
    ) -> Result<GithubIssueInquiry, GithubChecksError>;
}

impl GithubIssueFetcher for GithubChecksClient {
    fn fetch_issue_strict(
        &self,
        target: &GithubIssueTarget,
    ) -> Result<GithubIssueInquiry, GithubChecksError> {
        let url = format!(
            "{}/repos/{}/{}/issues/{}",
            target.api_base_url, target.owner, target.repo, target.issue_number
        );
        let (status, _, body) = self.send_get_with_link(&url, &target.token, GITHUB_API_ACCEPT)?;
        if !(200..300).contains(&status) {
            return Err(GithubChecksError::Http {
                status,
                url,
                message: extract_github_error_message(&body),
            });
        }
        parse_github_issue_inquiry_api_json(&body, target.issue_number.get())
    }
}

#[derive(Deserialize)]
struct GithubIssuePayload {
    #[serde(default)]
    number: Option<u64>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    user: Option<GithubIssueUserRef>,
    #[serde(default)]
    labels: Vec<GithubIssueLabelRef>,
    #[serde(default)]
    pull_request: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct GithubIssueUserRef {
    #[serde(default)]
    login: Option<String>,
}

#[derive(Deserialize)]
struct GithubIssueLabelRef {
    #[serde(default)]
    name: Option<String>,
}

fn project_issue_payload(
    parsed: GithubIssuePayload,
    fallback_number: Option<u64>,
    source_context: &str,
) -> Result<GithubIssueInquiry, GithubChecksError> {
    let number = parsed.number.filter(|n| *n >= 1).ok_or_else(|| {
        GithubChecksError::Config(format!(
            "GitHub issue payload from {source_context} is missing a valid positive integer `number`.\n  \
             Hint: Pass an explicit issue number via `--github-issue <NUMBER>` or verify the event payload."
        ))
    })?;

    if parsed.pull_request.as_ref().is_some_and(|v| !v.is_null()) {
        let display_num = fallback_number.unwrap_or(number);
        return Err(GithubChecksError::Config(format!(
            "GitHub item #{display_num} is a Pull Request, not an Issue (`--github-issue` rejects pull requests).\n  \
             Hint: Pass `--github-pr {display_num}` to ingest a pull request, or specify an issue number for `--github-issue <NUMBER>`."
        )));
    }

    let title = parsed
        .title
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let author = parsed
        .user
        .and_then(|u| u.login)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let labels = parsed
        .labels
        .into_iter()
        .filter_map(|label| {
            label
                .name
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .collect();
    let body = parsed.body.filter(|s| !s.trim().is_empty());
    let url = parsed
        .html_url
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    Ok(GithubIssueInquiry {
        number,
        title,
        author,
        labels,
        body,
        url,
    })
}

/// Decodes a GitHub REST `GET /repos/{owner}/{repo}/issues/{number}` JSON payload strictly,
/// rejecting pull requests (`"pull_request"` key present) and malformed payloads.
pub(super) fn parse_github_issue_inquiry_api_json(
    response_body: &str,
    requested_number: u64,
) -> Result<GithubIssueInquiry, GithubChecksError> {
    let parsed: GithubIssuePayload = serde_json::from_str(response_body).map_err(|err| {
        GithubChecksError::Protocol(format!(
            "failed to decode GitHub Issue #{requested_number} JSON response: {err}"
        ))
    })?;
    project_issue_payload(
        parsed,
        Some(requested_number),
        &format!("GitHub Issues API (#{requested_number})"),
    )
}

/// Parses `issue.number`, `title`, `body`, `labels[].name`, `user.login`, and `html_url`
/// from a GitHub Actions `on: issues` event JSON string (`$GITHUB_EVENT_PATH`).
fn parse_issue_event_json(event_json: &str) -> Result<GithubIssueInquiry, GithubChecksError> {
    let value: serde_json::Value = serde_json::from_str(event_json).map_err(|err| {
        GithubChecksError::Config(format!(
            "Malformed GitHub event payload JSON in `{ENV_GITHUB_EVENT_PATH}`: {err}\n  \
             Hint: Ensure `{ENV_GITHUB_EVENT_PATH}` points to a valid GitHub Actions `issues` event JSON file, or pass an explicit issue number via `--github-issue <NUMBER>`."
        ))
    })?;

    let issue_val = value
        .get("issue")
        .filter(|v| v.is_object())
        .ok_or_else(|| {
            GithubChecksError::Config(format!(
                "GitHub event payload in `{ENV_GITHUB_EVENT_PATH}` does not contain an `issue` object.\n  \
                 Hint: Run bare `--github-issue` on an `issues` workflow event, or specify an explicit issue number via `--github-issue <NUMBER>`."
            ))
        })?;

    let parsed: GithubIssuePayload = serde_json::from_value(issue_val.clone()).map_err(|err| {
        GithubChecksError::Config(format!(
            "Malformed `issue` object in `{ENV_GITHUB_EVENT_PATH}`: {err}\n  \
             Hint: Verify `{ENV_GITHUB_EVENT_PATH}` contains a valid GitHub Actions `issues` event payload, or pass `--github-issue <NUMBER>`."
        ))
    })?;

    project_issue_payload(parsed, None, &format!("`{ENV_GITHUB_EVENT_PATH}`"))
}

/// Reads and parses GitHub Issue context from `$GITHUB_EVENT_PATH` with zero API calls.
fn parse_issue_context_from_event_path(
    env: &BTreeMap<String, String>,
) -> Result<GithubIssueInquiry, GithubChecksError> {
    let event_path = env_non_empty(env, ENV_GITHUB_EVENT_PATH).ok_or_else(|| {
        GithubChecksError::Config(format!(
            "Missing `{ENV_GITHUB_EVENT_PATH}` for bare `--github-issue`.\n  \
             Hint: Run inside GitHub Actions on an `issues` event with `{ENV_GITHUB_EVENT_PATH}` set, or pass an explicit issue number via `--github-issue <NUMBER>`."
        ))
    })?;

    let contents = fs::read_to_string(Path::new(event_path)).map_err(|err| {
        GithubChecksError::Config(format!(
            "Failed to read `{ENV_GITHUB_EVENT_PATH}` (`{event_path}`): {err}\n  \
             Hint: Verify `{ENV_GITHUB_EVENT_PATH}` points to a readable event JSON file, or pass an explicit issue number via `--github-issue <NUMBER>`."
        ))
    })?;

    parse_issue_event_json(&contents)
}

/// Resolves a GitHub Issue inquiry for `--github-issue [<NUMBER>]`.
///
/// - When `issue_number_override` is `None` (bare `--github-issue`), reads the issue payload
///   strictly from `$GITHUB_EVENT_PATH` with zero GitHub API calls (`repo_flag` and tokens are not required).
/// - When `issue_number_override` is `Some(number)`, resolves `repo_flag` (falling back to
///   `GITHUB_REPOSITORY` in `env` when `repo_flag` is `None`), resolves `GITHUB_TOKEN` / `GH_TOKEN`
///   and `GITHUB_API_URL`, and fetches the issue via `fetcher`.
///
/// # Errors
/// Returns [`GithubChecksError::Config`] if `$GITHUB_EVENT_PATH` is missing, unreadable, malformed,
/// or lacks a positive `issue.number` in bare mode, if `GITHUB_TOKEN`/`GH_TOKEN` or the target
/// `<owner>/<repo>` repository is missing/invalid in numbered mode, or if the payload represents
/// a Pull Request (`"pull_request"` key present). Returns [`GithubChecksError::Http`],
/// [`GithubChecksError::Transport`], or [`GithubChecksError::Protocol`] if the GitHub Issues API request fails.
pub fn resolve_github_issue_inquiry(
    issue_number_override: Option<NonZeroU64>,
    repo_flag: Option<&str>,
    env: &BTreeMap<String, String>,
    fetcher: &dyn GithubIssueFetcher,
) -> Result<GithubIssueInquiry, GithubChecksError> {
    let Some(issue_number) = issue_number_override else {
        return parse_issue_context_from_event_path(env);
    };

    let token = env_non_empty(env, ENV_GITHUB_TOKEN)
        .or_else(|| env_non_empty(env, ENV_GH_TOKEN))
        .ok_or_else(|| {
            GithubChecksError::Config(
                "Missing GitHub authentication token for `--github-issue <NUMBER>`.\n  \
                 Hint: Set `GITHUB_TOKEN` or `GH_TOKEN` (for example, `GH_TOKEN=$(gh auth token)`) with `issues: read` permission before running `canon-clerk`."
                    .to_string(),
            )
        })?;

    let raw_repo = repo_flag
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| env_non_empty(env, ENV_GITHUB_REPOSITORY))
        .ok_or_else(|| {
            GithubChecksError::Config(
                "Missing target GitHub repository for `--github-issue <NUMBER>`.\n  \
                 Hint: Pass `--github-repo <owner/repo>` or set `GITHUB_REPOSITORY`."
                    .to_string(),
            )
        })?;
    let (owner, repo) = parse_github_repository(raw_repo)?;

    let api_base_url = env_non_empty(env, ENV_GITHUB_API_URL)
        .unwrap_or(DEFAULT_GITHUB_API_URL)
        .trim_end_matches('/')
        .to_string();

    let target = GithubIssueTarget {
        api_base_url,
        owner,
        repo,
        issue_number,
        token: token.to_string(),
    };

    fetcher.fetch_issue_strict(&target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct StubIssueFetcher {
        result: Result<GithubIssueInquiry, GithubChecksError>,
    }

    impl GithubIssueFetcher for StubIssueFetcher {
        fn fetch_issue_strict(
            &self,
            _target: &GithubIssueTarget,
        ) -> Result<GithubIssueInquiry, GithubChecksError> {
            self.result.clone()
        }
    }

    #[test]
    fn test_format_as_intent_includes_title_optional_labels_and_body() {
        let full = GithubIssueInquiry {
            number: 397,
            title: Some("feat(intake, apprise): ingest GitHub Issues".to_string()),
            author: Some("jimbojw".to_string()),
            labels: vec!["type: feat".to_string(), "scope: cli".to_string()],
            body: Some("## Problem\n\nAdd `--github-issue`.\n".to_string()),
            url: Some("https://github.com/PAIR-code/canon-clerk/issues/397".to_string()),
        };
        assert_eq!(
            full.format_as_intent(),
            "#397: feat(intake, apprise): ingest GitHub Issues\n\
             Labels: type: feat, scope: cli\n\n\
             ## Problem\n\nAdd `--github-issue`."
        );

        let minimal = GithubIssueInquiry {
            number: 42,
            title: Some("Short title".to_string()),
            author: None,
            labels: Vec::new(),
            body: None,
            url: None,
        };
        assert_eq!(minimal.format_as_intent(), "#42: Short title");
    }

    #[test]
    fn test_parse_issue_event_json_extracts_fields_and_ignores_comments_reactions() {
        let json = r###"{
            "action": "opened",
            "issue": {
                "number": 397,
                "title": "  Add --github-issue  ",
                "body": "## Problem\nSupport issue inquiry.",
                "labels": [{"name": "type: feat"}, {"name": " "}, {"name": "scope: cli"}],
                "user": {"login": "jimbojw"},
                "html_url": "https://github.com/PAIR-code/canon-clerk/issues/397",
                "comments": 12,
                "reactions": {"total_count": 5}
            }
        }"###;
        let parsed = parse_issue_event_json(json).expect("parse issue event");
        assert_eq!(parsed.number, 397);
        assert_eq!(parsed.title.as_deref(), Some("Add --github-issue"));
        assert_eq!(parsed.author.as_deref(), Some("jimbojw"));
        assert_eq!(parsed.labels, vec!["type: feat", "scope: cli"]);
        assert_eq!(
            parsed.body.as_deref(),
            Some("## Problem\nSupport issue inquiry.")
        );
        assert_eq!(
            parsed.url.as_deref(),
            Some("https://github.com/PAIR-code/canon-clerk/issues/397")
        );
    }

    #[test]
    fn test_parse_issue_event_json_rejects_malformed_missing_and_zero_number() {
        assert!(parse_issue_event_json("not json").is_err());
        assert!(parse_issue_event_json(r#"{"pull_request": {"number": 1}}"#).is_err());
        assert!(parse_issue_event_json(r#"{"issue": {"number": 0, "title": "bad"}}"#).is_err());
        assert!(parse_issue_event_json(r#"{"issue": {"title": "no number"}}"#).is_err());
    }

    #[test]
    fn test_parse_github_issue_inquiry_api_json_rejects_pull_requests() {
        let pr_json = r#"{
            "number": 444,
            "title": "Some PR",
            "pull_request": {"url": "https://api.github.com/repos/o/r/pulls/444"}
        }"#;
        let err = parse_github_issue_inquiry_api_json(pr_json, 444)
            .expect_err("pull request payload must be rejected");
        assert!(err.to_string().contains("Pull Request"));
        assert!(err.to_string().contains("--github-pr 444"));
    }

    #[test]
    fn test_bare_github_issue_reads_event_path_without_token_or_repo() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("canon-clerk-issue-event-{stamp}.json"));
        fs::write(
            &path,
            r#"{"issue":{"number":397,"title":"Issue inquiry","body":"Design body","labels":[{"name":"scope: cli"}]}}"#,
        )
        .expect("write event fixture");

        let mut env = BTreeMap::new();
        env.insert(
            ENV_GITHUB_EVENT_PATH.to_string(),
            path.to_string_lossy().into_owned(),
        );
        // Deliberately no GITHUB_TOKEN / GH_TOKEN / GITHUB_REPOSITORY.
        let fetcher = StubIssueFetcher {
            result: Err(GithubChecksError::Protocol(
                "fetcher must not be called in bare mode".to_string(),
            )),
        };

        let resolved =
            resolve_github_issue_inquiry(None, None, &env, &fetcher).expect("resolve bare issue");
        assert_eq!(resolved.number, 397);
        assert_eq!(resolved.title.as_deref(), Some("Issue inquiry"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_numbered_github_issue_validates_token_and_repo() {
        let fetcher = StubIssueFetcher {
            result: Ok(GithubIssueInquiry {
                number: 1,
                title: Some("ok".to_string()),
                author: None,
                labels: Vec::new(),
                body: None,
                url: None,
            }),
        };

        let issue_num = NonZeroU64::new(397).unwrap();
        let empty_env = BTreeMap::new();
        let missing_tok = resolve_github_issue_inquiry(Some(issue_num), None, &empty_env, &fetcher)
            .expect_err("missing token must fail");
        assert!(missing_tok.to_string().contains("GITHUB_TOKEN"));

        let mut token_only = BTreeMap::new();
        token_only.insert(ENV_GH_TOKEN.to_string(), "tok".to_string());
        let missing_repo =
            resolve_github_issue_inquiry(Some(issue_num), None, &token_only, &fetcher)
                .expect_err("missing repo must fail");
        assert!(missing_repo.to_string().contains("--github-repo"));

        let resolved = resolve_github_issue_inquiry(
            Some(issue_num),
            Some("PAIR-code/canon-clerk"),
            &token_only,
            &fetcher,
        )
        .expect("numbered issue resolves");
        assert_eq!(resolved.number, 1);
    }
}
