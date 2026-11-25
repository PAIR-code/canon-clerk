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

//! Intent-required guard, stage-prefixed flag validation, and fast-forward predicates for `canon-clerk apprise`.

use super::AppriseArgs;
use crate::apprise::{
    DEFAULT_APPRISE_MAX_CONCURRENT_CALLS, DEFAULT_APPRISE_THRESHOLD, ENV_APPRISE_BATCH_SIZE,
    ENV_APPRISE_CONCURRENCY, ENV_APPRISE_THRESHOLD,
};
use crate::caseload::{Caseload, HeuristicNode, normalize_unit_score};
use crate::cli::CliError;
use crate::cli::closure::has_provider_overrides;
use crate::cli::node_args::check_retired_tuning_env_vars;
use crate::configure::StoredConfig;
use crate::intake::IntakeOptions;
use std::collections::BTreeMap;

pub(super) fn ensure_stage_prefixed_threshold_flags(args: &AppriseArgs) -> Result<(), CliError> {
    args.rejected_unqualified.validate(
        "apprise",
        "`--apprise-threshold <FLOAT>` (or set `CANON_CLERK_APPRISE_THRESHOLD`)",
        "`--apprise-model <PROVIDER:MODEL>`",
        Some("`--apprise-batch-size <COUNT>`"),
        "`--apprise-concurrency <COUNT>`",
    )?;
    let _ = args.models.resolve_node_models()?;
    if args.rejected_docket_threshold.is_some() {
        return Err(CliError::usage(
            "Out-of-closure flag `--docket-threshold` is not valid for `canon-clerk apprise` (`docket` is pruned from the apprisal closure).\n  \
             Hint: Pass `--apprise-threshold <FLOAT>` for prospective statutory apprisal.",
        ));
    }
    if args.rejected_admit_threshold.is_some() {
        return Err(CliError::usage(
            "Out-of-closure flag `--admit-threshold` is not valid for `canon-clerk apprise` (`admit` is pruned from the apprisal closure).\n  \
             Hint: Pass `--apprise-threshold <FLOAT>` for prospective statutory apprisal.",
        ));
    }
    if args.rejected_docket_batch_size.is_some() {
        return Err(CliError::usage(
            "Out-of-closure flag `--docket-batch-size` is not valid for `canon-clerk apprise` (`docket` is pruned from the apprisal closure).\n  \
             Hint: Pass `--apprise-batch-size <COUNT>` for prospective statutory apprisal.",
        ));
    }
    if args.rejected_docket_concurrency.is_some() {
        return Err(CliError::usage(
            "Out-of-closure flag `--docket-concurrency` is not valid for `canon-clerk apprise` (`docket` is pruned from the apprisal closure).\n  \
             Hint: Pass `--apprise-concurrency <COUNT>` for prospective statutory apprisal.",
        ));
    }
    Ok(())
}

pub(super) fn resolve_apprise_threshold(
    flag_threshold: Option<f64>,
    env: &BTreeMap<String, String>,
    stored: Option<&StoredConfig>,
) -> Result<f64, CliError> {
    if let Some(val) = flag_threshold {
        return normalize_unit_score(val).map_err(|detail| {
            CliError::usage(format!(
                "Invalid `--apprise-threshold` value `{val}`: {detail}.\n  \
                 Hint: Pass a finite float in `[0.0, 1.0]` via `--apprise-threshold <FLOAT>` (default: `{DEFAULT_APPRISE_THRESHOLD}`)."
            ))
        });
    }

    if let Some(raw_env) = env
        .get(ENV_APPRISE_THRESHOLD)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let parsed = raw_env.parse::<f64>().map_err(|_| {
            CliError::usage(format!(
                "Invalid threshold `{raw_env}` in `{ENV_APPRISE_THRESHOLD}`: expected a float in `[0.0, 1.0]`.\n  \
                 Hint: Pass `--apprise-threshold <FLOAT>` or set `{ENV_APPRISE_THRESHOLD}={DEFAULT_APPRISE_THRESHOLD}`."
            ))
        })?;
        return normalize_unit_score(parsed).map_err(|detail| {
            CliError::usage(format!(
                "Invalid threshold `{raw_env}` in `{ENV_APPRISE_THRESHOLD}`: {detail}.\n  \
                 Hint: Set `{ENV_APPRISE_THRESHOLD}` to a finite value in `[0.0, 1.0]` (default: `{DEFAULT_APPRISE_THRESHOLD}`)."
            ))
        });
    }

    if let Some(stored_val) = stored.and_then(|s| s.node_threshold(HeuristicNode::Apprise)) {
        return normalize_unit_score(stored_val).map_err(|detail| {
            CliError::usage(format!(
                "Invalid `nodes.apprise.threshold` value `{stored_val}` in user configuration: {detail}.\n  \
                 Hint: Configure a finite float in `[0.0, 1.0]` for `nodes.apprise.threshold` or override with `--apprise-threshold <FLOAT>`."
            ))
        });
    }

    Ok(DEFAULT_APPRISE_THRESHOLD)
}

