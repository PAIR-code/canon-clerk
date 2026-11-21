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

//! GitHub linked-issue discovery (`SPEC.md` §3.4) and REST payload projection.

use super::{GithubChecksError, GithubPullRequestFetcher, GithubPullRequestTarget};
use crate::caseload::LinkedIssueContext;
use serde::Deserialize;
use std::collections::BTreeSet;
use tracing::warn;

const CLOSING_KEYWORDS: &[&str] = &[
    "resolved", "resolves", "resolve", "closed", "closes", "close", "fixed", "fixes", "fix",
];

#[derive(Deserialize)]
struct GithubIssueApiResponse {
    number: u64,
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

/// Extracts unique, deterministically sorted (ascending) same-repo issue numbers referenced
/// by `SPEC.md` §3.4 closing keywords: `(close[sd]?|fix(e[sd])?|resolve[sd]?)\s+#(\d+)`.
fn extract_closing_issue_numbers(pr_body: &str) -> Vec<u64> {
    let mut numbers = BTreeSet::new();
    let bytes = pr_body.as_bytes();
    let mut idx = 0usize;

    while idx < bytes.len() {
        if !is_word_boundary_before(bytes, idx) {
            idx += 1;
            continue;
        }
        if let Some((after_kw, issue_num)) = match_closing_keyword_ref(pr_body, idx) {
            if issue_num >= 1 {
                numbers.insert(issue_num);
            }
            idx = after_kw;
        } else {
            idx += 1;
        }
    }

    numbers.into_iter().collect()
}

fn is_word_boundary_before(bytes: &[u8], idx: usize) -> bool {
    if idx == 0 {
        return true;
    }
    let prev = bytes[idx - 1];
    !(prev.is_ascii_alphanumeric() || prev == b'_')
}

fn match_closing_keyword_ref(text: &str, start: usize) -> Option<(usize, u64)> {
    let tail = text.get(start..)?;
    let keyword_len = CLOSING_KEYWORDS.iter().find_map(|kw| {
        let prefix = tail.get(..kw.len())?;
        if prefix.eq_ignore_ascii_case(kw) {
            Some(kw.len())
        } else {
            None
        }
    })?;

    let after_kw = tail.get(keyword_len..)?;
    let ws_len: usize = after_kw
        .chars()
        .take_while(|c| c.is_whitespace())
        .map(char::len_utf8)
        .sum();
    if ws_len == 0 {
        return None;
    }

    let after_ws = after_kw.get(ws_len..)?;
    let after_hash = after_ws.strip_prefix('#')?;
    let digits_len = after_hash
        .bytes()
        .take_while(|b| b.is_ascii_digit())
        .count();
    if digits_len == 0 {
        return None;
    }

    let digits = after_hash.get(..digits_len)?;
    let number = digits.parse::<u64>().ok()?;
    let end = start + keyword_len + ws_len + 1 + digits_len;
    Some((end, number))
}

/// Decodes a GitHub REST `GET /repos/{owner}/{repo}/issues/{number}` JSON payload into a
/// [`LinkedIssueContext`], returning `Ok(None)` when the endpoint returned a pull request.
pub(super) fn parse_github_issue_json(
    response_body: &str,
) -> Result<Option<LinkedIssueContext>, GithubChecksError> {
    let parsed: GithubIssueApiResponse = serde_json::from_str(response_body).map_err(|err| {
        GithubChecksError::Protocol(format!(
            "failed to decode GitHub Issue JSON response: {err}"
        ))
    })?;

    if parsed.pull_request.as_ref().is_some_and(|v| !v.is_null()) {
        return Ok(None);
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

    Ok(Some(LinkedIssueContext {
        number: parsed.number,
        title,
        author,
        labels,
        body,
        url,
    }))
}

/// Discovers linked issue numbers from `pr_body` and fetches each via `fetcher`,
/// gracefully skipping pull requests, `404`/`403`s, and per-issue errors with warnings.
pub fn fetch_linked_issues_for_pr_body(
    target: &GithubPullRequestTarget,
    pr_body: Option<&str>,
    fetcher: &dyn GithubPullRequestFetcher,
) -> Vec<LinkedIssueContext> {
    let Some(body) = pr_body else {
        return Vec::new();
    };
    let issue_numbers = extract_closing_issue_numbers(body);
    let mut issues = Vec::new();

    for issue_number in issue_numbers {
        match fetcher.fetch_issue(target, issue_number) {
            Ok(Some(issue)) => issues.push(issue),
            Ok(None) => {
                warn!(
                    owner = %target.owner,
                    repo = %target.repo,
                    issue_number,
                    "Skipped linked issue #{issue_number} (not found, forbidden, or pull request)."
                );
            }
            Err(err) => {
                warn!(
                    owner = %target.owner,
                    repo = %target.repo,
                    issue_number,
                    error = %err,
                    "Failed to fetch linked issue #{issue_number}; continuing without it."
                );
            }
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn test_extract_closing_issue_numbers_handles_all_keywords_case_and_dedup() {
        let body = "\
            Closes #383 and FIXES #42.\n\
            Also resolve   #7, resolved\t#42, close #100, fixed #9, resolves #7.\n\
            Should ignore prefixed #55, #999, closes #0, and closes#12.\n\
            Multi-line:\nClosed\n#15";
        assert_eq!(
            extract_closing_issue_numbers(body),
            vec![7, 9, 15, 42, 100, 383]
        );
    }

    #[test]
    fn test_parse_github_issue_json_projects_fields_and_skips_pull_requests() {
        let issue_json = r###"{
            "number": 383,
            "title": "  feat: ingest linked_issues  ",
            "user": { "login": "jimbojw" },
            "labels": [{ "name": "type: feat" }, { "name": "  " }, { "name": "scope: cli" }],
            "body": "## Problem\nWire linked_issues.",
            "html_url": "https://github.com/PAIR-code/canon-clerk/issues/383",
            "comments": 5,
            "reactions": { "total_count": 3 }
        }"###;
        let parsed = parse_github_issue_json(issue_json)
            .expect("parse issue")
            .expect("some issue");
        assert_eq!(parsed.number, 383);
        assert_eq!(parsed.title.as_deref(), Some("feat: ingest linked_issues"));
        assert_eq!(parsed.author.as_deref(), Some("jimbojw"));
        assert_eq!(parsed.labels, vec!["type: feat", "scope: cli"]);
        assert_eq!(
            parsed.body.as_deref(),
            Some("## Problem\nWire linked_issues.")
        );
        assert_eq!(
            parsed.url.as_deref(),
            Some("https://github.com/PAIR-code/canon-clerk/issues/383")
        );

        let pr_json = r#"{
            "number": 380,
            "title": "Some PR",
            "pull_request": { "url": "https://api.github.com/repos/o/r/pulls/380" }
        }"#;
        assert_eq!(
            parse_github_issue_json(pr_json).expect("parse pr-as-issue"),
            None
        );
    }

    struct MapIssueFetcher {
        responses: BTreeMap<u64, Result<Option<LinkedIssueContext>, GithubChecksError>>,
    }

    impl GithubPullRequestFetcher for MapIssueFetcher {
        fn fetch_pull_request_metadata(
            &self,
            _target: &GithubPullRequestTarget,
        ) -> Result<super::super::GithubPullRequestMetadata, GithubChecksError> {
            Ok(Default::default())
        }

        fn fetch_pull_request_diff(
            &self,
            _target: &GithubPullRequestTarget,
        ) -> Result<String, GithubChecksError> {
            Ok(String::new())
        }

        fn fetch_issue(
            &self,
            _target: &GithubPullRequestTarget,
            issue_number: u64,
        ) -> Result<Option<LinkedIssueContext>, GithubChecksError> {
            match self.responses.get(&issue_number) {
                Some(Ok(opt)) => Ok(opt.clone()),
                Some(Err(err)) => Err(err.clone()),
                None => Ok(None),
            }
        }
    }

    #[test]
    fn test_fetch_linked_issues_for_pr_body_degrades_gracefully_on_errors_and_skips() {
        let mut responses = BTreeMap::new();
        responses.insert(
            10,
            Ok(Some(LinkedIssueContext {
                number: 10,
                title: Some("First issue".to_string()),
                author: Some("alice".to_string()),
                labels: vec!["type: feat".to_string()],
                body: Some("Body 10".to_string()),
                url: None,
            })),
        );
        responses.insert(20, Ok(None)); // 404 or PR-as-issue
        responses.insert(
            30,
            Err(GithubChecksError::Transport {
                url: "http://example.invalid".to_string(),
                message: "timeout".to_string(),
            }),
        );

        let fetcher = MapIssueFetcher { responses };
        let target = GithubPullRequestTarget {
            api_base_url: "https://api.github.com".to_string(),
            owner: "o".to_string(),
            repo: "r".to_string(),
            pull_number: 99,
            token: "tok".to_string(),
        };

        let fetched = fetch_linked_issues_for_pr_body(
            &target,
            Some("Fixes #30, closes #10, resolves #20"),
            &fetcher,
        );
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].number, 10);
        assert_eq!(fetched[0].author.as_deref(), Some("alice"));
    }
}
