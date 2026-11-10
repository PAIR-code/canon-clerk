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

//! Terminal Funnel Provenance receipt and `stylish`/`compact`/`json` output formatters for `canon-clerk apprise`.

use super::{AppriseArgs, AppriseFormat};
use crate::caseload::{ApprisalStatus, Caseload, CaseloadApprisal, CaseloadDiscovery};
use crate::cli::CliError;
use crate::cli::provenance::{
    StageDisposition, fast_forward_suffix, glyph_for, write_configure_provenance,
    write_discover_provenance, write_intent_intake_provenance, write_validate_provenance,
};
use std::io::Write;

/// Complete provenance watermarks across the `[intake, discover, validate, configure, apprise]` closure.
#[derive(Debug, Clone, Copy)]
pub(super) struct AppriseWatermarks {
    pub intake: StageDisposition,
    pub discover: StageDisposition,
    pub validate: StageDisposition,
    pub configure: StageDisposition,
    pub apprise: StageDisposition,
    pub total_workspace_canons: Option<usize>,
    pub threshold: f64,
}

pub(super) fn render_apprise_output(
    args: &AppriseArgs,
    caseload: &Caseload,
    watermarks: &AppriseWatermarks,
    stdout: &mut dyn Write,
) -> Result<(), CliError> {
    let effective_format = if args.io.json {
        AppriseFormat::Json
    } else {
        args.format
    };

    match effective_format {
        AppriseFormat::Json => {
            let serialized = serde_json::to_string_pretty(caseload).map_err(|err| {
                CliError::runtime(format!("Failed to serialize Caseload JSON: {err}"))
            })?;
            writeln!(stdout, "{serialized}")
                .map_err(|err| CliError::runtime(format!("Failed to write to stdout: {err}")))?;
        }
        AppriseFormat::Compact => {
            if let Some(apprisal) = &caseload.apprisal {
                render_compact_output(apprisal, stdout).map_err(|err| {
                    CliError::runtime(format!("Failed to write compact output: {err}"))
                })?;
            }
        }
        AppriseFormat::Stylish => {
            render_stylish_output(caseload, watermarks, stdout).map_err(|err| {
                CliError::runtime(format!("Failed to write stylish output: {err}"))
            })?;
        }
    }

    Ok(())
}

fn render_compact_output(
    apprisal: &CaseloadApprisal,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    for (canon_ref, assessment) in &apprisal.assessments {
        let single_line_summary = assessment
            .apprisal_summary
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(
            stdout,
            "{canon_ref}\t{}\t{:.2}\t{single_line_summary}",
            assessment.apprisal_status.as_str(),
            assessment.apprisal_score
        )?;
    }
    Ok(())
}

fn render_stylish_output(
    caseload: &Caseload,
    wm: &AppriseWatermarks,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    writeln!(stdout, "Pipeline Provenance:")?;
    write_intent_intake_provenance(caseload.intake.as_ref(), wm.intake, stdout)?;
    write_discover_provenance(
        caseload.discovery.as_ref(),
        wm.discover,
        wm.total_workspace_canons,
        stdout,
    )?;
    write_validate_provenance(caseload.validation.as_ref(), wm.validate, stdout)?;
    write_configure_provenance(caseload, wm.configure, stdout)?;
    write_apprise_provenance(caseload.apprisal.as_ref(), wm.apprise, wm.threshold, stdout)?;
    writeln!(stdout)?;

    if let Some(apprisal) = &caseload.apprisal {
        write_apprisal_summary(apprisal, caseload.discovery.as_ref(), wm.threshold, stdout)?;
    }

    Ok(())
}

fn write_apprise_provenance(
    apprisal: Option<&CaseloadApprisal>,
    disp: StageDisposition,
    threshold: f64,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    match disp {
        StageDisposition::SkippedZeroCandidates => {
            writeln!(
                stdout,
                "  ⊘ apprise   : (fast-forward: zero candidate canons)"
            )
        }
        StageDisposition::SkippedValidationFailed => {
            writeln!(
                stdout,
                "  ⊘ apprise   : (fast-forward: canon validation failed)"
            )
        }
        _ => {
            let glyph = glyph_for(disp);
            let suffix = fast_forward_suffix(disp);
            let (applicable, dismissed) =
                apprisal.map_or((0, 0), |a| (a.applicable_count(), a.dismissed_count()));
            writeln!(
                stdout,
                "  {glyph} apprise   : {applicable} canon(s) applicable ({dismissed} dismissed, threshold {threshold:.2}){suffix}"
            )
        }
    }
}

fn write_apprisal_summary(
    apprisal: &CaseloadApprisal,
    discovery: Option<&CaseloadDiscovery>,
    threshold: f64,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    let applicable = apprisal.applicable_count();
    let dismissed = apprisal.dismissed_count();
    writeln!(
        stdout,
        "Statutory Apprisal ({applicable} applicable, {dismissed} dismissed):"
    )?;

    if discovery.is_some_and(|d| d.candidate_canons.is_empty()) && apprisal.assessments.is_empty() {
        return writeln!(
            stdout,
            "  0 candidate canons governing target scope; no applicable statutory constraints."
        );
    }

    if applicable == 0 {
        return writeln!(
            stdout,
            "  No candidate canons claimed prospective jurisdiction at threshold {threshold:.2}."
        );
    }

    for (canon_ref, assessment) in &apprisal.assessments {
        if assessment.apprisal_status == ApprisalStatus::Applicable {
            writeln!(
                stdout,
                "  - {canon_ref} [score: {:.2}]",
                assessment.apprisal_score
            )?;
            writeln!(stdout, "      {}", assessment.apprisal_summary)?;
        }
    }
    Ok(())
}
