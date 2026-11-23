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

//! Shared Pipeline Funnel Provenance watermarks and receipt line writers (`docs/architecture/scheduling.md` §6).

use crate::caseload::{
    Caseload, CaseloadConfig, CaseloadDiscovery, CaseloadDocket, CaseloadEvidence, CaseloadIntake,
    CaseloadValidation, CaseloadVerdict, CaseloadVetting, HeuristicNode, IntakeScope,
};
use std::io::Write;

/// Per-stage execution watermark recorded for the Pipeline Funnel Provenance receipt (`docs/architecture/scheduling.md` §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageDisposition {
    /// Stage executed during the current CLI invocation.
    Executed,
    /// Stage was already populated on `--caseload` and fast-forwarded without re-execution.
    PrePopulated,
    /// `discover` executed and matched zero candidate canons, triggering an early clean pass.
    ZeroCandidateShortCircuit,
    /// Downstream stage skipped because upstream `discover` matched zero candidate canons.
    SkippedZeroCandidates,
    /// `validate` executed and found one or more candidate canon lint errors (`exit 1`).
    ValidationFailed,
    /// Downstream stage skipped because upstream `validate` failed (`exit 1`).
    SkippedValidationFailed,
    /// Downstream stage skipped because upstream `docket` opened zero active cases (`active_docket` empty).
    SkippedEmptyDocket,
    /// Downstream stage skipped because upstream `admit` admitted zero exhibits (`active_cases` empty).
    SkippedZeroEvidence,
    /// `vet` was absent on a pre-populated `.verdict` Caseload replay.
    SkippedAbsentVetting,
    /// Downstream `audit` skipped because upstream `vet` cleared all active cases (`contested_cases` empty).
    SkippedAllVetted,
}

pub(crate) fn glyph_for(disp: StageDisposition) -> &'static str {
    match disp {
        StageDisposition::Executed => "✓",
        StageDisposition::PrePopulated => "↷",
        StageDisposition::ZeroCandidateShortCircuit
        | StageDisposition::SkippedZeroCandidates
        | StageDisposition::SkippedValidationFailed
        | StageDisposition::SkippedEmptyDocket
        | StageDisposition::SkippedZeroEvidence
        | StageDisposition::SkippedAbsentVetting
        | StageDisposition::SkippedAllVetted => "⊘",
        StageDisposition::ValidationFailed => "✗",
    }
}

pub(crate) fn fast_forward_suffix(disp: StageDisposition) -> &'static str {
    match disp {
        StageDisposition::PrePopulated => " (fast-forward: pre-populated)",
        _ => "",
    }
}

/// Writes the `intake` provenance line for prospective statutory apprisal (`apprise`).
pub(crate) fn write_intent_intake_provenance(
    intake: Option<&CaseloadIntake>,
    disp: StageDisposition,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    let suffix = fast_forward_suffix(disp);
    let glyph = glyph_for(disp);
    let detail = match intake {
        Some(inc) if inc.scope == IntakeScope::AllTargets => {
            "design intent + all repository targets".to_string()
        }
        Some(inc) if !inc.diffs.is_empty() => {
            format!("design intent + {} modified file(s)", inc.diffs.len())
        }
        Some(inc) if !inc.target_paths.is_empty() => format!(
            "design intent + {} target directive(s) ({})",
            inc.target_paths.len(),
            inc.target_paths.join(", ")
        ),
        Some(_) => "prospective design intent ingested".to_string(),
        None => "(not populated)".to_string(),
    };
    writeln!(stdout, "  {glyph} intake    : {detail}{suffix}")
}

/// Writes the `intake` provenance line for contentious filing closures (`docket`, `admit`, `audit`).
pub(crate) fn write_filing_intake_provenance(
    intake: Option<&CaseloadIntake>,
    disp: StageDisposition,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    let suffix = fast_forward_suffix(disp);
    let glyph = glyph_for(disp);
    let detail = match intake {
        Some(inc) if inc.scope == IntakeScope::AllTargets => {
            "all repository targets ingested".to_string()
        }
        Some(inc) if !inc.diffs.is_empty() => {
            let added: usize = inc.diffs.values().map(|a| a.lines_added).sum();
            let deleted: usize = inc.diffs.values().map(|a| a.lines_deleted).sum();
            format!(
                "{} modified file(s) ingested (+{added} / -{deleted} lines)",
                inc.diffs.len()
            )
        }
        Some(inc) if !inc.target_paths.is_empty() => format!(
            "{} target directive(s) ingested ({})",
            inc.target_paths.len(),
            inc.target_paths.join(", ")
        ),
        Some(inc)
            if inc.pr_title.is_some()
                || inc.pr_body.is_some()
                || inc.linked_issues.as_ref().is_some_and(|v| !v.is_empty())
                || inc.commit_messages.as_ref().is_some_and(|v| !v.is_empty()) =>
        {
            "pull request metadata ingested".to_string()
        }
        Some(_) => "0 modified files ingested".to_string(),
        None => "(not populated)".to_string(),
    };
    writeln!(stdout, "  {glyph} intake    : {detail}{suffix}")
}

