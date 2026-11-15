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

//! Terminal 8-stage Funnel Provenance receipt and `stylish`/`compact`/`json`/`github` output formatters for `canon-clerk audit`.

use super::stages::AuditCascadeThresholds;
use super::{AuditArgs, AuditFormat};
use crate::caseload::{
    AnnotationLevel, CanonAdjudication, Caseload, CaseloadVerdict, CodeAnnotation, VerdictStatus,
};
use crate::cli::CliError;
use crate::cli::provenance::{
    StageDisposition, write_admit_provenance, write_audit_configure_provenance,
    write_audit_provenance, write_discover_provenance, write_docket_provenance,
    write_filing_intake_provenance, write_validate_provenance, write_vet_provenance,
};
use crate::cli::telemetry::resolve_verbose_flag;
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

/// Complete provenance watermarks across the 8-stage `[intake, discover, validate, configure, docket, admit, vet, audit]` closure.
#[derive(Debug, Clone, Copy)]
pub(super) struct AuditWatermarks {
    pub intake: StageDisposition,
    pub discover: StageDisposition,
    pub validate: StageDisposition,
    pub configure: StageDisposition,
    pub docket: StageDisposition,
    pub admit: StageDisposition,
    pub vet: StageDisposition,
    pub audit: StageDisposition,
    pub total_workspace_canons: Option<usize>,
    pub docket_threshold: f64,
    pub admit_threshold: f64,
    pub vet_threshold: f64,
    pub audit_threshold: f64,
}

impl AuditWatermarks {
    pub(super) const fn pre_populated(
        docket_threshold: f64,
        admit_threshold: f64,
        vet_threshold: f64,
        audit_threshold: f64,
        vet_was_present: bool,
    ) -> Self {
        Self {
            intake: StageDisposition::PrePopulated,
            discover: StageDisposition::PrePopulated,
            validate: StageDisposition::PrePopulated,
            configure: StageDisposition::PrePopulated,
            docket: StageDisposition::PrePopulated,
            admit: StageDisposition::PrePopulated,
            vet: if vet_was_present {
                StageDisposition::PrePopulated
            } else {
                StageDisposition::SkippedGenerativeScreener
            },
            audit: StageDisposition::PrePopulated,
            total_workspace_canons: None,
            docket_threshold,
            admit_threshold,
            vet_threshold,
            audit_threshold,
        }
    }

    pub(super) const fn zero_candidates(
        intake: StageDisposition,
        total_workspace_canons: Option<usize>,
        thresholds: AuditCascadeThresholds,
    ) -> Self {
        Self {
            intake,
            discover: StageDisposition::ZeroCandidateShortCircuit,
            validate: StageDisposition::SkippedZeroCandidates,
            configure: StageDisposition::SkippedZeroCandidates,
            docket: StageDisposition::SkippedZeroCandidates,
            admit: StageDisposition::SkippedZeroCandidates,
            vet: StageDisposition::SkippedZeroCandidates,
            audit: StageDisposition::SkippedZeroCandidates,
            total_workspace_canons,
            docket_threshold: thresholds.docket_threshold,
            admit_threshold: thresholds.admit_threshold,
            vet_threshold: thresholds.vet_threshold,
            audit_threshold: thresholds.audit_threshold,
        }
    }

    pub(super) const fn validation_failed(
        intake: StageDisposition,
        discover: StageDisposition,
        total_workspace_canons: Option<usize>,
        thresholds: AuditCascadeThresholds,
    ) -> Self {
        Self {
            intake,
            discover,
            validate: StageDisposition::ValidationFailed,
            configure: StageDisposition::SkippedValidationFailed,
            docket: StageDisposition::SkippedValidationFailed,
            admit: StageDisposition::SkippedValidationFailed,
            vet: StageDisposition::SkippedValidationFailed,
            audit: StageDisposition::SkippedValidationFailed,
            total_workspace_canons,
            docket_threshold: thresholds.docket_threshold,
            admit_threshold: thresholds.admit_threshold,
            vet_threshold: thresholds.vet_threshold,
            audit_threshold: thresholds.audit_threshold,
        }
    }
}

pub(super) fn resolve_effective_format(
    args: &AuditArgs,
    env: &BTreeMap<String, String>,
) -> AuditFormat {
    if args.io.json {
        AuditFormat::Json
    } else if let Some(explicit) = args.format {
        explicit
    } else if env.get("GITHUB_ACTIONS").map(String::as_str) == Some("true") {
        AuditFormat::Github
    } else {
        AuditFormat::Stylish
    }
}

