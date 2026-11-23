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

//! Shared closure workspace materialization, metadata merging, and `validate` resolution helpers.

use crate::caseload::{Caseload, CaseloadIntake, normalize_commit_messages};
use crate::cli::CliError;
use crate::cli::args::{CanonSelectionArgs, ProviderConfigArgs};
use crate::cli::provenance::StageDisposition;
use crate::cli::runners::map_discover_error;
use crate::discover::{DiscoverOptions, WorkspaceSnapshot, execute_discover};
use crate::intake::{IntakeError, IntakeOptions, execute_intake};
use crate::provider::StructuredModelProvider;
use crate::telemetry::{PipelineStage, TelemetrySession};
use crate::validate::{ValidateError, ValidateOptions, execute_validate};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

/// Execution backend for converged CLI closures (`apprise`, `docket`, `admit`, `audit`).
pub(crate) enum ClosureBackend<'a> {
    /// Reads workspace files from `workspace_root` on disk and invokes live `GeminiModelProvider`.
    LiveDisk { workspace_root: &'a Path },
    /// Evaluates 100% hermetically in-memory against `workspace` and injected `provider`.
    Hermetic {
        workspace: &'a WorkspaceSnapshot,
        provider: &'a dyn StructuredModelProvider,
    },
}

/// Returns `true` if any explicit CLI provider, reasoning, or credential override flag was passed.
pub(crate) fn has_provider_overrides(provider: &ProviderConfigArgs) -> bool {
    provider.provider.is_some()
        || provider.thinking_level.is_some()
        || provider.reasoning_budget.is_some()
        || provider.gemini_api_key.is_some()
        || provider.typesafe_api_key.is_some()
        || provider.gemini_base_url.is_some()
        || provider.typesafe_base_url.is_some()
}

/// Returns `true` if `opts` specifies any inspectable filing metadata plane override
/// (`pr_title`, `pr_body`, `linked_issues`, or `commit_messages`) that can affect
/// `discover` canon eligibility and therefore requires invalidating `.discovery` and `.validation`.
pub(crate) fn has_filing_metadata_overrides(opts: &IntakeOptions) -> bool {
    opts.pr_title
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
        || opts
            .pr_body
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
        || opts
            .linked_issues
            .as_ref()
            .is_some_and(|issues| !issues.is_empty())
        || opts
            .commit_messages
            .as_ref()
            .is_some_and(|commits| commits.iter().any(|c| !c.message.trim().is_empty()))
}

/// Merges supplementary CLI metadata (`intent`, `pr_title`, `pr_body`, `linked_issues`,
/// `commit_messages`, `workspace_state`) onto an existing [`CaseloadIntake`].
pub(crate) fn merge_metadata_onto_intake(
    mut base: CaseloadIntake,
    opts: &IntakeOptions,
) -> CaseloadIntake {
    if let Some(intent) = opts
        .intent
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        base.intent = Some(intent.to_string());
    }
    if let Some(title) = opts
        .pr_title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        base.pr_title = Some(title.to_string());
    }
    if let Some(body) = opts
        .pr_body
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        base.pr_body = Some(body.to_string());
    }
    if let Some(issues) = opts
        .linked_issues
        .clone()
        .filter(|issues| !issues.is_empty())
    {
        base.linked_issues = Some(issues);
    }
    if let Some(commits) = opts
        .commit_messages
        .clone()
        .and_then(normalize_commit_messages)
    {
        base.commit_messages = Some(commits);
    }
    if let Some(ws_state) = opts.workspace_state {
        base.workspace_state = ws_state;
    }
    base
}

/// Materializes an in-memory [`WorkspaceSnapshot`] from `backend`, backfilling any pre-populated
/// candidate canons (including `inline:*`) referenced in `caseload.discovery` and resolving any
/// explicitly supplied CLI inline canons.
pub(crate) fn materialize_closure_snapshot(
    backend: &ClosureBackend<'_>,
    canons: &CanonSelectionArgs,
    stdin: &mut dyn Read,
    caseload: &Caseload,
) -> Result<(WorkspaceSnapshot, BTreeMap<String, String>), CliError> {
    let inline_canons = canons.resolve_inline_canons(stdin)?;
    let mut snapshot = match backend {
        ClosureBackend::Hermetic { workspace, .. } => (*workspace).clone(),
        ClosureBackend::LiveDisk { workspace_root } => {
            let mut snapshot = WorkspaceSnapshot::from_disk(workspace_root, &canons.canon)
                .map_err(map_discover_error)?;
            if let Some(disc) = &caseload.discovery {
                for canon_ref in &disc.candidate_canons {
                    snapshot.backfill_canon_ref(workspace_root, canon_ref);
                }
            }
            snapshot
        }
    };
    if let Some(disc) = &caseload.discovery {
        snapshot.backfill_inline_canons(&disc.inline_canons);
    }
    for (source_ref, raw_markdown) in &inline_canons {
        snapshot.insert_inline_canon(source_ref.clone(), raw_markdown);
    }
    Ok((snapshot, inline_canons))
}

