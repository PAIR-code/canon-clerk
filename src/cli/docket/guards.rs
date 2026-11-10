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

//! Case-or-Controversy guard, stage-prefixed threshold validation, and fast-forward predicates for `canon-clerk docket`.

use super::DocketArgs;
use crate::caseload::{Caseload, normalize_colorability_score};
use crate::cli::CliError;
use crate::cli::args::CanonSelectionArgs;
use crate::cli::closure::has_provider_overrides;
use crate::docket::{DEFAULT_DOCKET_THRESHOLD, ENV_DOCKET_THRESHOLD};
use crate::intake::IntakeOptions;
use std::collections::BTreeMap;

pub(super) fn ensure_stage_prefixed_threshold_flags(args: &DocketArgs) -> Result<(), CliError> {
    if args.rejected_threshold.is_some() {
        return Err(CliError::usage(
            "Ambiguous unqualified flag `--threshold` is disallowed.\n  \
             Hint: Pass stage-prefixed `--docket-threshold <FLOAT>` \
             (or set `CANON_CLERK_DOCKET_THRESHOLD`) for `canon-clerk docket`.",
        ));
    }
    if args.rejected_apprise_threshold.is_some() {
        return Err(CliError::usage(
            "Out-of-closure flag `--apprise-threshold` is not valid for `canon-clerk docket` (`apprise` is pruned from the docket closure).\n  \
             Hint: Pass `--docket-threshold <FLOAT>` for macro subject-matter jurisdiction triage.",
        ));
    }
    if args.rejected_admit_threshold.is_some() {
        return Err(CliError::usage(
            "Out-of-closure flag `--admit-threshold` is not valid for `canon-clerk docket` (`admit` is downstream of `docket`).\n  \
             Hint: Pass `--docket-threshold <FLOAT>` for macro subject-matter jurisdiction triage.",
        ));
    }
    Ok(())
}

pub(super) fn resolve_docket_threshold(
    flag_threshold: Option<f64>,
    env: &BTreeMap<String, String>,
) -> Result<f64, CliError> {
    if let Some(val) = flag_threshold {
        return normalize_colorability_score(val).map_err(|detail| {
            CliError::usage(format!(
                "Invalid `--docket-threshold` value `{val}`: {detail}.\n  \
                 Hint: Pass a finite float in `[0.0, 1.0]` via `--docket-threshold <FLOAT>` (default: `{DEFAULT_DOCKET_THRESHOLD}`)."
            ))
        });
    }

    if let Some(raw_env) = env
        .get(ENV_DOCKET_THRESHOLD)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let parsed = raw_env.parse::<f64>().map_err(|_| {
            CliError::usage(format!(
                "Invalid threshold `{raw_env}` in `{ENV_DOCKET_THRESHOLD}`: expected a float in `[0.0, 1.0]`.\n  \
                 Hint: Pass `--docket-threshold <FLOAT>` or set `{ENV_DOCKET_THRESHOLD}=0.5`."
            ))
        })?;
        return normalize_colorability_score(parsed).map_err(|detail| {
            CliError::usage(format!(
                "Invalid threshold `{raw_env}` in `{ENV_DOCKET_THRESHOLD}`: {detail}.\n  \
                 Hint: Set `{ENV_DOCKET_THRESHOLD}` to a finite value in `[0.0, 1.0]` (default: `{DEFAULT_DOCKET_THRESHOLD}`)."
            ))
        });
    }

    Ok(DEFAULT_DOCKET_THRESHOLD)
}

pub(super) fn can_fast_forward_completed_docket(
    args: &DocketArgs,
    intake_opts: &IntakeOptions,
    incoming: Option<&Caseload>,
) -> bool {
    incoming.is_some_and(|c| c.intake.is_some() && c.docket.is_some())
        && !intake_opts.has_designated_source()
        && args.canons.is_empty()
        && args.docket_threshold.is_none()
        && !has_provider_overrides(&args.provider)
}

pub(super) fn has_concrete_cli_filing(intake_opts: &IntakeOptions) -> bool {
    intake_opts.all_targets
        || intake_opts.patch_content.is_some()
        || intake_opts.target_paths.is_some()
        || has_non_blank(&intake_opts.pr_title)
        || has_non_blank(&intake_opts.pr_body)
        || intake_opts
            .linked_issues
            .as_ref()
            .is_some_and(|issues| !issues.is_empty())
}

pub(super) fn enforce_filing_and_case_guard(
    intake_opts: &IntakeOptions,
    canons: &CanonSelectionArgs,
    incoming: Option<&Caseload>,
) -> Result<(), CliError> {
    let cli_has_filing = has_concrete_cli_filing(intake_opts);
    let upstream_has_intake = incoming.is_some_and(|c| c.intake.is_some());

    if cli_has_filing || upstream_has_intake {
        return Ok(());
    }

    if !canons.is_empty() || incoming.is_some() {
        return Err(CliError::usage(
            "Case-or-Controversy violation: `canon-clerk docket` cannot open an active docket of cases without an underlying filing in `.intake`.\n  \
             Hint: Provide target paths (`src/...`, `-`, `--all-targets`), pipe a diff (`--diff -`), \
             or pass `--caseload <PATH|->` produced from an upstream filing.",
        ));
    }

    Err(CliError::usage(
        "No filing source provided for `docket`.\n  \
         Hint: Pipe a diff via standard input (`--diff -`), specify target paths (`src/...`, `-`, `--all-targets`), \
         or pass an upstream caseload (`--caseload <PATH|->`) with `.intake` populated.",
    ))
}

fn has_non_blank(opt: &Option<String>) -> bool {
    opt.as_deref().is_some_and(|s| !s.trim().is_empty())
}