pub(super) fn render_audit_output(
    args: &AuditArgs,
    caseload: &Caseload,
    watermarks: &AuditWatermarks,
    env: &BTreeMap<String, String>,
    stdout: &mut dyn Write,
) -> Result<(), CliError> {
    let verbose = resolve_verbose_flag(args.verbose, env)?;
    let effective_format = resolve_effective_format(args, env);

    match effective_format {
        AuditFormat::Json => {
            let serialized = serde_json::to_string_pretty(caseload).map_err(|err| {
                CliError::runtime(format!("Failed to serialize Caseload JSON: {err}"))
            })?;
            writeln!(stdout, "{serialized}")
                .map_err(|err| CliError::runtime(format!("Failed to write to stdout: {err}")))?;
        }
        AuditFormat::Compact => {
            if let Some(verdict) = &caseload.verdict {
                render_compact_output(verdict, stdout).map_err(|err| {
                    CliError::runtime(format!("Failed to write compact output: {err}"))
                })?;
            }
        }
        AuditFormat::Github => {
            if let Some(verdict) = &caseload.verdict {
                render_github_annotations(verdict, stdout).map_err(|err| {
                    CliError::runtime(format!("Failed to write GitHub annotations: {err}"))
                })?;
            }
            if let Some(summary_path) = env
                .get("GITHUB_STEP_SUMMARY")
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                let _ = append_github_step_summary(Path::new(summary_path), caseload, watermarks);
            }
        }
        AuditFormat::Stylish => {
            render_stylish_output(caseload, watermarks, verbose, stdout).map_err(|err| {
                CliError::runtime(format!("Failed to write stylish output: {err}"))
            })?;
        }
    }

    Ok(())
}

fn render_compact_output(verdict: &CaseloadVerdict, stdout: &mut dyn Write) -> std::io::Result<()> {
    for adj in &verdict.adjudications {
        if adj.status == VerdictStatus::Pass {
            continue;
        }
        if adj.annotations.is_empty() {
            let summary = collapse_whitespace(&adj.compliance_summary);
            writeln!(
                stdout,
                "{}\t0:0\tfailure\t{}\t{summary}",
                adj.canon_path, adj.canon_path
            )?;
        } else {
            for ann in &adj.annotations {
                let col = ann.start_column.unwrap_or(1);
                let msg = collapse_whitespace(&ann.message);
                writeln!(
                    stdout,
                    "{}\t{}:{col}\t{}\t{}\t{msg}",
                    ann.path,
                    ann.start_line,
                    ann.annotation_level.as_str(),
                    adj.canon_path
                )?;
            }
        }
    }
    Ok(())
}

fn render_github_annotations(
    verdict: &CaseloadVerdict,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    for adj in &verdict.adjudications {
        for ann in &adj.annotations {
            writeln!(
                stdout,
                "{}",
                format_github_workflow_annotation(ann, &adj.canon_path)
            )?;
        }
    }
    Ok(())
}

pub(crate) fn format_github_workflow_annotation(
    ann: &CodeAnnotation,
    default_title: &str,
) -> String {
    let command = match ann.annotation_level {
        AnnotationLevel::Failure => "error",
        AnnotationLevel::Warning => "warning",
        AnnotationLevel::Notice => "notice",
    };
    let mut props = format!(
        "file={},line={},endLine={}",
        escape_workflow_property(&ann.path),
        ann.start_line,
        ann.end_line
    );
    if let Some(sc) = ann.start_column {
        props.push_str(&format!(",col={sc}"));
    }
    if let Some(ec) = ann.end_column {
        props.push_str(&format!(",endColumn={ec}"));
    }
    let title = ann.title.as_deref().unwrap_or(default_title);
    props.push_str(&format!(",title={}", escape_workflow_property(title)));

    let escaped_msg = escape_workflow_data(&ann.message);
    format!("::{command} {props}::{escaped_msg}")
}