pub(super) fn resolve_apprise_batch_size(
    flag_batch_size: Option<usize>,
    env: &BTreeMap<String, String>,
    stored: Option<&StoredConfig>,
) -> Result<Option<usize>, CliError> {
    check_retired_tuning_env_vars(env)?;
    if let Some(val) = flag_batch_size {
        if val == 0 {
            return Err(CliError::usage(format!(
                "Invalid `--apprise-batch-size` value `0`: batch size must be at least 1.\n  \
                 Hint: Pass a positive integer (`>= 1`) via `--apprise-batch-size <COUNT>` or `{ENV_APPRISE_BATCH_SIZE}`."
            )));
        }
        return Ok(Some(val));
    }

    if let Some(raw_env) = env
        .get(ENV_APPRISE_BATCH_SIZE)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let parsed = raw_env.parse::<usize>().ok().filter(|v| *v >= 1).ok_or_else(|| {
            CliError::usage(format!(
                "Invalid batch size `{raw_env}` in `{ENV_APPRISE_BATCH_SIZE}`: expected a positive integer (`>= 1`).\n  \
                 Hint: Pass `--apprise-batch-size <COUNT>` or set `{ENV_APPRISE_BATCH_SIZE}=<COUNT>`."
            ))
        })?;
        return Ok(Some(parsed));
    }

    if let Some(stored_val) = stored.and_then(|s| s.node_batch_size(HeuristicNode::Apprise)) {
        if stored_val == 0 {
            return Err(CliError::usage(format!(
                "Invalid `nodes.apprise.batchSize` value `0` in user configuration: batch size must be at least 1.\n  \
                 Hint: Configure a positive integer (`>= 1`) for `nodes.apprise.batchSize` or override with `--apprise-batch-size <COUNT>` / `{ENV_APPRISE_BATCH_SIZE}`."
            )));
        }
        return Ok(Some(stored_val));
    }

    Ok(None)
}

pub(super) fn resolve_apprise_concurrency(
    flag_concurrency: Option<usize>,
    env: &BTreeMap<String, String>,
    stored: Option<&StoredConfig>,
) -> Result<Option<usize>, CliError> {
    check_retired_tuning_env_vars(env)?;
    if let Some(val) = flag_concurrency {
        if val == 0 {
            return Err(CliError::usage(format!(
                "Invalid `--apprise-concurrency` value `0`: concurrency limit must be at least 1.\n  \
                 Hint: Pass a positive integer (`>= 1`) via `--apprise-concurrency <COUNT>` or `{ENV_APPRISE_CONCURRENCY}` (default: `{DEFAULT_APPRISE_MAX_CONCURRENT_CALLS}`)."
            )));
        }
        return Ok(Some(val));
    }

    if let Some(raw_env) = env
        .get(ENV_APPRISE_CONCURRENCY)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let parsed = raw_env.parse::<usize>().ok().filter(|v| *v >= 1).ok_or_else(|| {
            CliError::usage(format!(
                "Invalid concurrency limit `{raw_env}` in `{ENV_APPRISE_CONCURRENCY}`: expected a positive integer (`>= 1`).\n  \
                 Hint: Pass `--apprise-concurrency <COUNT>` or set `{ENV_APPRISE_CONCURRENCY}={DEFAULT_APPRISE_MAX_CONCURRENT_CALLS}`."
            ))
        })?;
        return Ok(Some(parsed));
    }

    if let Some(stored_val) = stored.and_then(|s| s.node_concurrency(HeuristicNode::Apprise)) {
        if stored_val == 0 {
            return Err(CliError::usage(format!(
                "Invalid `nodes.apprise.concurrency` value `0` in user configuration: concurrency limit must be at least 1.\n  \
                 Hint: Configure a positive integer (`>= 1`) for `nodes.apprise.concurrency` or override with `--apprise-concurrency <COUNT>` / `{ENV_APPRISE_CONCURRENCY}`."
            )));
        }
        return Ok(Some(stored_val));
    }

    Ok(None)
}

pub(super) fn can_fast_forward_completed_apprisal(
    args: &AppriseArgs,
    intake_opts: &IntakeOptions,
    incoming: Option<&Caseload>,
) -> bool {
    incoming.is_some_and(|c| c.apprisal.is_some())
        && !intake_opts.has_designated_source()
        && args.canons.is_empty()
        && args.apprise_threshold.is_none()
        && args.apprise_batch_size.is_none()
        && args.apprise_concurrency.is_none()
        && !args.models.has_overrides()
        && !has_provider_overrides(&args.provider)
}

pub(super) fn enforce_intent_required_guard(
    intake_opts: &IntakeOptions,
    incoming: Option<&Caseload>,
) -> Result<(), CliError> {
    let has_intent = if let Some(cli_intent) = &intake_opts.intent {
        !cli_intent.trim().is_empty()
    } else {
        incoming
            .and_then(|c| c.intake.as_ref())
            .and_then(|i| i.intent.as_deref())
            .is_some_and(|s| !s.trim().is_empty())
    };

    if has_intent {
        Ok(())
    } else {
        Err(CliError::usage(
            "Prospective apprisal requires a non-empty design intent (`--intent <TEXT>`).\n  \
             Hint: Pass `--intent <TEXT>` describing the planned work (defaults to `--all-canons`, \
             or narrow with target paths / `--canon <PATH|GLOB>`), or supply `--caseload <PATH|->` \
             with `.intake.intent` populated.",
        ))
    }
}
