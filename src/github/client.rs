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

//! HTTP client implementation for GitHub Checks REST API publication.

use super::linked_issues::parse_github_issue_json;
use super::{
    CheckRunPublisher, CheckRunReport, DEFAULT_GITHUB_API_TIMEOUT_MS,
    GITHUB_CHECKS_MAX_ANNOTATIONS_PER_REQUEST, GITHUB_CHECKS_MAX_OUTPUT_CHARS,
    GithubCheckAnnotation, GithubCheckTarget, GithubChecksError, GithubPullRequestFetcher,
    GithubPullRequestMetadata, GithubPullRequestTarget, PublishedCheckRun,
    sanitize_annotation_for_github, truncate_github_output,
};
use crate::caseload::LinkedIssueContext;
use crate::provider::{build_pooled_http_client, http_reactor_handle};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::sync::mpsc;
use std::time::Duration;

const GITHUB_API_ACCEPT: &str = "application/vnd.github+json";
const GITHUB_API_DIFF_ACCEPT: &str = "application/vnd.github.v3.diff";
const GITHUB_API_VERSION: &str = "2022-11-28";
const GITHUB_USER_AGENT: &str = concat!("canon-clerk/", env!("CARGO_PKG_VERSION"));

#[derive(Serialize)]
struct CreateCheckRunBody<'a> {
    name: &'a str,
    head_sha: &'a str,
    status: &'static str,
    conclusion: &'static str,
    output: CheckRunOutputBody<'a>,
}

#[derive(Serialize)]
struct UpdateCheckRunBody<'a> {
    output: CheckRunOutputBody<'a>,
}

#[derive(Serialize)]
struct CheckRunOutputBody<'a> {
    title: &'a str,
    summary: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<&'a str>,
    annotations: &'a [GithubCheckAnnotation],
}

#[derive(Deserialize)]
struct CheckRunApiResponse {
    id: u64,
    #[serde(default)]
    html_url: Option<String>,
}

#[derive(Deserialize)]
struct GithubErrorResponse {
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize)]
struct PullRequestApiResponse {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    head: Option<PullRequestHeadRef>,
    #[serde(default)]
    labels: Vec<PullRequestLabelRef>,
}

#[derive(Deserialize)]
struct PullRequestHeadRef {
    #[serde(default)]
    sha: Option<String>,
}

#[derive(Deserialize)]
struct PullRequestLabelRef {
    #[serde(default)]
    name: Option<String>,
}

/// Live GitHub Checks REST API client executing on Canon Clerk's persistent background Tokio reactor.
#[derive(Debug, Clone)]
pub struct GithubChecksClient {
    client: reqwest::Client,
    timeout_ms: u64,
}

impl Default for GithubChecksClient {
    fn default() -> Self {
        Self::new()
    }
}

impl GithubChecksClient {
    /// Creates a new [`GithubChecksClient`] with the default 30-second per-request wire timeout.
    pub fn new() -> Self {
        Self::with_timeout_ms(DEFAULT_GITHUB_API_TIMEOUT_MS)
    }

    /// Creates a new [`GithubChecksClient`] with an explicit per-request wire timeout in milliseconds.
    pub fn with_timeout_ms(timeout_ms: u64) -> Self {
        Self {
            client: build_pooled_http_client(),
            timeout_ms: timeout_ms.max(1),
        }
    }

    fn send_json<T: Serialize>(
        &self,
        method: Method,
        url: &str,
        token: &str,
        payload: &T,
    ) -> Result<(u16, String), GithubChecksError> {
        let body_bytes = serde_json::to_vec(payload).map_err(|err| {
            GithubChecksError::Protocol(format!(
                "failed to serialize GitHub Check Run payload: {err}"
            ))
        })?;
        let handle = http_reactor_handle("github-checks").map_err(|(err, _)| {
            GithubChecksError::Transport {
                url: url.to_string(),
                message: err.to_string(),
            }
        })?;

        let client = self.client.clone();
        let owned_url = url.to_string();
        let owned_token = token.to_string();
        let timeout = Duration::from_millis(self.timeout_ms);
        let (tx, rx) = mpsc::sync_channel(1);

        handle.spawn(async move {
            let result = execute_github_request(
                &client,
                method,
                &owned_url,
                &owned_token,
                body_bytes,
                timeout,
            )
            .await;
            let _ = tx.send(result);
        });

        rx.recv().map_err(|_| GithubChecksError::Transport {
            url: url.to_string(),
            message: "background HTTP task terminated unexpectedly".to_string(),
        })?
    }

