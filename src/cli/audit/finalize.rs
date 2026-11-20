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

//! Terminal Caseload flushing, output rendering, GitHub Check publication, and exit-code finalization for `canon-clerk audit`.

use super::github_check::{
    format_missing_credentials_skip_markdown, publish_github_check_if_enabled,
    publish_skipped_github_check_if_enabled,
};
use super::render::{AuditWatermarks, render_audit_output, resolve_effective_format};
use super::stages::{AuditCascadeOutcome, AuditCascadeResult, AuditCascadeThresholds};
use super::{AuditArgs, AuditFormat};
use crate::caseload::{
    CanonAdjudication, Caseload, CaseloadDocket, CaseloadEvidence, CaseloadVerdict,
    CaseloadVetting, VerdictStatus,
};
use crate::cli::CliError;
use crate::cli::provenance::StageDisposition;
use crate::github::GithubCheckTarget;
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub(super) struct AuditOutputContext<'a> {
    pub(super) env: &'a BTreeMap<String, String>,
    pub(super) stdout: &'a mut dyn Write,
    pub(super) github_target: Option<&'a GithubCheckTarget>,
}

pub(super) fn finalize_verdict_and_exit_code(
    args: &AuditArgs,
    caseload: Caseload,
    wm: &AuditWatermarks,
    output_ctx: &mut AuditOutputContext<'_>,
) -> Result<Caseload, CliError> {
    args.io.write_output_caseload(&caseload)?;
    render_audit_output(args, &caseload, wm, output_ctx.env, output_ctx.stdout)?;
    publish_github_check_if_enabled(args, &caseload, wm, output_ctx.github_target)?;
    if caseload
        .verdict
        .as_ref()
        .is_some_and(|v| v.status == VerdictStatus::Fail)
    {
        return Err(CliError::silent(1));
    }
    Ok(caseload)
}

pub(super) fn finish_zero_candidate_audit(
    args: &AuditArgs,
    caseload_after_discover: Caseload,
    intake_disp: StageDisposition,
    total_workspace_canons: Option<usize>,
    thresholds: AuditCascadeThresholds,
    output_ctx: &mut AuditOutputContext<'_>,
) -> Result<Caseload, CliError> {
    let short_circuited = caseload_after_discover
        .with_docket(CaseloadDocket::default())
        .with_evidence(CaseloadEvidence::default())
        .with_verdict(CaseloadVerdict::default());
    args.io.write_output_caseload(&short_circuited)?;
    let wm = AuditWatermarks::zero_candidates(intake_disp, total_workspace_canons, thresholds);
    render_audit_output(
        args,
        &short_circuited,
        &wm,
        output_ctx.env,
        output_ctx.stdout,
    )?;
    publish_github_check_if_enabled(args, &short_circuited, &wm, output_ctx.github_target)?;
    Ok(short_circuited)
}

pub(super) fn handle_validation_failure(
    args: &AuditArgs,
    caseload: &Caseload,
    intake_disp: StageDisposition,
    discover_disp: StageDisposition,
    total_workspace_canons: Option<usize>,
    thresholds: AuditCascadeThresholds,
    output_ctx: &mut AuditOutputContext<'_>,
) -> Result<Caseload, CliError> {
    args.io.write_output_caseload(caseload)?;
    let wm = AuditWatermarks::validation_failed(
        intake_disp,
        discover_disp,
        total_workspace_canons,
        thresholds,
    );
    if resolve_effective_format(args, output_ctx.env) != AuditFormat::Compact {
        render_audit_output(args, caseload, &wm, output_ctx.env, output_ctx.stdout)?;
    }
    publish_github_check_if_enabled(args, caseload, &wm, output_ctx.github_target)?;

    let total_errors: usize = caseload
        .validation
        .as_ref()
        .map_or(1, |v| v.results.values().map(|m| m.error_count).sum());
    Err(CliError::runtime(format!(
        "Candidate canon validation failed with {total_errors} error(s); aborting `audit` before model spend.\n  \
         Hint: Run `canon-clerk validate` to inspect and repair malformed candidate canons."
    )))
}

pub(super) fn finalize_cascade_result(
    args: &AuditArgs,
    result: AuditCascadeResult,
    branch_a: (StageDisposition, StageDisposition, StageDisposition),
    total_workspace_canons: Option<usize>,
    thresholds: AuditCascadeThresholds,
    output_ctx: &mut AuditOutputContext<'_>,
) -> Result<Caseload, CliError> {
    match result {
        AuditCascadeResult::SkippedMissingCredentials {
            caseload,
            missing_provider_slug,
        } => finish_skipped_missing_credentials(args, caseload, &missing_provider_slug, output_ctx),
        AuditCascadeResult::Completed(outcome) => {
            let (intake, discover, validate) = branch_a;
            let wm = AuditWatermarks {
                intake,
                discover,
                validate,
                configure: outcome.configure_disp,
                docket: outcome.docket_disp,
                admit: outcome.admit_disp,
                vet: outcome.vet_disp,
                audit: outcome.audit_disp,
                total_workspace_canons,
                docket_threshold: thresholds.docket_threshold,
                admit_threshold: thresholds.admit_threshold,
                vet_threshold: thresholds.vet_threshold,
                audit_threshold: thresholds.audit_threshold,
            };
            finalize_verdict_and_exit_code(args, outcome.caseload, &wm, output_ctx)
        }
    }
}

