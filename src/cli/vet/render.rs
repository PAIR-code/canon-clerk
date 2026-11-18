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

//! Terminal 7-stage Funnel Provenance receipt and `stylish`/`compact`/`json` output formatters for `canon-clerk vet`.

use super::{VetArgs, VetFormat};
use crate::caseload::{
    Caseload, CaseloadDiscovery, CaseloadDocket, CaseloadEvidence, CaseloadVetting, HeuristicNode,
    VettingStatus,
};
use crate::cli::CliError;
use crate::cli::provenance::{
    StageDisposition, write_admit_provenance, write_closure_configure_provenance,
    write_discover_provenance, write_docket_provenance, write_filing_intake_provenance,
    write_validate_provenance, write_vet_provenance,
};
use crate::cli::telemetry::resolve_verbose_flag;
use std::collections::BTreeMap;
use std::io::Write;

/// Complete provenance watermarks across the 7-stage `[intake, discover, validate, configure, docket, admit, vet]` closure.
#[derive(Debug, Clone, Copy)]
pub(super) struct VetWatermarks {
    pub intake: StageDisposition,
    pub discover: StageDisposition,
    pub validate: StageDisposition,
    pub configure: StageDisposition,
    pub docket: StageDisposition,
    pub admit: StageDisposition,
    pub vet: StageDisposition,
    pub total_workspace_canons: Option<usize>,
    pub docket_threshold: f64,
    pub admit_threshold: f64,
    pub vet_threshold: f64,
}

impl VetWatermarks {
    pub(super) const fn pre_populated(
        docket_threshold: f64,
        admit_threshold: f64,
        vet_threshold: f64,
    ) -> Self {
        Self {
            intake: StageDisposition::PrePopulated,
            discover: StageDisposition::PrePopulated,
            validate: StageDisposition::PrePopulated,
            configure: StageDisposition::PrePopulated,
            docket: StageDisposition::PrePopulated,
            admit: StageDisposition::PrePopulated,
            vet: StageDisposition::PrePopulated,
            total_workspace_canons: None,
            docket_threshold,
            admit_threshold,
            vet_threshold,
        }
    }
}

pub(super) fn render_vet_output(
    args: &VetArgs,
    caseload: &Caseload,
    watermarks: &VetWatermarks,
    env: &BTreeMap<String, String>,
    stdout: &mut dyn Write,
) -> Result<(), CliError> {
    let verbose = resolve_verbose_flag(args.verbose, env)?;
    let effective_format = if args.io.json {
        VetFormat::Json
    } else {
        args.format
    };

    match effective_format {
        VetFormat::Json => {
            let serialized = serde_json::to_string_pretty(caseload).map_err(|err| {
                CliError::runtime(format!("Failed to serialize Caseload JSON: {err}"))
            })?;
            writeln!(stdout, "{serialized}")
                .map_err(|err| CliError::runtime(format!("Failed to write to stdout: {err}")))?;
        }
        VetFormat::Compact => {
            if let Some(vetting) = &caseload.vetting {
                render_compact_output(vetting, stdout).map_err(|err| {
                    CliError::runtime(format!("Failed to write compact output: {err}"))
                })?;
            }
        }
        VetFormat::Stylish => {
            render_stylish_output(caseload, watermarks, verbose, stdout).map_err(|err| {
                CliError::runtime(format!("Failed to write stylish output: {err}"))
            })?;
        }
    }

    Ok(())
}

fn render_compact_output(vetting: &CaseloadVetting, stdout: &mut dyn Write) -> std::io::Result<()> {
    for (canon_ref, assessment) in &vetting.cases {
        if assessment.vetting_status == VettingStatus::Vetted {
            continue;
        }
        let single_line = assessment
            .vetting_summary
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(
            stdout,
            "{canon_ref}\t{}\t{:.2}\t{single_line}",
            assessment.vetting_status.as_str(),
            assessment.vetting_score
        )?;
    }
    Ok(())
}

fn render_stylish_output(
    caseload: &Caseload,
    wm: &VetWatermarks,
    verbose: bool,
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
    write_closure_configure_provenance(
        caseload,
        &[
            HeuristicNode::Docket,
            HeuristicNode::Admit,
            HeuristicNode::Vet,
        ],
        wm.configure,
        stdout,
    )?;
    write_docket_provenance(
        caseload.docket.as_ref(),
        wm.docket,
        wm.docket_threshold,
        stdout,
    )?;
    write_admit_provenance(
        caseload.evidence.as_ref(),
        wm.admit,
        wm.admit_threshold,
        stdout,
    )?;
    write_vet_provenance(
        caseload.vetting.as_ref(),
        caseload.evidence.as_ref(),
        wm.vet,
        wm.vet_threshold,
        stdout,
    )?;
    writeln!(stdout)?;

    if let Some(vetting) = &caseload.vetting {
        write_vetting_summary(
            vetting,
            caseload.discovery.as_ref(),
            caseload.docket.as_ref(),
            caseload.evidence.as_ref(),
            wm.vet_threshold,
            verbose,
            stdout,
        )?;
    }

    Ok(())
}

fn write_vetting_summary(
    vetting: &CaseloadVetting,
    discovery: Option<&CaseloadDiscovery>,
    docket: Option<&CaseloadDocket>,
    evidence: Option<&CaseloadEvidence>,
    threshold: f64,
    verbose: bool,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    let vetted = vetting.vetted_count();
    let contested = vetting.contested_count();
    let banner = if contested == 0 { "PASS" } else { "CONTESTED" };

    if discovery.is_some_and(|d| d.candidate_canons.is_empty()) && vetting.cases.is_empty() {
        return writeln!(
            stdout,
            "Vetting: {banner} (0 candidate canons matched filing; 0 cases vetted)"
        );
    }
    if docket.is_some_and(|d| d.active_docket.is_empty()) && vetting.cases.is_empty() {
        return writeln!(
            stdout,
            "Vetting: {banner} (0 active cases on docket; 0 cases vetted)"
        );
    }
    if evidence.is_some_and(|ev| ev.active_cases.is_empty()) && vetting.cases.is_empty() {
        return writeln!(
            stdout,
            "Vetting: {banner} (0 admissible exhibits; 0 cases vetted)"
        );
    }

    writeln!(
        stdout,
        "Vetting: {banner} ({vetted} vetted, {contested} contested at threshold {threshold:.2})"
    )?;
    for (canon_ref, item) in &vetting.cases {
        if !verbose && item.vetting_status == VettingStatus::Vetted {
            continue;
        }
        writeln!(
            stdout,
            "  - {canon_ref} [{}] (score: {:.2})",
            item.vetting_status.as_str(),
            item.vetting_score
        )?;
        writeln!(stdout, "      {}", item.vetting_summary)?;
    }
    if contested > 0 {
        writeln!(
            stdout,
            "  Hint: Run `canon-clerk audit` to adjudicate contested case(s) with exception evaluation and line annotations."
        )?;
    }
    Ok(())
}