/// Executes `intake` within a contentious cascade (`vet`, `audit`), fast-forwarding if `.intake` is pre-populated.
pub(crate) fn resolve_cascade_intake(
    stage_name: &str,
    intake_opts: &IntakeOptions,
    incoming: Option<Caseload>,
    telemetry: &TelemetrySession<'_>,
) -> Result<(Caseload, StageDisposition, bool), CliError> {
    if !intake_opts.has_designated_source() {
        let mut existing = incoming.ok_or_else(|| {
            CliError::usage(format!(
                "Missing upstream caseload or filing inputs for `{stage_name}`."
            ))
        })?;
        if let Some(ws_state) = intake_opts.workspace_state
            && let Some(intake) = existing.intake.as_mut()
        {
            intake.workspace_state = ws_state;
        }
        return Ok((existing, StageDisposition::PrePopulated, false));
    }

    telemetry.run_stage(PipelineStage::Intake, |_scope| {
        execute_cascade_intake_body(intake_opts, incoming)
    })
}

fn execute_cascade_intake_body(
    intake_opts: &IntakeOptions,
    mut incoming: Option<Caseload>,
) -> Result<(Caseload, StageDisposition, bool), CliError> {
    let cli_has_targets = intake_opts.all_targets
        || intake_opts.patch_content.is_some()
        || intake_opts.target_paths.is_some();

    if !cli_has_targets
        && let Some(existing) = incoming.as_mut()
        && let Some(prev_intake) = existing.intake.take()
    {
        let pr_meta_added = has_filing_metadata_overrides(intake_opts);
        let merged = merge_metadata_onto_intake(prev_intake, intake_opts);
        existing.docket = None;
        existing.evidence = None;
        existing.vetting = None;
        existing.verdict = None;
        existing.intake = Some(merged);
        if pr_meta_added {
            existing.discovery = None;
            existing.validation = None;
        }
        return Ok((existing.clone(), StageDisposition::Executed, pr_meta_added));
    }

    let mut updated = execute_intake(intake_opts, incoming).map_err(|err| match err {
        IntakeError::MissingInputSource | IntakeError::MalformedDiff(_) => {
            CliError::usage(err.to_string())
        }
    })?;
    updated.discovery = None;
    updated.validation = None;
    updated.docket = None;
    updated.evidence = None;
    updated.vetting = None;
    updated.verdict = None;
    Ok((updated, StageDisposition::Executed, true))
}

/// Executes `discover` within a contentious cascade (`vet`, `audit`), fast-forwarding if `.discovery` is pre-populated.
pub(crate) fn resolve_cascade_discover(
    canons: &CanonSelectionArgs,
    inline_canons: &BTreeMap<String, String>,
    mut caseload: Caseload,
    snapshot: &WorkspaceSnapshot,
    intake_changed: bool,
    telemetry: &TelemetrySession<'_>,
) -> Result<(Caseload, StageDisposition), CliError> {
    if caseload.discovery.is_some() && !intake_changed && canons.is_empty() {
        return Ok((caseload, StageDisposition::PrePopulated));
    }

    caseload.discovery = None;
    caseload.validation = None;
    caseload.docket = None;
    caseload.evidence = None;
    caseload.vetting = None;
    caseload.verdict = None;

    let discover_opts = DiscoverOptions {
        all_canons: canons.all_canons,
        canon_patterns: canons.canon.clone(),
        include_builtins: canons.include_builtins(),
        include_workspace_canons: canons.include_workspace_canons(),
        inline_canons: inline_canons.clone(),
    };

    let updated = telemetry.run_stage(PipelineStage::Discover, |_scope| {
        execute_discover(&discover_opts, caseload, snapshot).map_err(map_discover_error)
    })?;
    Ok((updated, StageDisposition::Executed))
}