    fn send_get_with_link(
        &self,
        url: &str,
        token: &str,
        accept: &str,
    ) -> Result<(u16, Option<String>, String), GithubChecksError> {
        let handle =
            http_reactor_handle("github-pr").map_err(|(err, _)| GithubChecksError::Transport {
                url: url.to_string(),
                message: err.to_string(),
            })?;

        let client = self.client.clone();
        let owned_url = url.to_string();
        let owned_token = token.to_string();
        let owned_accept = accept.to_string();
        let timeout = Duration::from_millis(self.timeout_ms);
        let (tx, rx) = mpsc::sync_channel(1);

        handle.spawn(async move {
            let result = execute_github_get_request(
                &client,
                &owned_url,
                &owned_token,
                &owned_accept,
                timeout,
            )
            .await;
            let _ = tx.send(result);
        });

        rx.recv().map_err(|_| GithubChecksError::Transport {
            url: url.to_string(),
            message: "background HTTP task terminated unexpectedly".to_string(),
        })?
    }
}

async fn execute_github_get_request(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    accept: &str,
    timeout: Duration,
) -> Result<(u16, Option<String>, String), GithubChecksError> {
    let request = client
        .request(Method::GET, url)
        .timeout(timeout)
        .header(reqwest::header::ACCEPT, accept)
        .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
        .header("User-Agent", GITHUB_USER_AGENT)
        .bearer_auth(token)
        .build()
        .map_err(|err| GithubChecksError::Transport {
            url: url.to_string(),
            message: err.to_string(),
        })?;

    let response = client
        .execute(request)
        .await
        .map_err(|err| GithubChecksError::Transport {
            url: url.to_string(),
            message: err.to_string(),
        })?;

    let status = response.status().as_u16();
    let link_header = response
        .headers()
        .get(reqwest::header::LINK)
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned);
    let body = response
        .text()
        .await
        .map_err(|err| GithubChecksError::Transport {
            url: url.to_string(),
            message: err.to_string(),
        })?;

    Ok((status, link_header, body))
}

async fn execute_github_request(
    client: &reqwest::Client,
    method: Method,
    url: &str,
    token: &str,
    body_bytes: Vec<u8>,
    timeout: Duration,
) -> Result<(u16, String), GithubChecksError> {
    let request = client
        .request(method, url)
        .timeout(timeout)
        .header(reqwest::header::ACCEPT, GITHUB_API_ACCEPT)
        .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
        .header(reqwest::header::USER_AGENT, GITHUB_USER_AGENT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .bearer_auth(token)
        .body(body_bytes)
        .build()
        .map_err(|err| GithubChecksError::Transport {
            url: url.to_string(),
            message: err.to_string(),
        })?;

    let response = client
        .execute(request)
        .await
        .map_err(|err| GithubChecksError::Transport {
            url: url.to_string(),
            message: err.to_string(),
        })?;

    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|err| GithubChecksError::Transport {
            url: url.to_string(),
            message: err.to_string(),
        })?;

    Ok((status, body))
}

pub(super) fn extract_github_error_message(body: &str) -> String {
    if let Ok(parsed) = serde_json::from_str::<GithubErrorResponse>(body)
        && let Some(msg) = parsed.message.map(|m| m.trim().to_string())
        && !msg.is_empty()
    {
        return msg;
    }
    let trimmed = body.trim();
    if trimmed.is_empty() {
        "empty error response body".to_string()
    } else {
        trimmed.chars().take(400).collect()
    }
}

