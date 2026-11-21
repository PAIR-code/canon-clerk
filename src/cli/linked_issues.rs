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

//! Local CLI `--linked-issue <PATH|->` JSON parser supporting native [`LinkedIssueContext`],
//! `gh issue view --json`, and GitHub REST API issue shapes (`SPEC.md` §3.4).

use crate::caseload::LinkedIssueContext;
use crate::cli::CliError;
use crate::cli::io::read_operand_source;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::Read;

#[derive(Deserialize)]
#[serde(untagged)]
enum LinkedIssuesJsonInput {
    Single(LinkedIssueJsonRecord),
    Many(Vec<LinkedIssueJsonRecord>),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AuthorJsonInput {
    Plain(String),
    Object { login: Option<String> },
}

impl AuthorJsonInput {
    fn into_login(self) -> Option<String> {
        let raw = match self {
            Self::Plain(s) => Some(s),
            Self::Object { login } => login,
        };
        raw.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LabelJsonInput {
    Plain(String),
    Object { name: Option<String> },
}

impl LabelJsonInput {
    fn into_name(self) -> Option<String> {
        let raw = match self {
            Self::Plain(s) => Some(s),
            Self::Object { name } => name,
        };
        raw.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }
}

#[derive(Deserialize)]
struct LinkedIssueJsonRecord {
    number: u64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    author: Option<AuthorJsonInput>,
    #[serde(default)]
    user: Option<AuthorJsonInput>,
    #[serde(default)]
    labels: Vec<LabelJsonInput>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default, alias = "html_url")]
    url: Option<String>,
}

impl LinkedIssueJsonRecord {
    fn into_context(self, source_label: &str) -> Result<LinkedIssueContext, CliError> {
        if self.number == 0 {
            return Err(CliError::usage(format!(
                "Invalid linked issue record in `{source_label}`: `number` must be a positive integer (`>= 1`).\n  \
                 Hint: Supply valid issue JSON (e.g. from `gh issue view <NUM> --json number,title,author,labels,body,url`)."
            )));
        }
        let author = self
            .author
            .and_then(AuthorJsonInput::into_login)
            .or_else(|| self.user.and_then(AuthorJsonInput::into_login));
        let labels = self
            .labels
            .into_iter()
            .filter_map(LabelJsonInput::into_name)
            .collect();
        let title = self
            .title
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let body = self.body.filter(|s| !s.trim().is_empty());
        let url = self
            .url
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        Ok(LinkedIssueContext {
            number: self.number,
            title,
            author,
            labels,
            body,
            url,
        })
    }
}

/// Strictly parses linked-issue JSON (single object or JSON array) in native
/// [`LinkedIssueContext`], `gh issue view --json`, or GitHub REST API issue shape.
pub fn parse_linked_issues_json(
    raw: &str,
    source_label: &str,
) -> Result<Vec<LinkedIssueContext>, CliError> {
    let parsed: LinkedIssuesJsonInput = serde_json::from_str(raw).map_err(|err| {
        CliError::usage(format!(
            "Failed to parse `--linked-issue` JSON from `{source_label}`: {err}\n  \
             Hint: `--linked-issue` accepts strictly JSON (a single issue object or JSON array), such as output from `gh issue view <NUM> --json number,title,author,labels,body,url`."
        ))
    })?;

    let records = match parsed {
        LinkedIssuesJsonInput::Single(one) => vec![one],
        LinkedIssuesJsonInput::Many(many) => many,
    };

    let mut out = Vec::with_capacity(records.len());
    for record in records {
        out.push(record.into_context(source_label)?);
    }
    Ok(out)
}

/// Reads and parses repeatable `--linked-issue <PATH|->` operands, deduplicating by issue number
/// in deterministic ascending order. Returns `Ok(None)` when `sources` is empty.
pub fn resolve_linked_issue_operands(
    sources: &[String],
    stdin: &mut dyn Read,
) -> Result<Option<Vec<LinkedIssueContext>>, CliError> {
    if sources.is_empty() {
        return Ok(None);
    }

    let mut by_number: BTreeMap<u64, LinkedIssueContext> = BTreeMap::new();
    for source in sources {
        let raw = read_operand_source(source, "--linked-issue", stdin)?;
        let parsed = parse_linked_issues_json(&raw, source)?;
        for issue in parsed {
            by_number.insert(issue.number, issue);
        }
    }

    Ok(Some(by_number.into_values().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_parse_native_gh_cli_and_rest_api_json_dialects() {
        // 1. Native LinkedIssueContext
        let native = r#"{
            "number": 10,
            "title": "Native issue",
            "author": "alice",
            "labels": ["type: feat"],
            "body": "Native body",
            "url": "https://example.test/issues/10"
        }"#;
        let parsed_native = parse_linked_issues_json(native, "native.json").expect("native");
        assert_eq!(parsed_native.len(), 1);
        assert_eq!(parsed_native[0].author.as_deref(), Some("alice"));
        assert_eq!(parsed_native[0].labels, vec!["type: feat"]);

        // 2. gh issue view --json (including comments/reactions which must be ignored)
        let gh_json = r#"{
            "number": 383,
            "title": "GH issue",
            "author": { "login": "jimbojw", "name": "Jimbo" },
            "labels": [{ "name": "scope: cli", "color": "006B75" }],
            "body": "GH body",
            "url": "https://github.com/PAIR-code/canon-clerk/issues/383",
            "comments": [{ "body": "should be ignored" }]
        }"#;
        let parsed_gh = parse_linked_issues_json(gh_json, "-").expect("gh json");
        assert_eq!(parsed_gh[0].number, 383);
        assert_eq!(parsed_gh[0].author.as_deref(), Some("jimbojw"));
        assert_eq!(parsed_gh[0].labels, vec!["scope: cli"]);
        assert_eq!(
            parsed_gh[0].url.as_deref(),
            Some("https://github.com/PAIR-code/canon-clerk/issues/383")
        );

        // 3. GitHub REST API JSON array
        let rest_array = r#"[
            {
                "number": 50,
                "title": "REST issue",
                "user": { "login": "octocat" },
                "labels": [{ "name": "bug" }],
                "body": "REST body",
                "html_url": "https://github.com/o/r/issues/50"
            },
            {
                "number": 12,
                "title": "Earlier issue"
            }
        ]"#;
        let resolved =
            resolve_linked_issue_operands(&["-".to_string()], &mut Cursor::new(rest_array))
                .expect("resolve array")
                .expect("some issues");
        assert_eq!(resolved.len(), 2);
        // Sorted deterministically by ascending issue number
        assert_eq!(resolved[0].number, 12);
        assert_eq!(resolved[1].number, 50);
        assert_eq!(resolved[1].author.as_deref(), Some("octocat"));
        assert_eq!(
            resolved[1].url.as_deref(),
            Some("https://github.com/o/r/issues/50")
        );
    }

    #[test]
    fn test_malformed_json_and_zero_issue_number_fail_fast_with_hint() {
        let err_md =
            parse_linked_issues_json("# Plain Markdown Issue\nNot JSON", "issue.md").unwrap_err();
        assert_eq!(err_md.exit_code(), 2);
        assert!(err_md.to_string().contains("Hint:"));

        let err_zero =
            parse_linked_issues_json(r#"{"number": 0, "title": "Bad"}"#, "zero.json").unwrap_err();
        assert_eq!(err_zero.exit_code(), 2);
        assert!(err_zero.to_string().contains("Hint:"));
    }
}
