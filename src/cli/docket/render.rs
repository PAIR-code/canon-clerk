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

//! Terminal Funnel Provenance receipt and `stylish`/`compact`/`json` output formatters for `canon-clerk docket`.

use super::{DocketArgs, DocketFormat};
use crate::caseload::{Caseload, CaseloadDiscovery, CaseloadDocket};
use crate::cli::CliError;
use crate::cli::provenance::{
    StageDisposition, write_configure_provenance, write_discover_provenance,
    write_docket_provenance, write_filing_intake_provenance, write_validate_provenance,
};
use std::io::Write;

/// Complete provenance watermarks across the `[intake, discover, validate, configure, docket]` closure.
#[derive(Debug, Clone, Copy)]
pub(super) struct DocketWatermarks {
    pub intake: StageDisposition,
    pub discover: StageDisposition,
    pub validate: StageDisposition,
    pub configure: StageDisposition,
    pub docket: StageDisposition,
    pub total_workspace_canons: Option<usize>,
    pub threshold: f64,
}

pub(super) fn render_docket_output(
    args: &DocketArgs,
    caseload: &Caseload,
    watermarks: &DocketWatermarks,
    stdout: &mut dyn Write,
) -> Result<(), CliError> {
    let effective_format = if args.io.json {
        DocketFormat::Json
    } else {
        args.format
    };

    match effective_format {
        DocketFormat::Json => {
            let serialized = serde_json::to_string_pretty(caseload).map_err(|err| {
                CliError::runtime(format!("Failed to serialize Caseload JSON: {err}"))
            })?;
            writeln!(stdout, "{serialized}")
                .map_err(|err| CliError::runtime(format!("Failed to write to stdout: {err}")))?;
        }
        DocketFormat::Compact => {
            if let Some(docket) = &caseload.docket {
                render_compact_output(docket, stdout).map_err(|err| {
                    CliError::runtime(format!("Failed to write compact output: {err}"))
                })?;
            }
        }
        DocketFormat::Stylish => {
            render_stylish_output(caseload, watermarks, stdout).map_err(|err| {
                CliError::runtime(format!("Failed to write stylish output: {err}"))
            })?;
        }
    }

    Ok(())
}

fn render_compact_output(docket: &CaseloadDocket, stdout: &mut dyn Write) -> std::io::Result<()> {
    for (canon_ref, assessment) in &docket.cases {
        let single_line_summary = assessment
            .colorability_summary
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(
            stdout,
            "{canon_ref}\t{}\t{:.2}\t{single_line_summary}",
            assessment.colorability_status.as_str(),
            assessment.colorability_score
        )?;
    }
    Ok(())
}

fn render_stylish_output(
    caseload: &Caseload,
    wm: &DocketWatermarks,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    writeln!(stdout, "Pipeline Provenance:")?;
    write_filing_intake_provenance(caseload.intake.as_ref(), wm.intake, stdout)?;
    write_discover_provenance(
        caseload.discovery.as_ref(),
        wm.discover,
        wm.total_workspace_canons,
        stdout,
    )?;
    write_validate_provenance(caseload.validation.as_ref(), wm.validate, stdout)?;
    write_configure_provenance(caseload, wm.configure, stdout)?;
    write_docket_provenance(caseload.docket.as_ref(), wm.docket, wm.threshold, stdout)?;
    writeln!(stdout)?;

    if let Some(docket) = &caseload.docket {
        write_active_docket_summary(docket, caseload.discovery.as_ref(), wm.threshold, stdout)?;
    }

    Ok(())
}

fn write_active_docket_summary(
    docket: &CaseloadDocket,
    discovery: Option<&CaseloadDiscovery>,
    threshold: f64,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    let docketed = docket.docketed_count();
    let dismissed = docket.dismissed_count();
    writeln!(
        stdout,
        "Active Docket ({docketed} docketed, {dismissed} dismissed):"
    )?;

    if discovery.is_some_and(|d| d.candidate_canons.is_empty()) && docket.cases.is_empty() {
        return writeln!(
            stdout,
            "  0 candidate canons matched filing; active docket empty."
        );
    }

    if docket.active_docket.is_empty() {
        return writeln!(
            stdout,
            "  0 colorable cases at threshold {threshold:.2}; active docket empty (downstream admit & audit pruned)."
        );
    }

    for canon_ref in &docket.active_docket {
        if let Some(assessment) = docket.cases.get(canon_ref) {
            writeln!(
                stdout,
                "  - {canon_ref} [score: {:.2}]",
                assessment.colorability_score
            )?;
            writeln!(stdout, "      {}", assessment.colorability_summary)?;
        }
    }
    Ok(())
}
