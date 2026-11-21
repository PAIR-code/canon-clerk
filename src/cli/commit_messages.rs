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

//! Local CLI `--commit-message <TEXT>` and strict-JSON `--commit-messages-file <PATH|->` parsers
//! (`SPEC.md` §4.2).

use crate::caseload::{CommitMessageContext, normalize_commit_messages};
use crate::cli::CliError;
use crate::cli::io::read_operand_source;
use serde_json::Value;
use std::io::Read;

const JSON_SHAPE_HINT: &str = "Hint: `--commit-messages-file` accepts strictly JSON in one of four shapes: \
(1) a JSON string or array of strings, \
(2) native `{\"sha\"?, \"message\"}` object/array, \
(3) GitHub REST PR commits JSON (`gh api repos/{owner}/{repo}/pulls/<N>/commits`), or \
(4) GitHub CLI JSON (`gh pr view <N> --json commits`).";

/// Resolves repeatable `--commit-message <TEXT>` or `--commit-messages-file <PATH|->` operands
/// into an optional chronological list of [`CommitMessageContext`] records.
///
/// Returns `Ok(None)` when neither flag was supplied, or `Ok(Some(vec))` when an explicit CLI
/// override was provided (even if all file entries normalized to empty, so callers know to skip
/// `--github-pr` Commits API calls).
pub(super) fn resolve_commit_message_operands(
    literal_messages: &[String],
    file_sources: &[String],
    stdin: &mut dyn Read,
) -> Result<Option<Vec<CommitMessageContext>>, CliError> {
    if !literal_messages.is_empty() && !file_sources.is_empty() {
        return Err(CliError::usage(
            "Flags `--commit-message` and `--commit-messages-file` are mutually exclusive.\n  \
             Hint: Pass either repeatable `--commit-message <TEXT>` or `--commit-messages-file <PATH|->`.",
        ));
    }

    if !literal_messages.is_empty() {
        let mut commits = Vec::with_capacity(literal_messages.len());
        for raw in literal_messages {
            if raw.trim().is_empty() {
                return Err(CliError::usage(
                    "Flag `--commit-message` requires a non-empty commit message.\n  \
                     Hint: Pass a non-empty commit message string (e.g. `--commit-message \"feat(cli): add flag\"`).",
                ));
            }
            commits.push(CommitMessageContext {
                sha: None,
                message: raw.clone(),
            });
        }
        return Ok(Some(normalize_commit_messages(commits).unwrap_or_default()));
    }

    if file_sources.is_empty() {
        return Ok(None);
    }

    let mut collected = Vec::new();
    for source in file_sources {
        let raw = read_operand_source(source, "--commit-messages-file", stdin)?;
        let parsed = parse_commit_messages_json(&raw, source)?;
        collected.extend(parsed);
    }

    Ok(Some(
        normalize_commit_messages(collected).unwrap_or_default(),
    ))
}

/// Strictly parses commit messages from JSON in any of the 4 supported dialects.
pub(crate) fn parse_commit_messages_json(
    raw: &str,
    source_label: &str,
) -> Result<Vec<CommitMessageContext>, CliError> {
    let value: Value = serde_json::from_str(raw).map_err(|err| {
        CliError::usage(format!(
            "Failed to parse `--commit-messages-file` JSON from `{source_label}`: {err}\n  \
             {JSON_SHAPE_HINT}"
        ))
    })?;

    parse_commit_messages_value(&value, source_label)
}

fn parse_commit_messages_value(
    value: &Value,
    source_label: &str,
) -> Result<Vec<CommitMessageContext>, CliError> {
    match value {
        Value::String(s) => Ok(vec![CommitMessageContext {
            sha: None,
            message: s.clone(),
        }]),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(parse_single_commit_item(item, source_label)?);
            }
            Ok(out)
        }
        Value::Object(map) => {
            if let Some(commits_val) = map.get("commits") {
                let items = commits_val.as_array().ok_or_else(|| {
                    CliError::usage(format!(
                        "Unsupported `--commit-messages-file` JSON in `{source_label}`: `commits` must be a JSON array.\n  \
                         {JSON_SHAPE_HINT}"
                    ))
                })?;
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(parse_single_commit_item(item, source_label)?);
                }
                return Ok(out);
            }
            Ok(vec![parse_single_commit_item(value, source_label)?])
        }
        _ => Err(CliError::usage(format!(
            "Unsupported `--commit-messages-file` JSON shape in `{source_label}`.\n  \
             {JSON_SHAPE_HINT}"
        ))),
    }
}