impl CheckRunPublisher for GithubChecksClient {
    fn publish_check_run(
        &self,
        target: &GithubCheckTarget,
        report: &CheckRunReport,
    ) -> Result<PublishedCheckRun, GithubChecksError> {
        let sanitized: Vec<GithubCheckAnnotation> = report
            .annotations
            .iter()
            .map(sanitize_annotation_for_github)
            .collect();
        let summary = truncate_github_output(&report.summary, GITHUB_CHECKS_MAX_OUTPUT_CHARS);
        let text = report
            .text
            .as_deref()
            .filter(|t| !t.is_empty())
            .map(|t| truncate_github_output(t, GITHUB_CHECKS_MAX_OUTPUT_CHARS));

        let mut chunks = sanitized.chunks(GITHUB_CHECKS_MAX_ANNOTATIONS_PER_REQUEST);
        let first_batch = chunks.next().unwrap_or(&[]);

        let create_url = format!(
            "{}/repos/{}/{}/check-runs",
            target.api_base_url, target.owner, target.repo
        );
        let create_body = CreateCheckRunBody {
            name: &report.name,
            head_sha: &target.head_sha,
            status: "completed",
            conclusion: report.conclusion.as_str(),
            output: CheckRunOutputBody {
                title: &report.title,
                summary: &summary,
                text: text.as_deref(),
                annotations: first_batch,
            },
        };

        let (status, resp_body) =
            self.send_json(Method::POST, &create_url, &target.token, &create_body)?;
        if !(200..300).contains(&status) {
            return Err(GithubChecksError::Http {
                status,
                url: create_url,
                message: extract_github_error_message(&resp_body),
            });
        }

        let created: CheckRunApiResponse = serde_json::from_str(&resp_body).map_err(|err| {
            GithubChecksError::Protocol(format!(
                "failed to decode GitHub Check Run creation response: {err}"
            ))
        })?;

        let mut requests_sent = 1usize;
        let update_url = format!(
            "{}/repos/{}/{}/check-runs/{}",
            target.api_base_url, target.owner, target.repo, created.id
        );

        for batch in chunks {
            let patch_body = UpdateCheckRunBody {
                output: CheckRunOutputBody {
                    title: &report.title,
                    summary: &summary,
                    text: text.as_deref(),
                    annotations: batch,
                },
            };
            let (patch_status, patch_resp) =
                self.send_json(Method::PATCH, &update_url, &target.token, &patch_body)?;
            requests_sent += 1;
            if !(200..300).contains(&patch_status) {
                return Err(GithubChecksError::Http {
                    status: patch_status,
                    url: update_url,
                    message: extract_github_error_message(&patch_resp),
                });
            }
        }

        Ok(PublishedCheckRun {
            id: created.id,
            html_url: created.html_url,
            annotations_published: sanitized.len(),
            requests_sent,
        })
    }
}

impl GithubPullRequestFetcher for GithubChecksClient {
    fn fetch_pull_request_metadata(
        &self,
        target: &GithubPullRequestTarget,
    ) -> Result<GithubPullRequestMetadata, GithubChecksError> {
        let url = format!(
            "{}/repos/{}/{}/pulls/{}",
            target.api_base_url, target.owner, target.repo, target.pull_number
        );
        let (status, _, body) = self.send_get_with_link(&url, &target.token, GITHUB_API_ACCEPT)?;
        if !(200..300).contains(&status) {
            return Err(GithubChecksError::Http {
                status,
                url,
                message: extract_github_error_message(&body),
            });
        }
        let parsed: PullRequestApiResponse = serde_json::from_str(&body).map_err(|err| {
            GithubChecksError::Protocol(format!(
                "failed to decode GitHub Pull Request metadata response: {err}"
            ))
        })?;
        Ok(GithubPullRequestMetadata {
            title: parsed
                .title
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            body: parsed.body.filter(|s| !s.trim().is_empty()),
            head_sha: parsed
                .head
                .and_then(|h| h.sha)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            labels: parsed
                .labels
                .into_iter()
                .filter_map(|label| {
                    label
                        .name
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                })
                .collect(),
            actor: None,
        })
    }

    fn fetch_pull_request_diff(
        &self,
        target: &GithubPullRequestTarget,
    ) -> Result<String, GithubChecksError> {
        let url = format!(
            "{}/repos/{}/{}/pulls/{}",
            target.api_base_url, target.owner, target.repo, target.pull_number
        );
        let (status, _, body) =
            self.send_get_with_link(&url, &target.token, GITHUB_API_DIFF_ACCEPT)?;
        if !(200..300).contains(&status) {
            return Err(GithubChecksError::Http {
                status,
                url,
                message: extract_github_error_message(&body),
            });
        }
        Ok(body)
    }