fn escape_workflow_data(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn escape_workflow_property(text: &str) -> String {
    escape_workflow_data(text)
        .replace(':', "%3A")
        .replace(',', "%2C")
}

fn append_github_step_summary(
    summary_path: &Path,
    caseload: &Caseload,
    wm: &AuditWatermarks,
) -> std::io::Result<()> {
    if let Some(parent) = summary_path.parent()
        && !parent.as_os_str().is_empty()
    {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(summary_path)?;

    let mut provenance_buf = Vec::new();
    write_seven_stage_provenance(caseload, wm, &mut provenance_buf)?;
    let provenance_text = String::from_utf8_lossy(&provenance_buf);

    let (status_badge, summary_line) = match &caseload.verdict {
        Some(v) if v.status == VerdictStatus::Fail => ("FAIL", v.summary.as_str()),
        Some(v) => ("PASS", v.summary.as_str()),
        None => (
            "FAIL",
            "Candidate canon static validation failed prior to audit.",
        ),
    };

    writeln!(file, "### Canon Clerk Audit Gate: {status_badge}\n")?;
    writeln!(file, "{summary_line}\n")?;
    writeln!(file, "```text\n{provenance_text}```\n")?;

    if let Some(verdict) = &caseload.verdict
        && !verdict.adjudications.is_empty()
    {
        let (failing, passing): (Vec<&CanonAdjudication>, Vec<&CanonAdjudication>) = verdict
            .adjudications
            .iter()
            .partition(|adj| adj.status == VerdictStatus::Fail);

        if !failing.is_empty() {
            write_markdown_decree_table(&failing, &mut file)?;
            if !passing.is_empty() {
                writeln!(file)?;
            }
        }
        if !passing.is_empty() {
            writeln!(
                file,
                "<details>\n<summary>Passing cases ({})</summary>\n",
                passing.len()
            )?;
            write_markdown_decree_table(&passing, &mut file)?;
            writeln!(file, "\n</details>")?;
        }
    }
    Ok(())
}

fn write_markdown_decree_table(
    adjudications: &[&CanonAdjudication],
    out: &mut dyn Write,
) -> std::io::Result<()> {
    writeln!(out, "| Case (`canon_path`) | Status | Score | Decree |")?;
    writeln!(out, "| :--- | :--- | ---: | :--- |")?;
    for adj in adjudications {
        let decree = collapse_whitespace(&adj.compliance_summary).replace('|', "\\|");
        writeln!(
            out,
            "| `{}` | `{}` | `{:.2}` | {} |",
            adj.canon_path,
            adj.status.as_str(),
            adj.compliance_score,
            decree
        )?;
    }
    Ok(())
}

fn write_seven_stage_provenance(
    caseload: &Caseload,
    wm: &AuditWatermarks,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    writeln!(out, "Pipeline Provenance:")?;
    write_filing_intake_provenance(caseload.intake.as_ref(), wm.intake, out)?;
    write_discover_provenance(
        caseload.discovery.as_ref(),
        wm.discover,
        wm.total_workspace_canons,
        out,
    )?;
    write_validate_provenance(caseload.validation.as_ref(), wm.validate, out)?;
    write_audit_configure_provenance(caseload, wm.configure, out)?;
    write_docket_provenance(
        caseload.docket.as_ref(),
        wm.docket,
        wm.docket_threshold,
        out,
    )?;
    write_admit_provenance(
        caseload.evidence.as_ref(),
        wm.admit,
        wm.admit_threshold,
        out,
    )?;
    write_vet_provenance(
        caseload.vetting.as_ref(),
        caseload.evidence.as_ref(),
        wm.vet,
        wm.vet_threshold,
        out,
    )?;
    write_audit_provenance(
        caseload.verdict.as_ref(),
        caseload.vetting.as_ref(),
        wm.audit,
        wm.audit_threshold,
        out,
    )?;
    Ok(())
}

fn render_stylish_output(
    caseload: &Caseload,
    wm: &AuditWatermarks,
    verbose: bool,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    write_seven_stage_provenance(caseload, wm, stdout)?;
    writeln!(stdout)?;

    if let Some(verdict) = &caseload.verdict {
        write_verdict_report(verdict, verbose, stdout)?;
    }

    Ok(())
}

fn write_verdict_report(
    verdict: &CaseloadVerdict,
    verbose: bool,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    let banner = if verdict.is_pass() { "PASS" } else { "FAIL" };
    writeln!(stdout, "Verdict: {banner} ({})", verdict.summary)?;

    for adj in &verdict.adjudications {
        if !verbose && adj.status == VerdictStatus::Pass {
            continue;
        }
        writeln!(
            stdout,
            "  - {} [{}] (score: {:.2})",
            adj.canon_path,
            adj.status.as_str(),
            adj.compliance_score
        )?;
        writeln!(stdout, "      {}", adj.compliance_summary)?;
        for ann in &adj.annotations {
            let col = ann.start_column.unwrap_or(1);
            let title = ann.title.as_deref().unwrap_or(adj.canon_path.as_str());
            writeln!(
                stdout,
                "      -> {}:{}:{col} [{}] ({title}): {}",
                ann.path,
                ann.start_line,
                ann.annotation_level.as_str(),
                ann.message
            )?;
        }
    }
    Ok(())
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
