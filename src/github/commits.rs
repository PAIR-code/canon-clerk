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

//! GitHub Pull Request Commits API pagination, parsing, and cap guards (`docs/architecture/github.md` §3).

use super::client::extract_github_error_message;
use super::{GithubChecksError, GithubPullRequestTarget};
use crate::caseload::{CommitMessageContext, normalize_commit_messages};
use serde::Deserialize;

/// Maximum commits requested per GitHub REST API page.
const GITHUB_PR_COMMITS_PER_PAGE: usize = 100;

/// Maximum number of paginated HTTP requests issued for pull request commits.
const GITHUB_PR_COMMITS_MAX_PAGES: usize = 3;

/// Maximum number of pull request commits retained (matching GitHub's PR commits ceiling).
const GITHUB_PR_COMMITS_MAX_COUNT: usize = 250;

#[derive(Deserialize)]
struct GithubCommitRecord {
    #[serde(default)]
    sha: Option<String>,
    commit: GithubCommitInner,
}

#[derive(Deserialize)]
struct GithubCommitInner {
    message: String,
}

/// Extracts the `rel="next"` target URL from an RFC 8288 HTTP `Link` header, if present.
pub(crate) fn parse_next_link_header(link_header: &str) -> Option<String> {
    for segment in link_header.split(',') {
        let mut parts = segment.split(';');
        let raw_url = parts.next()?.trim();
        if !raw_url.starts_with('<') || !raw_url.ends_with('>') {
            continue;
        }
        let is_next = parts.any(|param| {
            let Some((key, val)) = param.trim().split_once('=') else {
                return false;
            };
            if !key.trim().eq_ignore_ascii_case("rel") {
                return false;
            }
            let unquoted = val.trim().trim_matches('"');
            unquoted
                .split_whitespace()
                .any(|token| token.eq_ignore_ascii_case("next"))
        });
        if is_next {
            let url = raw_url[1..raw_url.len() - 1].trim();
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
    }
    None
}

/// Parses a single page of GitHub REST PR commits JSON (`GET /repos/{owner}/{repo}/pulls/{pull_number}/commits`).
pub(crate) fn parse_github_pr_commits_page(
    body: &str,
) -> Result<Vec<CommitMessageContext>, GithubChecksError> {
    let records: Vec<GithubCommitRecord> = serde_json::from_str(body).map_err(|err| {
        GithubChecksError::Protocol(format!(
            "failed to decode GitHub Pull Request commits response: {err}\n  \
             Hint: Verify the GitHub API endpoint returned a valid JSON array of pull request commits."
        ))
    })?;

    Ok(records
        .into_iter()
        .map(|rec| CommitMessageContext {
            sha: rec.sha,
            message: rec.commit.message,
        })
        .collect())
}

/// Fetches pull request commits across paginated `Link: rel="next"` pages up to
/// [`GITHUB_PR_COMMITS_MAX_PAGES`] pages / [`GITHUB_PR_COMMITS_MAX_COUNT`] commits,
/// failing fast on HTTP/transport/decode errors and logging a diagnostic warning to `stderr`
/// if truncated at the cap.
pub(super) fn fetch_pr_commits_paginated<F>(
    target: &GithubPullRequestTarget,
    mut fetch_page: F,
) -> Result<Vec<CommitMessageContext>, GithubChecksError>
where
    F: FnMut(&str) -> Result<(u16, Option<String>, String), GithubChecksError>,
{
    let mut next_url = Some(format!(
        "{}/repos/{}/{}/pulls/{}/commits?per_page={GITHUB_PR_COMMITS_PER_PAGE}",
        target.api_base_url, target.owner, target.repo, target.pull_number
    ));
    let mut collected = Vec::new();
    let mut truncated = false;

    for page_idx in 1..=GITHUB_PR_COMMITS_MAX_PAGES {
        let Some(url) = next_url.take() else {
            break;
        };

        let (status, link_header, body) = fetch_page(&url).map_err(|err| match err {
            GithubChecksError::Transport { url, message } => GithubChecksError::Transport {
                url,
                message: format!(
                    "{message}\n  Hint: Check network connectivity to GitHub or pass explicit `--commit-message` / `--commit-messages-file` flags."
                ),
            },
            other => other,
        })?;

        if !(200..300).contains(&status) {
            let api_msg = extract_github_error_message(&body);
            return Err(GithubChecksError::Http {
                status,
                url,
                message: format!(
                    "{api_msg}\n  Hint: Verify `GITHUB_TOKEN` (or `GH_TOKEN`) has `pull-requests: read` permission and PR #{} exists in `{}/{}`.",
                    target.pull_number, target.owner, target.repo
                ),
            });
        }

        let page_commits = parse_github_pr_commits_page(&body)?;
        for commit in page_commits {
            if collected.len() >= GITHUB_PR_COMMITS_MAX_COUNT {
                truncated = true;
                break;
            }
            collected.push(commit);
        }

        if truncated {
            break;
        }

        let maybe_next = link_header.as_deref().and_then(parse_next_link_header);
        if maybe_next.is_some() {
            if page_idx == GITHUB_PR_COMMITS_MAX_PAGES
                || collected.len() >= GITHUB_PR_COMMITS_MAX_COUNT
            {
                truncated = true;
                break;
            }
            next_url = maybe_next;
        }
    }

    if truncated {
        eprintln!(
            "warning: GitHub PR #{} commit history truncated at {} commits ({} pages max); subsequent commits were omitted.",
            target.pull_number, GITHUB_PR_COMMITS_MAX_COUNT, GITHUB_PR_COMMITS_MAX_PAGES
        );
    }

    Ok(normalize_commit_messages(collected).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_target() -> GithubPullRequestTarget {
        GithubPullRequestTarget {
            api_base_url: "https://api.github.com".to_string(),
            owner: "PAIR-code".to_string(),
            repo: "canon-clerk".to_string(),
            pull_number: 384,
            token: "tok".to_string(),
        }
    }

    #[test]
    fn test_parse_next_link_header_extracts_rel_next_url() {
        let header = r#"<https://api.github.com/repos/o/r/pulls/1/commits?per_page=100&page=2>; rel="next", <https://api.github.com/repos/o/r/pulls/1/commits?per_page=100&page=3>; rel="last""#;
        assert_eq!(
            parse_next_link_header(header).as_deref(),
            Some("https://api.github.com/repos/o/r/pulls/1/commits?per_page=100&page=2")
        );

        let last_only =
            r#"<https://api.github.com/repos/o/r/pulls/1/commits?per_page=100&page=1>; rel="prev""#;
        assert_eq!(parse_next_link_header(last_only), None);
    }

    #[test]
    fn test_fetch_pr_commits_paginated_follows_link_and_preserves_order() {
        let mut seen_urls = Vec::new();
        let commits = fetch_pr_commits_paginated(&sample_target(), |url| {
            seen_urls.push(url.to_string());
            if seen_urls.len() == 1 {
                Ok((
                    200,
                    Some(r#"<https://api.github.com/repos/PAIR-code/canon-clerk/pulls/384/commits?per_page=100&page=2>; rel="next""#.to_string()),
                    r#"[{"sha":"aaa1111","commit":{"message":"feat(core): first\r\n\r\nBody"}},{"sha":"bbb2222","commit":{"message":"wip"}}]"#.to_string(),
                ))
            } else {
                Ok((
                    200,
                    None,
                    r#"[{"sha":"ccc3333","commit":{"message":"wip"}}]"#.to_string(),
                ))
            }
        })
        .expect("paginated fetch");

        assert_eq!(seen_urls.len(), 2);
        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0].sha.as_deref(), Some("aaa1111"));
        assert_eq!(commits[0].message, "feat(core): first\n\nBody");
        assert_eq!(commits[1].sha.as_deref(), Some("bbb2222"));
        assert_eq!(commits[1].message, "wip");
        assert_eq!(commits[2].sha.as_deref(), Some("ccc3333"));
        assert_eq!(commits[2].message, "wip");
    }

    #[test]
    fn test_fetch_pr_commits_paginated_enforces_250_commit_and_3_page_cap() {
        let mut calls = 0usize;
        let commits = fetch_pr_commits_paginated(&sample_target(), |_url| {
            calls += 1;
            let entries: Vec<String> = (0..100)
                .map(|i| {
                    format!(
                        r#"{{"sha":"p{calls}-{i:03}","commit":{{"message":"chore: commit {calls}-{i}"}}}}"#
                    )
                })
                .collect();
            let body = format!("[{}]", entries.join(","));
            let next_link = format!(
                r#"<https://api.github.com/repos/PAIR-code/canon-clerk/pulls/384/commits?per_page=100&page={}>; rel="next""#,
                calls + 1
            );
            Ok((200, Some(next_link), body))
        })
        .expect("capped fetch");

        assert_eq!(calls, 3);
        assert_eq!(commits.len(), GITHUB_PR_COMMITS_MAX_COUNT);
    }

    #[test]
    fn test_fetch_pr_commits_paginated_fails_fast_on_http_error_with_hint() {
        let err = fetch_pr_commits_paginated(&sample_target(), |_url| {
            Ok((
                403,
                None,
                r#"{"message":"Resource not accessible"}"#.to_string(),
            ))
        })
        .expect_err("HTTP 403 must fail fast");

        let msg = err.to_string();
        assert!(msg.contains("403"));
        assert!(msg.contains("Hint:"));
    }
}