    fn fetch_issue(
        &self,
        target: &GithubPullRequestTarget,
        issue_number: u64,
    ) -> Result<Option<LinkedIssueContext>, GithubChecksError> {
        let url = format!(
            "{}/repos/{}/{}/issues/{}",
            target.api_base_url, target.owner, target.repo, issue_number
        );
        let (status, _, body) = self.send_get_with_link(&url, &target.token, GITHUB_API_ACCEPT)?;
        if status == 404 || status == 403 {
            return Ok(None);
        }
        if !(200..300).contains(&status) {
            return Err(GithubChecksError::Http {
                status,
                url,
                message: extract_github_error_message(&body),
            });
        }
        parse_github_issue_json(&body)
    }

    fn fetch_pull_request_commits(
        &self,
        target: &GithubPullRequestTarget,
    ) -> Result<Vec<crate::caseload::CommitMessageContext>, GithubChecksError> {
        super::commits::fetch_pr_commits_paginated(target, |url| {
            self.send_get_with_link(url, &target.token, GITHUB_API_ACCEPT)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{AnnotationLevel, CodeAnnotation};
    use crate::github::CheckRunConclusion;
    use serde_json::Value;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread;

    struct CapturedRequest {
        request_line: String,
        headers: String,
        body: Value,
    }

    fn read_http_request(stream: &mut TcpStream) -> CapturedRequest {
        let mut buf = Vec::new();
        let mut temp = [0u8; 4096];
        let mut header_end = None;
        let mut content_length = 0usize;

        while header_end.is_none() {
            let n = stream.read(&mut temp).expect("read request");
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&temp[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                header_end = Some(pos + 4);
                let headers_str = String::from_utf8_lossy(&buf[..pos]);
                for line in headers_str.lines() {
                    if let Some((k, v)) = line.split_once(':')
                        && k.eq_ignore_ascii_case("content-length")
                    {
                        content_length = v.trim().parse::<usize>().unwrap_or(0);
                    }
                }
            }
        }

        let start = header_end.expect("header terminator");
        while buf.len() < start + content_length {
            let n = stream.read(&mut temp).expect("read body");
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&temp[..n]);
        }

        let headers_raw = String::from_utf8_lossy(&buf[..start]).to_string();
        let request_line = headers_raw.lines().next().unwrap_or("").to_string();
        let body_bytes = &buf[start..start + content_length];
        let body: Value = if body_bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(body_bytes).expect("valid JSON request body")
        };

        CapturedRequest {
            request_line,
            headers: headers_raw,
            body,
        }
    }

    fn write_http_response(stream: &mut TcpStream, status: &str, body: &str) {
        let resp = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(resp.as_bytes()).expect("write response");
        stream.flush().expect("flush response");
    }

    fn sample_target(addr: std::net::SocketAddr) -> GithubCheckTarget {
        GithubCheckTarget {
            api_base_url: format!("http://{addr}"),
            owner: "PAIR-code".to_string(),
            repo: "canon-clerk".to_string(),
            head_sha: "abc123def456".to_string(),
            token: "ghs_test_token".to_string(),
        }
    }

    fn build_sample_annotations(count: usize) -> Vec<CodeAnnotation> {
        (1..=count)
            .map(|i| CodeAnnotation {
                path: format!("src/file_{i}.rs"),
                start_line: i,
                end_line: if i == 1 { i + 3 } else { i },
                start_column: Some(2),
                end_column: Some(10),
                annotation_level: AnnotationLevel::Failure,
                message: format!("Violation #{i}"),
                title: Some("test-canon".to_string()),
            })
            .collect()
    }

    fn spawn_batch_mock_server(
        expected_requests: usize,
    ) -> (
        std::net::SocketAddr,
        Arc<Mutex<Vec<CapturedRequest>>>,
        thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let captured = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
        let captured_ref = Arc::clone(&captured);

        let server = thread::spawn(move || {
            for idx in 0..expected_requests {
                let (mut stream, _) = listener.accept().expect("accept request");
                let req = read_http_request(&mut stream);
                captured_ref.lock().expect("lock").push(req);
                if idx == 0 {
                    write_http_response(
                        &mut stream,
                        "201 Created",
                        r#"{"id":987654,"html_url":"https://github.com/PAIR-code/canon-clerk/runs/987654"}"#,
                    );
                } else {
                    write_http_response(&mut stream, "200 OK", r#"{"id":987654}"#);
                }
            }
        });
        (addr, captured, server)
    }

    fn annotation_count(req: &CapturedRequest) -> usize {
        req.body["output"]["annotations"]
            .as_array()
            .expect("annotations array")
            .len()
    }

    fn assert_create_request_headers_and_metadata(req: &CapturedRequest) {
        assert_eq!(
            req.request_line,
            "POST /repos/PAIR-code/canon-clerk/check-runs HTTP/1.1"
        );
        let lower_headers = req.headers.to_ascii_lowercase();
        assert!(lower_headers.contains("authorization: bearer ghs_test_token"));
        assert!(lower_headers.contains("x-github-api-version: 2022-11-28"));
        assert!(lower_headers.contains("user-agent: canon-clerk/"));
        assert_eq!(req.body["conclusion"], "failure");
        assert_eq!(req.body["status"], "completed");
        assert_eq!(req.body["head_sha"], "abc123def456");
    }

    #[test]
    fn test_batches_120_annotations_across_post_and_two_patches() {
        let (addr, captured, server) = spawn_batch_mock_server(3);
        let report = CheckRunReport {
            name: "Canon Clerk Audit".to_string(),
            conclusion: CheckRunConclusion::Failure,
            title: "Canon Clerk Audit: FAIL".to_string(),
            summary: "Summary markdown".to_string(),
            text: Some("Detailed findings".to_string()),
            annotations: build_sample_annotations(120),
        };

        let client = GithubChecksClient::with_timeout_ms(5_000);
        let published = client
            .publish_check_run(&sample_target(addr), &report)
            .expect("publish check run");

        server.join().expect("server thread joined");

        assert_eq!(
            published,
            PublishedCheckRun {
                id: 987654,
                html_url: Some("https://github.com/PAIR-code/canon-clerk/runs/987654".to_string()),
                annotations_published: 120,
                requests_sent: 3,
            }
        );

        let reqs = captured.lock().expect("lock");
        assert_eq!(reqs.len(), 3);
        assert_create_request_headers_and_metadata(&reqs[0]);
        assert_eq!(
            [
                annotation_count(&reqs[0]),
                annotation_count(&reqs[1]),
                annotation_count(&reqs[2]),
            ],
            [50, 50, 20]
        );
        assert!(
            reqs[0].body["output"]["annotations"][0]
                .get("start_column")
                .is_none()
        );
        assert_eq!(reqs[0].body["output"]["annotations"][1]["start_column"], 2);
        assert_eq!(
            reqs[1].request_line,
            "PATCH /repos/PAIR-code/canon-clerk/check-runs/987654 HTTP/1.1"
        );
        assert_eq!(
            reqs[2].request_line,
            "PATCH /repos/PAIR-code/canon-clerk/check-runs/987654 HTTP/1.1"
        );
    }

    #[test]
    fn test_http_error_surfaces_status_and_github_message() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let _ = read_http_request(&mut stream);
            write_http_response(
                &mut stream,
                "403 Forbidden",
                r#"{"message":"Resource not accessible by integration"}"#,
            );
        });