/// Writes the `discover` provenance line across multi-stage closures.
pub(crate) fn write_discover_provenance(
    discovery: Option<&CaseloadDiscovery>,
    disp: StageDisposition,
    total_canons: Option<usize>,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    let glyph = glyph_for(disp);
    let suffix = fast_forward_suffix(disp);
    let matched = discovery.map_or(0, |d| d.candidate_canons.len());
    let detail = match (matched, total_canons) {
        (0, Some(total)) => format!("0 candidate canons matched ({total} canons evaluated)"),
        (0, None) => "0 candidate canons matched".to_string(),
        (n, Some(total)) if total >= n => format!(
            "{n} candidate canon(s) matched ({} un-triggered)",
            total - n
        ),
        (n, _) => format!("{n} candidate canon(s) matched"),
    };
    writeln!(stdout, "  {glyph} discover  : {detail}{suffix}")
}

/// Writes the `validate` provenance line across multi-stage closures.
pub(crate) fn write_validate_provenance(
    validation: Option<&CaseloadValidation>,
    disp: StageDisposition,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    if disp == StageDisposition::SkippedZeroCandidates {
        return writeln!(
            stdout,
            "  ⊘ validate  : (fast-forward: zero candidate canons)"
        );
    }
    let glyph = glyph_for(disp);
    let suffix = fast_forward_suffix(disp);
    let (total, errors, warnings) = match validation {
        Some(val) => {
            let errs: usize = val.results.values().map(|m| m.error_count).sum();
            let warns: usize = val.results.values().map(|m| m.warning_count).sum();
            (val.results.len(), errs, warns)
        }
        None => (0, 0, 0),
    };
    let status_word = if disp == StageDisposition::ValidationFailed {
        "failed"
    } else {
        "valid"
    };
    writeln!(
        stdout,
        "  {glyph} validate  : {total} candidate canon(s) {status_word} ({errors} errors, {warnings} warnings){suffix}"
    )
}