pub(super) fn finish_skipped_missing_credentials(
    args: &AuditArgs,
    caseload: Caseload,
    missing_provider_slug: &str,
    output_ctx: &mut AuditOutputContext<'_>,
) -> Result<Caseload, CliError> {
    args.io.write_output_caseload(&caseload)?;

    match resolve_effective_format(args, output_ctx.env) {
        AuditFormat::Json => {
            let serialized = serde_json::to_string_pretty(&caseload).map_err(|err| {
                CliError::runtime(format!("Failed to serialize Caseload JSON: {err}"))
            })?;
            writeln!(output_ctx.stdout, "{serialized}")
                .map_err(|err| CliError::runtime(format!("Failed to write to stdout: {err}")))?;
        }
        AuditFormat::Compact => {}
        AuditFormat::Stylish | AuditFormat::Github => {
            writeln!(
                output_ctx.stdout,
                "Canon Clerk Audit: SKIPPED (provider API credentials for `{missing_provider_slug}` are unavailable; --on-missing-credentials skip)"
            )
            .map_err(|err| CliError::runtime(format!("Failed to write to stdout: {err}")))?;
        }
    }

    if let Some(summary_path) = output_ctx
        .env
        .get("GITHUB_STEP_SUMMARY")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        && let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(Path::new(summary_path))
    {
        let _ = file
            .write_all(format_missing_credentials_skip_markdown(missing_provider_slug).as_bytes());
    }

    publish_skipped_github_check_if_enabled(args, missing_provider_slug, output_ctx.github_target)?;
    Ok(caseload)
}

pub(super) fn fold_vetted_to_verdict(
    vetting: &CaseloadVetting,
    audit_threshold: f64,
) -> CaseloadVerdict {
    let adjudications: Vec<CanonAdjudication> = vetting
        .vetted_cases
        .iter()
        .filter_map(|canon_ref| {
            vetting
                .cases
                .get(canon_ref)
                .map(|assessment| CanonAdjudication {
                    canon_path: canon_ref.clone(),
                    compliance_summary: assessment.vetting_summary.clone(),
                    compliance_score: assessment.vetting_score,
                    status: VerdictStatus::from_score(assessment.vetting_score, audit_threshold),
                    annotations: Vec::new(),
                })
        })
        .collect();
    CaseloadVerdict::from_adjudications(adjudications)
}

pub(super) fn try_pre_populated_short_circuit(
    caseload: &Caseload,
    docket_pre_populated: bool,
    evidence_pre_populated: bool,
    vetting_pre_populated: bool,
    audit_threshold: f64,
) -> Option<AuditCascadeOutcome> {
    if vetting_pre_populated
        && let Some(vetting) = caseload.vetting.as_ref()
        && vetting.is_all_vetted()
    {
        let cfg_disp = if caseload.config.is_some() {
            StageDisposition::PrePopulated
        } else {
            StageDisposition::SkippedAllVetted
        };
        let doc_disp = if caseload.docket.is_some() {
            StageDisposition::PrePopulated
        } else {
            StageDisposition::SkippedAllVetted
        };
        let adm_disp = if caseload.evidence.is_some() {
            StageDisposition::PrePopulated
        } else {
            StageDisposition::SkippedAllVetted
        };
        return Some(AuditCascadeOutcome {
            caseload: caseload
                .clone()
                .with_verdict(fold_vetted_to_verdict(vetting, audit_threshold)),
            configure_disp: cfg_disp,
            docket_disp: doc_disp,
            admit_disp: adm_disp,
            vet_disp: StageDisposition::PrePopulated,
            audit_disp: StageDisposition::SkippedAllVetted,
        });
    }

    if evidence_pre_populated
        && caseload
            .evidence
            .as_ref()
            .is_some_and(|ev| ev.active_cases.is_empty())
    {
        let cfg_disp = if caseload.config.is_some() {
            StageDisposition::PrePopulated
        } else {
            StageDisposition::SkippedZeroEvidence
        };
        let doc_disp = if caseload.docket.is_some() {
            StageDisposition::PrePopulated
        } else {
            StageDisposition::SkippedZeroEvidence
        };
        return Some(AuditCascadeOutcome {
            caseload: caseload.clone().with_verdict(CaseloadVerdict::default()),
            configure_disp: cfg_disp,
            docket_disp: doc_disp,
            admit_disp: StageDisposition::PrePopulated,
            vet_disp: StageDisposition::SkippedZeroEvidence,
            audit_disp: StageDisposition::SkippedZeroEvidence,
        });
    }

    if !evidence_pre_populated
        && docket_pre_populated
        && caseload
            .docket
            .as_ref()
            .is_some_and(|d| d.active_docket.is_empty())
    {
        let cfg_disp = if caseload.config.is_some() {
            StageDisposition::PrePopulated
        } else {
            StageDisposition::SkippedEmptyDocket
        };
        return Some(AuditCascadeOutcome {
            caseload: caseload
                .clone()
                .with_evidence(CaseloadEvidence::default())
                .with_verdict(CaseloadVerdict::default()),
            configure_disp: cfg_disp,
            docket_disp: StageDisposition::PrePopulated,
            admit_disp: StageDisposition::SkippedEmptyDocket,
            vet_disp: StageDisposition::SkippedEmptyDocket,
            audit_disp: StageDisposition::SkippedEmptyDocket,
        });
    }

    None
}