        let report = CheckRunReport {
            name: "Canon Clerk Audit".to_string(),
            conclusion: CheckRunConclusion::Success,
            title: "Canon Clerk Audit: PASS".to_string(),
            summary: "All clear".to_string(),
            text: None,
            annotations: Vec::new(),
        };

        let client = GithubChecksClient::with_timeout_ms(5_000);
        let err = client
            .publish_check_run(&sample_target(addr), &report)
            .expect_err("403 must fail");

        server.join().expect("server joined");

        match err {
            GithubChecksError::Http {
                status, message, ..
            } => {
                assert_eq!(status, 403);
                assert!(message.contains("Resource not accessible by integration"));
            }
            other => panic!("expected Http error, got {other:?}"),
        }
    }

    #[test]
    fn test_fetch_pull_request_metadata_and_diff_via_loopback_http() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let captured = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
        let captured_server = Arc::clone(&captured);

        let server = thread::spawn(move || {
            for idx in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept request");
                let req = read_http_request(&mut stream);
                captured_server
                    .lock()
                    .unwrap()
                    .push((req.request_line, req.headers));
                if idx == 0 {
                    write_http_response(
                        &mut stream,
                        "200 OK",
                        r#"{"title":"feat: native PR","body":"Detailed body","head":{"sha":"deadbeef1234"}}"#,
                    );
                } else {
                    let diff_body = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        diff_body.len(),
                        diff_body
                    );
                    stream.write_all(response.as_bytes()).expect("write diff");
                }
            }
        });

        let target = GithubPullRequestTarget {
            api_base_url: format!("http://{addr}"),
            owner: "PAIR-code".to_string(),
            repo: "canon-clerk".to_string(),
            pull_number: 358,
            token: "ghs_test_pr_token".to_string(),
        };

        let client = GithubChecksClient::with_timeout_ms(5_000);
        let meta = client
            .fetch_pull_request_metadata(&target)
            .expect("fetch PR metadata");
        let diff = client
            .fetch_pull_request_diff(&target)
            .expect("fetch PR diff");

        server.join().expect("server joined");

        assert_eq!(meta.title.as_deref(), Some("feat: native PR"));
        assert_eq!(meta.body.as_deref(), Some("Detailed body"));
        assert_eq!(meta.head_sha.as_deref(), Some("deadbeef1234"));
        assert!(diff.contains("diff --git a/src/lib.rs b/src/lib.rs"));

        let reqs = captured.lock().unwrap();
        assert_eq!(reqs.len(), 2);
        assert_eq!(
            reqs[0].0,
            "GET /repos/PAIR-code/canon-clerk/pulls/358 HTTP/1.1"
        );
        assert!(
            reqs[0]
                .1
                .to_ascii_lowercase()
                .contains("accept: application/vnd.github+json")
        );
        assert!(
            reqs[0]
                .1
                .to_ascii_lowercase()
                .contains("authorization: bearer ghs_test_pr_token")
        );
        assert!(
            reqs[0]
                .1
                .to_ascii_lowercase()
                .contains("x-github-api-version: 2022-11-28")
        );

        assert_eq!(
            reqs[1].0,
            "GET /repos/PAIR-code/canon-clerk/pulls/358 HTTP/1.1"
        );
        assert!(
            reqs[1]
                .1
                .to_ascii_lowercase()
                .contains("accept: application/vnd.github.v3.diff")
        );
    }

    #[test]
    fn test_fetch_issue_handles_issue_pr_404_and_403() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured_server = Arc::clone(&captured);

        let server = thread::spawn(move || {
            for idx in 0..4 {
                let (mut stream, _) = listener.accept().expect("accept");
                let req = read_http_request(&mut stream);
                captured_server.lock().unwrap().push(req.request_line);

                let (status_line, body) = match idx {
                    0 => (
                        "200 OK",
                        r#"{"number":383,"title":"Ingest linked_issues","user":{"login":"jimbojw"},"labels":[{"name":"type: feat"}],"body":"Details","html_url":"https://github.com/o/r/issues/383"}"#,
                    ),
                    1 => (
                        "200 OK",
                        r#"{"number":380,"title":"A PR","pull_request":{"url":"https://api.github.com/repos/o/r/pulls/380"}}"#,
                    ),
                    2 => ("404 Not Found", r#"{"message":"Not Found"}"#),
                    _ => ("403 Forbidden", r#"{"message":"Resource not accessible"}"#),
                };
                let resp = format!(
                    "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(resp.as_bytes()).expect("write resp");
            }
        });

        let client = GithubChecksClient::with_timeout_ms(5_000);
        let target = GithubPullRequestTarget {
            api_base_url: format!("http://{addr}"),
            owner: "o".to_string(),
            repo: "r".to_string(),
            pull_number: 99,
            token: "ghs_tok".to_string(),
        };

        let issue = client
            .fetch_issue(&target, 383)
            .expect("fetch 383")
            .expect("some issue");
        assert_eq!(issue.number, 383);
        assert_eq!(issue.author.as_deref(), Some("jimbojw"));
        assert_eq!(issue.labels, vec!["type: feat"]);
        assert_eq!(
            issue.url.as_deref(),
            Some("https://github.com/o/r/issues/383")
        );

        assert_eq!(client.fetch_issue(&target, 380).expect("pr as issue"), None);
        assert_eq!(client.fetch_issue(&target, 404).expect("404 issue"), None);
        assert_eq!(client.fetch_issue(&target, 403).expect("403 issue"), None);

        server.join().expect("server thread");
        let reqs = captured.lock().unwrap();
        assert_eq!(reqs[0], "GET /repos/o/r/issues/383 HTTP/1.1");
        assert_eq!(reqs[1], "GET /repos/o/r/issues/380 HTTP/1.1");
        assert_eq!(reqs[2], "GET /repos/o/r/issues/404 HTTP/1.1");
        assert_eq!(reqs[3], "GET /repos/o/r/issues/403 HTTP/1.1");
    }
}