/// Executes `validate` within a converged closure, fast-forwarding if `.validation` is already populated.
pub(crate) fn resolve_closure_validate(
    caseload: Caseload,
    snapshot: &WorkspaceSnapshot,
    telemetry: &TelemetrySession<'_>,
) -> Result<(Caseload, StageDisposition), CliError> {
    if caseload.validation.is_some() {
        return Ok((caseload, StageDisposition::PrePopulated));
    }
    let updated = telemetry.run_stage(PipelineStage::Validate, |_scope| {
        execute_validate(&ValidateOptions::default(), caseload, snapshot).map_err(|err| match err {
            ValidateError::MissingDiscovery => CliError::usage(err.to_string()),
            ValidateError::Discover(d) => map_discover_error(d),
        })
    })?;
    Ok((updated, StageDisposition::Executed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{
        CaseloadDiscovery, CaseloadDocket, CaseloadEvidence, CaseloadValidation, CaseloadVerdict,
        CaseloadVetting, CommitMessageContext, LinkedIssueContext,
    };

    fn sample_linked_issue(number: u64, title: &str) -> LinkedIssueContext {
        LinkedIssueContext {
            number,
            title: Some(title.to_string()),
            author: Some("jimbojw".to_string()),
            labels: vec!["type: fix".to_string()],
            body: Some("Issue details".to_string()),
            url: Some(format!(
                "https://github.com/PAIR-code/canon-clerk/issues/{number}"
            )),
        }
    }

    #[test]
    fn test_caseload_override_with_commit_messages_merges_and_invalidates_discovery() {
        let incoming = Caseload::new()
            .with_intake(CaseloadIntake {
                target_paths: vec!["src/lib.rs".to_string()],
                ..CaseloadIntake::default()
            })
            .with_discovery(CaseloadDiscovery::default())
            .with_validation(CaseloadValidation::default());

        let intake_opts = IntakeOptions {
            commit_messages: Some(vec![CommitMessageContext {
                sha: Some("abc1234".to_string()),
                message: "feat(cli): override commit".to_string(),
            }]),
            ..IntakeOptions::default()
        };

        let (updated, disp, intake_changed) = resolve_cascade_intake(
            "audit",
            &intake_opts,
            Some(incoming),
            &TelemetrySession::noop(),
        )
        .expect("merge commit_messages onto incoming caseload");

        assert_eq!(disp, StageDisposition::Executed);
        assert!(intake_changed);
        assert!(updated.discovery.is_none());
        assert!(updated.validation.is_none());
        let intake = updated.intake.expect("intake");
        assert_eq!(intake.target_paths, vec!["src/lib.rs".to_string()]);
        assert_eq!(
            intake.commit_messages.unwrap()[0].message,
            "feat(cli): override commit"
        );
    }

    #[test]
    fn test_caseload_override_with_linked_issues_replaces_and_invalidates_cascade() {
        let incoming = Caseload::new()
            .with_intake(CaseloadIntake {
                target_paths: vec!["src/lib.rs".to_string()],
                linked_issues: Some(vec![sample_linked_issue(383, "Original issue")]),
                ..CaseloadIntake::default()
            })
            .with_discovery(CaseloadDiscovery::default())
            .with_validation(CaseloadValidation::default())
            .with_docket(CaseloadDocket::default())
            .with_evidence(CaseloadEvidence::default())
            .with_vetting(CaseloadVetting::default())
            .with_verdict(CaseloadVerdict::default());

        let intake_opts = IntakeOptions {
            linked_issues: Some(vec![sample_linked_issue(391, "Override issue")]),
            ..IntakeOptions::default()
        };

        let (updated, disp, intake_changed) = resolve_cascade_intake(
            "audit",
            &intake_opts,
            Some(incoming),
            &TelemetrySession::noop(),
        )
        .expect("merge linked_issues onto incoming caseload");

        assert_eq!(disp, StageDisposition::Executed);
        assert!(intake_changed);
        assert!(updated.discovery.is_none());
        assert!(updated.validation.is_none());
        assert!(updated.docket.is_none());
        assert!(updated.evidence.is_none());
        assert!(updated.vetting.is_none());
        assert!(updated.verdict.is_none());
        let intake = updated.intake.expect("intake");
        assert_eq!(intake.target_paths, vec!["src/lib.rs".to_string()]);
        let issues = intake.linked_issues.expect("linked_issues");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 391);
        assert_eq!(issues[0].title.as_deref(), Some("Override issue"));
    }

    #[test]
    fn test_empty_linked_issues_does_not_override_or_trigger_metadata_invalidation() {
        let base = CaseloadIntake {
            linked_issues: Some(vec![sample_linked_issue(383, "Original issue")]),
            ..CaseloadIntake::default()
        };
        let opts = IntakeOptions {
            linked_issues: Some(Vec::new()),
            ..IntakeOptions::default()
        };

        assert!(!has_filing_metadata_overrides(&opts));
        let merged = merge_metadata_onto_intake(base, &opts);
        let issues = merged
            .linked_issues
            .expect("existing linked_issues preserved");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 383);
    }
}
