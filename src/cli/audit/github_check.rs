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

//! GitHub Checks API report construction and publication adapter for `canon-clerk audit`.

use super::AuditArgs;
use super::render::{AuditWatermarks, collapse_whitespace, write_github_markdown_summary};
use crate::caseload::{Caseload, CaseloadVerdict, CodeAnnotation, VerdictStatus};
use crate::cli::CliError;
use crate::github::{
    CheckRunConclusion, CheckRunPublisher, CheckRunReport, GithubCheckTarget, GithubChecksClient,
};
use std::fmt::Write as _;

pub(super) fn publish_github_check_if_enabled(
    args: &AuditArgs,
    caseload: &Caseload,
    wm: &AuditWatermarks,
    github_target: Option<&GithubCheckTarget>,
) -> Result<(), CliError> {
    let Some(target) = github_target else {
        return Ok(());
    };
    let report =
        build_audit_check_run_report(args.github_check.resolved_check_name(), caseload, wm);
    publish_check_run_report(args, target, &report)
}

pub(super) fn publish_skipped_github_check_if_enabled(
    args: &AuditArgs,
    missing_provider_slug: &str,
    github_target: Option<&GithubCheckTarget>,
) -> Result<(), CliError> {
    let Some(target) = github_target else {
        return Ok(());
    };
    let check_name = args.github_check.resolved_check_name();
    let summary = format_missing_credentials_skip_markdown(missing_provider_slug);
    let report = CheckRunReport {
        name: check_name.clone(),
        conclusion: CheckRunConclusion::Skipped,
        title: format!("{check_name}: SKIPPED"),
        summary,
        text: None,
        annotations: Vec::new(),
    };
    publish_check_run_report(args, target, &report)
}

pub(super) fn format_missing_credentials_skip_markdown(missing_provider_slug: &str) -> String {
    format!(
        "### Canon Clerk Audit Gate: SKIPPED\n\nLive audit skipped because provider API credentials for `{missing_provider_slug}` are unavailable in this execution context (`--on-missing-credentials skip`). Deterministic static canon validation passed prior to skip.\n"
    )
}

fn publish_check_run_report(
    args: &AuditArgs,
    target: &GithubCheckTarget,
    report: &CheckRunReport,
) -> Result<(), CliError> {
    let client = GithubChecksClient::new();
    client.publish_check_run(target, report).map_err(|err| {
        let replay_path = args
            .io
            .output_caseload
            .as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or("caseload.json");
        CliError::usage(format!(
            "Failed to publish GitHub Check Run `{}` to {}/{}: {err}\n  \
             Hint: Verify `GITHUB_TOKEN` / `GH_TOKEN` has `checks: write` permission and replay without LLM spend via `canon-clerk audit --caseload {replay_path} --publish-github-check`.",
            report.name, target.owner, target.repo
        ))
    })?;
    Ok(())
}

pub(super) fn build_audit_check_run_report(
    check_name: String,
    caseload: &Caseload,
    wm: &AuditWatermarks,
) -> CheckRunReport {
    let mut summary_buf = Vec::new();
    let _ = write_github_markdown_summary(caseload, wm, &mut summary_buf);
    let summary = String::from_utf8_lossy(&summary_buf).into_owned();

    if let Some(verdict) = &caseload.verdict {
        let conclusion = if verdict.status == VerdictStatus::Fail {
            CheckRunConclusion::Failure
        } else {
            CheckRunConclusion::Success
        };
        let badge = if verdict.is_pass() { "PASS" } else { "FAIL" };
        let title = format!("{check_name}: {badge} ({})", verdict.summary);
        let (annotations, text) = collect_verdict_findings(verdict);
        return CheckRunReport {
            name: check_name,
            conclusion,
            title,
            summary,
            text,
            annotations,
        };
    }

    let (annotations, text) = collect_validation_findings(caseload);
    let title = format!("{check_name}: FAIL (Candidate canon static validation failed)");
    CheckRunReport {
        name: check_name,
        conclusion: CheckRunConclusion::Failure,
        title,
        summary,
        text,
        annotations,
    }
}

fn collect_verdict_findings(verdict: &CaseloadVerdict) -> (Vec<CodeAnnotation>, Option<String>) {
    let mut annotations = Vec::new();
    let mut text = String::new();

    for adj in &verdict.adjudications {
        if adj.status == VerdictStatus::Fail && adj.annotations.is_empty() {
            let _ = writeln!(
                text,
                "- **`{}`** (`[failure]`): {}",
                adj.canon_path,
                collapse_whitespace(&adj.compliance_summary)
            );
        }
        for ann in &adj.annotations {
            let mut enriched = ann.clone();
            if enriched
                .title
                .as_deref()
                .is_none_or(|t| t.trim().is_empty())
            {
                enriched.title = Some(adj.canon_path.clone());
            }
            let _ = writeln!(
                text,
                "- **`{}`** (`{}:{}` · `{}`): {}",
                adj.canon_path,
                enriched.path,
                enriched.start_line,
                enriched.annotation_level.as_str(),
                collapse_whitespace(&enriched.message)
            );
            annotations.push(enriched);
        }
    }

    let text_opt = if text.is_empty() {
        None
    } else {
        Some(format!("### Findings\n\n{text}"))
    };
    (annotations, text_opt)
}

fn collect_validation_findings(caseload: &Caseload) -> (Vec<CodeAnnotation>, Option<String>) {
    let mut annotations = Vec::new();
    let mut text = String::new();

    if let Some(validation) = &caseload.validation {
        for (canon_id, meta) in &validation.results {
            for diag in &meta.diagnostics {
                let mut enriched = diag.clone();
                if enriched
                    .title
                    .as_deref()
                    .is_none_or(|t| t.trim().is_empty())
                {
                    enriched.title = Some(canon_id.clone());
                }
                let _ = writeln!(
                    text,
                    "- **`{canon_id}`** (`{}:{}` · `{}`): {}",
                    enriched.path,
                    enriched.start_line,
                    enriched.annotation_level.as_str(),
                    collapse_whitespace(&enriched.message)
                );
                annotations.push(enriched);
            }
        }
    }

    let text_opt = if text.is_empty() {
        None
    } else {
        Some(format!("### Static Validation Diagnostics\n\n{text}"))
    };
    (annotations, text_opt)
}
