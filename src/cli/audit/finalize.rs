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

use super::github_check::publish_github_check_if_enabled;
use super::render::{AuditWatermarks, render_audit_output, resolve_effective_format};
use super::stages::AuditCascadeThresholds;
use super::{AuditArgs, AuditFormat};
use crate::caseload::{Caseload, CaseloadDocket, CaseloadEvidence, CaseloadVerdict, VerdictStatus};
use crate::cli::CliError;
use crate::cli::provenance::StageDisposition;
use crate::github::GithubCheckTarget;
use std::collections::BTreeMap;
use std::io::Write;

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