/// Formats closure node bindings grouped by shared resolved `<provider>:<model>` in canonical order
/// (e.g. `docket, admit, vet 'google:gemini-decision-1'; audit 'google:gemini-4-carbon'`).
pub(crate) fn format_closure_node_bindings(
    config: Option<&CaseloadConfig>,
    closure_nodes: &[HeuristicNode],
) -> String {
    let Some(cfg) = config else {
        return "<unresolved>".to_string();
    };
    let mut groups: Vec<(String, Vec<&'static str>)> = Vec::new();
    for &node in closure_nodes {
        let Some(model) = cfg.model_for(node) else {
            continue;
        };
        if let Some((_, names)) = groups.iter_mut().find(|(m, _)| m == model) {
            names.push(node.as_str());
        } else {
            groups.push((model.to_string(), vec![node.as_str()]));
        }
    }
    if groups.is_empty() {
        return "<unresolved>".to_string();
    }
    groups
        .into_iter()
        .map(|(model, names)| format!("{} '{model}'", names.join(", ")))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Writes the `configure` provenance line across converged closures (`scheduling.md` §6).
pub(crate) fn write_closure_configure_provenance(
    caseload: &Caseload,
    closure_nodes: &[HeuristicNode],
    disp: StageDisposition,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    if let Some(reason) = skipped_fast_forward_reason(disp) {
        return writeln!(stdout, "  ⊘ configure : (fast-forward: {reason})");
    }
    let glyph = glyph_for(disp);
    let suffix = fast_forward_suffix(disp);
    let bindings = format_closure_node_bindings(caseload.config.as_ref(), closure_nodes);
    writeln!(stdout, "  {glyph} configure : {bindings}{suffix}")
}

/// Writes the `docket` provenance line across contentious cascade closures (`docket`, `admit`, `audit`).
pub(crate) fn write_docket_provenance(
    docket: Option<&CaseloadDocket>,
    disp: StageDisposition,
    threshold: f64,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    match disp {
        StageDisposition::SkippedZeroCandidates => {
            writeln!(
                stdout,
                "  ⊘ docket    : (fast-forward: zero candidate canons)"
            )
        }
        StageDisposition::SkippedValidationFailed => {
            writeln!(
                stdout,
                "  ⊘ docket    : (fast-forward: canon validation failed)"
            )
        }
        _ => {
            let suffix = fast_forward_suffix(disp);
            let (docketed, dismissed) =
                docket.map_or((0, 0), |d| (d.docketed_count(), d.dismissed_count()));
            let glyph = if docketed == 0 && disp == StageDisposition::Executed {
                "⊘"
            } else {
                glyph_for(disp)
            };
            writeln!(
                stdout,
                "  {glyph} docket    : {docketed} case(s) colorable ({dismissed} dismissed, threshold {threshold:.2}){suffix}"
            )
        }
    }
}

/// Writes the `admit` provenance line across contentious cascade closures (`admit`, `audit`).
pub(crate) fn write_admit_provenance(
    evidence: Option<&CaseloadEvidence>,
    disp: StageDisposition,
    threshold: f64,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    match disp {
        StageDisposition::SkippedZeroCandidates => {
            writeln!(
                stdout,
                "  ⊘ admit     : (fast-forward: zero candidate canons)"
            )
        }
        StageDisposition::SkippedValidationFailed => {
            writeln!(
                stdout,
                "  ⊘ admit     : (fast-forward: canon validation failed)"
            )
        }
        StageDisposition::SkippedEmptyDocket => {
            writeln!(
                stdout,
                "  ⊘ admit     : (fast-forward: empty active docket)"
            )
        }
        _ => {
            let suffix = fast_forward_suffix(disp);
            let (admitted, excluded, active_cases) = evidence.map_or((0, 0, 0), |ev| {
                (
                    ev.admitted_exhibit_count(),
                    ev.excluded_exhibit_count(),
                    ev.active_cases.len(),
                )
            });
            let glyph = if admitted == 0 && disp == StageDisposition::Executed {
                "⊘"
            } else {
                glyph_for(disp)
            };
            writeln!(
                stdout,
                "  {glyph} admit     : {admitted} exhibit(s) admitted across {active_cases} case(s) ({excluded} excluded, threshold {threshold:.2}){suffix}"
            )
        }
    }
}

fn skipped_fast_forward_reason(disp: StageDisposition) -> Option<&'static str> {
    match disp {
        StageDisposition::SkippedZeroCandidates => Some("zero candidate canons"),
        StageDisposition::SkippedValidationFailed => Some("canon validation failed"),
        StageDisposition::SkippedEmptyDocket => Some("empty active docket"),
        StageDisposition::SkippedZeroEvidence => Some("no admissible exhibits"),
        StageDisposition::SkippedAllVetted => Some("all active cases vetted"),
        _ => None,
    }
}

/// Writes the `vet` provenance line across `vet` and `audit` cascade closures.
pub(crate) fn write_vet_provenance(
    vetting: Option<&CaseloadVetting>,
    evidence: Option<&CaseloadEvidence>,
    disp: StageDisposition,
    threshold: f64,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    if let Some(reason) = skipped_fast_forward_reason(disp) {
        return writeln!(stdout, "  ⊘ vet       : (fast-forward: {reason})");
    }
    if disp == StageDisposition::SkippedAbsentVetting {
        let active = evidence.map_or(0, |ev| ev.active_cases.len());
        return writeln!(
            stdout,
            "  ⊘ vet       : skipped (all {active} active case(s) bound directly to audit)"
        );
    }
    let suffix = fast_forward_suffix(disp);
    let (vetted, contested) = vetting.map_or((0, 0), |v| (v.vetted_count(), v.contested_count()));
    let glyph = if vetted == 0 && contested == 0 && disp == StageDisposition::Executed {
        "⊘"
    } else {
        glyph_for(disp)
    };
    writeln!(
        stdout,
        "  {glyph} vet       : {vetted} case(s) vetted, {contested} contested (threshold {threshold:.2}){suffix}"
    )
}

/// Writes the `audit` provenance line for the 8-stage judicial adjudication closure.
pub(crate) fn write_audit_provenance(
    verdict: Option<&CaseloadVerdict>,
    vetting: Option<&CaseloadVetting>,
    disp: StageDisposition,
    threshold: f64,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    match disp {
        StageDisposition::SkippedZeroCandidates => {
            writeln!(
                stdout,
                "  ⊘ audit     : (fast-forward: zero candidate canons)"
            )
        }
        StageDisposition::SkippedValidationFailed => {
            writeln!(
                stdout,
                "  ⊘ audit     : (fast-forward: canon validation failed)"
            )
        }
        StageDisposition::SkippedEmptyDocket => {
            writeln!(
                stdout,
                "  ⊘ audit     : 0 trials scheduled (fast-forward: empty active docket)"
            )
        }
        StageDisposition::SkippedZeroEvidence => {
            writeln!(
                stdout,
                "  ⊘ audit     : 0 trials scheduled (fast-forward: no admissible exhibits)"
            )
        }
        StageDisposition::SkippedAllVetted => {
            let vetted = verdict.map_or(0, CaseloadVerdict::passed_count);
            writeln!(
                stdout,
                "  ⊘ audit     : {vetted} vetted, 0 trials scheduled (fast-forward: all active cases vetted)"
            )
        }
        _ => {
            let suffix = fast_forward_suffix(disp);
            let (passed, failed, total) = verdict.map_or((0, 0, 0), |v| {
                (v.passed_count(), v.failed_count(), v.adjudications.len())
            });
            let trials = vetting.map_or(total, CaseloadVetting::contested_count);
            let glyph = if failed > 0 {
                "✗"
            } else if trials == 0 && disp == StageDisposition::Executed {
                "⊘"
            } else {
                glyph_for(disp)
            };
            writeln!(
                stdout,
                "  {glyph} audit     : {passed} passed, {failed} failed across {trials} trial(s) (threshold {threshold:.2}){suffix}"
            )
        }
    }
}