fn parse_single_commit_item(
    value: &Value,
    source_label: &str,
) -> Result<CommitMessageContext, CliError> {
    if let Some(s) = value.as_str() {
        return Ok(CommitMessageContext {
            sha: None,
            message: s.to_string(),
        });
    }

    let Some(obj) = value.as_object() else {
        return Err(CliError::usage(format!(
            "Unsupported commit entry in `--commit-messages-file` `{source_label}`: expected a string or commit object.\n  \
             {JSON_SHAPE_HINT}"
        )));
    };

    // Dialect 2: Native `{"sha"?, "message"}`
    if let Some(msg_val) = obj.get("message") {
        let message = msg_val.as_str().ok_or_else(|| {
            CliError::usage(format!(
                "Invalid commit entry in `{source_label}`: `message` must be a string.\n  \
                 {JSON_SHAPE_HINT}"
            ))
        })?;
        let sha = extract_optional_string(obj.get("sha"), "sha", source_label)?;
        return Ok(CommitMessageContext {
            sha,
            message: message.to_string(),
        });
    }

    // Dialect 3: GitHub REST PR commits `{"sha"?, "commit": {"message": "..."}}`
    if let Some(commit_val) = obj.get("commit") {
        let commit_obj = commit_val.as_object().ok_or_else(|| {
            CliError::usage(format!(
                "Invalid GitHub REST commit entry in `{source_label}`: `commit` must be an object.\n  \
                 {JSON_SHAPE_HINT}"
            ))
        })?;
        let message = commit_obj
            .get("message")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                CliError::usage(format!(
                    "Invalid GitHub REST commit entry in `{source_label}`: `commit.message` must be a string.\n  \
                     {JSON_SHAPE_HINT}"
                ))
            })?;
        let sha = extract_optional_string(obj.get("sha"), "sha", source_label)?;
        return Ok(CommitMessageContext {
            sha,
            message: message.to_string(),
        });
    }

    // Dialect 4: GitHub CLI `{"oid"?, "messageHeadline": "...", "messageBody"?: "..."}`
    if let Some(headline_val) = obj.get("messageHeadline") {
        let headline = headline_val.as_str().ok_or_else(|| {
            CliError::usage(format!(
                "Invalid GitHub CLI commit entry in `{source_label}`: `messageHeadline` must be a string.\n  \
                 {JSON_SHAPE_HINT}"
            ))
        })?;
        let body = extract_optional_string(obj.get("messageBody"), "messageBody", source_label)?;
        let sha = extract_optional_string(obj.get("oid"), "oid", source_label)?;
        let message = match body.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(non_empty_body) => format!("{}\n\n{non_empty_body}", headline.trim_end()),
            None => headline.to_string(),
        };
        return Ok(CommitMessageContext { sha, message });
    }

    Err(CliError::usage(format!(
        "Unsupported commit object in `--commit-messages-file` `{source_label}`: expected `message`, `commit.message`, or `messageHeadline`.\n  \
         {JSON_SHAPE_HINT}"
    )))
}

fn extract_optional_string(
    val: Option<&Value>,
    field_name: &str,
    source_label: &str,
) -> Result<Option<String>, CliError> {
    match val {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(CliError::usage(format!(
            "Invalid commit entry in `{source_label}`: `{field_name}` must be a string or null.\n  \
             {JSON_SHAPE_HINT}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_parse_all_four_json_dialects() {
        // 1. String & string array
        let single_str = parse_commit_messages_json(r#""feat(cli): single""#, "a.json").unwrap();
        assert_eq!(single_str.len(), 1);
        assert_eq!(single_str[0].message, "feat(cli): single");

        let str_arr = parse_commit_messages_json(r#"["feat: one", "fix: two"]"#, "b.json").unwrap();
        assert_eq!(str_arr.len(), 2);
        assert_eq!(str_arr[1].message, "fix: two");

        // 2. Native CommitMessageContext object & array
        let native = parse_commit_messages_json(
            r#"{"sha":"abc1234","message":"feat: native"}"#,
            "native.json",
        )
        .unwrap();
        assert_eq!(native[0].sha.as_deref(), Some("abc1234"));
        assert_eq!(native[0].message, "feat: native");

        // 3. GitHub REST PR commits JSON
        let gh_rest = parse_commit_messages_json(
            r#"[{"sha":"def5678","commit":{"message":"fix(core): rest\r\n\r\nDetails"}}]"#,
            "rest.json",
        )
        .unwrap();
        assert_eq!(gh_rest[0].sha.as_deref(), Some("def5678"));
        assert_eq!(gh_rest[0].message, "fix(core): rest\r\n\r\nDetails");

        // 4. GitHub CLI `gh pr view --json commits`
        let gh_cli = parse_commit_messages_json(
            r#"{"commits":[{"oid":"999aaaa","messageHeadline":"feat: headline","messageBody":"Body paragraph"},{"oid":"888bbbb","messageHeadline":"chore: no body","messageBody":""}]}"#,
            "gh.json",
        )
        .unwrap();
        assert_eq!(gh_cli.len(), 2);
        assert_eq!(gh_cli[0].sha.as_deref(), Some("999aaaa"));
        assert_eq!(gh_cli[0].message, "feat: headline\n\nBody paragraph");
        assert_eq!(gh_cli[1].sha.as_deref(), Some("888bbbb"));
        assert_eq!(gh_cli[1].message, "chore: no body");
    }

    #[test]
    fn test_rejects_plain_text_malformed_json_and_blank_literal() {
        let err = parse_commit_messages_json("feat: plain text", "plain.txt").unwrap_err();
        assert!(err.to_string().contains("Hint:"));

        let err = parse_commit_messages_json(r#"{"unknown": 1}"#, "bad.json").unwrap_err();
        assert!(err.to_string().contains("Hint:"));

        let mut stdin = Cursor::new(Vec::<u8>::new());
        let err =
            resolve_commit_message_operands(&["   \n ".to_string()], &[], &mut stdin).unwrap_err();
        assert!(err.to_string().contains("Hint:"));
    }
}
