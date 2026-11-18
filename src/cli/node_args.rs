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

//! Closure-scoped `--<node>-model` CLI argument groups and unqualified/retired flag guards (`scheduling.md` §4).

use crate::caseload::HeuristicNode;
use crate::cli::CliError;
use clap::Args;
use std::collections::BTreeMap;

/// Full 5-node `--<node>-model` argument group for `configure`, `probe`, and `consistency_bench`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct AllNodeModelArgs {
    /// Canonical `<provider>:<model>` override for `apprise` (env: `CANON_CLERK_APPRISE_MODEL`).
    #[arg(
        long = "apprise-model",
        value_name = "PROVIDER:MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Canonical <provider>:<model> override for apprise (fallback env: CANON_CLERK_APPRISE_MODEL)"
    )]
    pub apprise_model: Option<String>,

    /// Canonical `<provider>:<model>` override for `docket` (env: `CANON_CLERK_DOCKET_MODEL`).
    #[arg(
        long = "docket-model",
        value_name = "PROVIDER:MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Canonical <provider>:<model> override for docket (fallback env: CANON_CLERK_DOCKET_MODEL)"
    )]
    pub docket_model: Option<String>,

    /// Canonical `<provider>:<model>` override for `admit` (env: `CANON_CLERK_ADMIT_MODEL`).
    #[arg(
        long = "admit-model",
        value_name = "PROVIDER:MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Canonical <provider>:<model> override for admit (fallback env: CANON_CLERK_ADMIT_MODEL)"
    )]
    pub admit_model: Option<String>,

    /// Canonical `<provider>:<model>` override for `vet` (env: `CANON_CLERK_VET_MODEL`).
    #[arg(
        long = "vet-model",
        value_name = "PROVIDER:MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Canonical <provider>:<model> override for vet (fallback env: CANON_CLERK_VET_MODEL)"
    )]
    pub vet_model: Option<String>,

    /// Canonical `<provider>:<model>` override for `audit` (env: `CANON_CLERK_AUDIT_MODEL`).
    #[arg(
        long = "audit-model",
        value_name = "PROVIDER:MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Canonical <provider>:<model> override for audit (fallback env: CANON_CLERK_AUDIT_MODEL)"
    )]
    pub audit_model: Option<String>,
}

impl AllNodeModelArgs {
    /// Collects all explicitly provided `--<node>-model` CLI flags into a canonical [`BTreeMap`].
    pub fn to_node_map(&self) -> BTreeMap<HeuristicNode, String> {
        let mut map = BTreeMap::new();
        insert_opt(&mut map, HeuristicNode::Apprise, &self.apprise_model);
        insert_opt(&mut map, HeuristicNode::Docket, &self.docket_model);
        insert_opt(&mut map, HeuristicNode::Admit, &self.admit_model);
        insert_opt(&mut map, HeuristicNode::Vet, &self.vet_model);
        insert_opt(&mut map, HeuristicNode::Audit, &self.audit_model);
        map
    }

    /// Returns `true` when at least one `--<node>-model` flag was supplied.
    pub fn has_overrides(&self) -> bool {
        self.apprise_model.is_some()
            || self.docket_model.is_some()
            || self.admit_model.is_some()
            || self.vet_model.is_some()
            || self.audit_model.is_some()
    }
}

/// Closure-scoped `--apprise-model` argument group for `canon-clerk apprise`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct AppriseNodeModelArgs {
    /// Canonical `<provider>:<model>` override for `apprise` (env: `CANON_CLERK_APPRISE_MODEL`).
    #[arg(
        long = "apprise-model",
        value_name = "PROVIDER:MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Canonical <provider>:<model> override for apprise (fallback env: CANON_CLERK_APPRISE_MODEL)"
    )]
    pub apprise_model: Option<String>,

    #[arg(long = "docket-model", hide = true, allow_hyphen_values = true)]
    pub rejected_docket_model: Option<String>,
    #[arg(long = "admit-model", hide = true, allow_hyphen_values = true)]
    pub rejected_admit_model: Option<String>,
    #[arg(long = "vet-model", hide = true, allow_hyphen_values = true)]
    pub rejected_vet_model: Option<String>,
    #[arg(long = "audit-model", hide = true, allow_hyphen_values = true)]
    pub rejected_audit_model: Option<String>,
}

impl AppriseNodeModelArgs {
    /// Validates closure scoping and returns the map of explicit node model overrides.
    pub fn resolve_node_models(&self) -> Result<BTreeMap<HeuristicNode, String>, CliError> {
        for (flag, val) in [
            ("--docket-model", &self.rejected_docket_model),
            ("--admit-model", &self.rejected_admit_model),
            ("--vet-model", &self.rejected_vet_model),
            ("--audit-model", &self.rejected_audit_model),
        ] {
            reject_out_of_closure_model_flag(
                flag,
                val,
                "apprise",
                "`--apprise-model <PROVIDER:MODEL>`",
            )?;
        }
        let mut map = BTreeMap::new();
        insert_opt(&mut map, HeuristicNode::Apprise, &self.apprise_model);
        Ok(map)
    }

    /// Returns `true` when `--apprise-model` was explicitly supplied.
    pub fn has_overrides(&self) -> bool {
        self.apprise_model.is_some()
    }
}

/// Closure-scoped `--docket-model` argument group for `canon-clerk docket`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct DocketNodeModelArgs {
    /// Canonical `<provider>:<model>` override for `docket` (env: `CANON_CLERK_DOCKET_MODEL`).
    #[arg(
        long = "docket-model",
        value_name = "PROVIDER:MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Canonical <provider>:<model> override for docket (fallback env: CANON_CLERK_DOCKET_MODEL)"
    )]
    pub docket_model: Option<String>,

    #[arg(long = "apprise-model", hide = true, allow_hyphen_values = true)]
    pub rejected_apprise_model: Option<String>,
    #[arg(long = "admit-model", hide = true, allow_hyphen_values = true)]
    pub rejected_admit_model: Option<String>,
    #[arg(long = "vet-model", hide = true, allow_hyphen_values = true)]
    pub rejected_vet_model: Option<String>,
    #[arg(long = "audit-model", hide = true, allow_hyphen_values = true)]
    pub rejected_audit_model: Option<String>,
}

impl DocketNodeModelArgs {
    /// Validates closure scoping and returns the map of explicit node model overrides.
    pub fn resolve_node_models(&self) -> Result<BTreeMap<HeuristicNode, String>, CliError> {
        for (flag, val) in [
            ("--apprise-model", &self.rejected_apprise_model),
            ("--admit-model", &self.rejected_admit_model),
            ("--vet-model", &self.rejected_vet_model),
            ("--audit-model", &self.rejected_audit_model),
        ] {
            reject_out_of_closure_model_flag(
                flag,
                val,
                "docket",
                "`--docket-model <PROVIDER:MODEL>`",
            )?;
        }
        let mut map = BTreeMap::new();
        insert_opt(&mut map, HeuristicNode::Docket, &self.docket_model);
        Ok(map)
    }

    /// Returns `true` when `--docket-model` was explicitly supplied.
    pub fn has_overrides(&self) -> bool {
        self.docket_model.is_some()
    }
}

/// Closure-scoped `[docket, admit]` model argument group for `canon-clerk admit`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct AdmitNodeModelArgs {
    /// Canonical `<provider>:<model>` override for `admit` (env: `CANON_CLERK_ADMIT_MODEL`).
    #[arg(
        long = "admit-model",
        value_name = "PROVIDER:MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Canonical <provider>:<model> override for admit (fallback env: CANON_CLERK_ADMIT_MODEL)"
    )]
    pub admit_model: Option<String>,

    /// Canonical `<provider>:<model>` override for upstream `docket` (env: `CANON_CLERK_DOCKET_MODEL`).
    #[arg(
        long = "docket-model",
        value_name = "PROVIDER:MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Canonical <provider>:<model> override for upstream docket (fallback env: CANON_CLERK_DOCKET_MODEL)"
    )]
    pub docket_model: Option<String>,

    #[arg(long = "apprise-model", hide = true, allow_hyphen_values = true)]
    pub rejected_apprise_model: Option<String>,
    #[arg(long = "vet-model", hide = true, allow_hyphen_values = true)]
    pub rejected_vet_model: Option<String>,
    #[arg(long = "audit-model", hide = true, allow_hyphen_values = true)]
    pub rejected_audit_model: Option<String>,
}

impl AdmitNodeModelArgs {
    /// Validates closure scoping and returns the map of explicit node model overrides.
    pub fn resolve_node_models(&self) -> Result<BTreeMap<HeuristicNode, String>, CliError> {
        for (flag, val) in [
            ("--apprise-model", &self.rejected_apprise_model),
            ("--vet-model", &self.rejected_vet_model),
            ("--audit-model", &self.rejected_audit_model),
        ] {
            reject_out_of_closure_model_flag(
                flag,
                val,
                "admit",
                "`--admit-model <PROVIDER:MODEL>` or `--docket-model <PROVIDER:MODEL>`",
            )?;
        }
        let mut map = BTreeMap::new();
        insert_opt(&mut map, HeuristicNode::Docket, &self.docket_model);
        insert_opt(&mut map, HeuristicNode::Admit, &self.admit_model);
        Ok(map)
    }

    /// Returns `true` when `--docket-model` or `--admit-model` was explicitly supplied.
    pub fn has_overrides(&self) -> bool {
        self.docket_model.is_some() || self.admit_model.is_some()
    }
}

/// Closure-scoped `[docket, admit, vet]` model argument group for `canon-clerk vet`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct VetNodeModelArgs {
    /// Canonical `<provider>:<model>` override for `vet` (env: `CANON_CLERK_VET_MODEL`).
    #[arg(
        long = "vet-model",
        value_name = "PROVIDER:MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Canonical <provider>:<model> override for vet (fallback env: CANON_CLERK_VET_MODEL)"
    )]
    pub vet_model: Option<String>,

    /// Canonical `<provider>:<model>` override for upstream `admit` (env: `CANON_CLERK_ADMIT_MODEL`).
    #[arg(
        long = "admit-model",
        value_name = "PROVIDER:MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Canonical <provider>:<model> override for upstream admit (fallback env: CANON_CLERK_ADMIT_MODEL)"
    )]
    pub admit_model: Option<String>,

    /// Canonical `<provider>:<model>` override for upstream `docket` (env: `CANON_CLERK_DOCKET_MODEL`).
    #[arg(
        long = "docket-model",
        value_name = "PROVIDER:MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Canonical <provider>:<model> override for upstream docket (fallback env: CANON_CLERK_DOCKET_MODEL)"
    )]
    pub docket_model: Option<String>,

    #[arg(long = "apprise-model", hide = true, allow_hyphen_values = true)]
    pub rejected_apprise_model: Option<String>,
    #[arg(long = "audit-model", hide = true, allow_hyphen_values = true)]
    pub rejected_audit_model: Option<String>,
}

impl VetNodeModelArgs {
    /// Validates closure scoping and returns the map of explicit node model overrides.
    pub fn resolve_node_models(&self) -> Result<BTreeMap<HeuristicNode, String>, CliError> {
        for (flag, val) in [
            ("--apprise-model", &self.rejected_apprise_model),
            ("--audit-model", &self.rejected_audit_model),
        ] {
            reject_out_of_closure_model_flag(
                flag,
                val,
                "vet",
                "`--vet-model <PROVIDER:MODEL>`, `--admit-model <PROVIDER:MODEL>`, or `--docket-model <PROVIDER:MODEL>`",
            )?;
        }
        let mut map = BTreeMap::new();
        insert_opt(&mut map, HeuristicNode::Docket, &self.docket_model);
        insert_opt(&mut map, HeuristicNode::Admit, &self.admit_model);
        insert_opt(&mut map, HeuristicNode::Vet, &self.vet_model);
        Ok(map)
    }

    /// Returns `true` when any closure `--<node>-model` flag was explicitly supplied.
    pub fn has_overrides(&self) -> bool {
        self.docket_model.is_some() || self.admit_model.is_some() || self.vet_model.is_some()
    }
}

/// Closure-scoped `[docket, admit, vet, audit]` model argument group for `canon-clerk audit`.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditNodeModelArgs {
    /// Canonical `<provider>:<model>` override for `audit` (env: `CANON_CLERK_AUDIT_MODEL`).
    #[arg(
        long = "audit-model",
        value_name = "PROVIDER:MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Canonical <provider>:<model> override for audit (fallback env: CANON_CLERK_AUDIT_MODEL)"
    )]
    pub audit_model: Option<String>,

    /// Canonical `<provider>:<model>` override for upstream `vet` (env: `CANON_CLERK_VET_MODEL`).
    #[arg(
        long = "vet-model",
        value_name = "PROVIDER:MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Canonical <provider>:<model> override for upstream vet (fallback env: CANON_CLERK_VET_MODEL)"
    )]
    pub vet_model: Option<String>,

    /// Canonical `<provider>:<model>` override for upstream `admit` (env: `CANON_CLERK_ADMIT_MODEL`).
    #[arg(
        long = "admit-model",
        value_name = "PROVIDER:MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Canonical <provider>:<model> override for upstream admit (fallback env: CANON_CLERK_ADMIT_MODEL)"
    )]
    pub admit_model: Option<String>,

    /// Canonical `<provider>:<model>` override for upstream `docket` (env: `CANON_CLERK_DOCKET_MODEL`).
    #[arg(
        long = "docket-model",
        value_name = "PROVIDER:MODEL",
        help_heading = "Model & Reasoning Options",
        help = "Canonical <provider>:<model> override for upstream docket (fallback env: CANON_CLERK_DOCKET_MODEL)"
    )]
    pub docket_model: Option<String>,

    #[arg(long = "apprise-model", hide = true, allow_hyphen_values = true)]
    pub rejected_apprise_model: Option<String>,
}

impl AuditNodeModelArgs {
    /// Validates closure scoping and returns the map of explicit node model overrides.
    pub fn resolve_node_models(&self) -> Result<BTreeMap<HeuristicNode, String>, CliError> {
        reject_out_of_closure_model_flag(
            "--apprise-model",
            &self.rejected_apprise_model,
            "audit",
            "`--audit-model`, `--vet-model`, `--admit-model`, or `--docket-model`",
        )?;
        let mut map = BTreeMap::new();
        insert_opt(&mut map, HeuristicNode::Docket, &self.docket_model);
        insert_opt(&mut map, HeuristicNode::Admit, &self.admit_model);
        insert_opt(&mut map, HeuristicNode::Vet, &self.vet_model);
        insert_opt(&mut map, HeuristicNode::Audit, &self.audit_model);
        Ok(map)
    }

    /// Returns `true` when any closure `--<node>-model` flag was explicitly supplied.
    pub fn has_overrides(&self) -> bool {
        self.docket_model.is_some()
            || self.admit_model.is_some()
            || self.vet_model.is_some()
            || self.audit_model.is_some()
    }
}

/// Hidden captures rejecting ambiguous unqualified or retired operational flags (`scheduling.md` §4).
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct RejectedUnqualifiedTuningArgs {
    #[arg(long = "threshold", hide = true, allow_hyphen_values = true)]
    pub rejected_threshold: Option<String>,
    #[arg(long = "model", hide = true, allow_hyphen_values = true)]
    pub rejected_model: Option<String>,
    #[arg(long = "batch-size", hide = true, allow_hyphen_values = true)]
    pub rejected_batch_size: Option<String>,
    #[arg(long = "max-canons-per-call", hide = true, allow_hyphen_values = true)]
    pub rejected_max_canons_per_call: Option<String>,
    #[arg(long = "concurrency", hide = true, allow_hyphen_values = true)]
    pub rejected_concurrency: Option<String>,
    #[arg(long = "max-concurrent-calls", hide = true, allow_hyphen_values = true)]
    pub rejected_max_concurrent_calls: Option<String>,
}

impl RejectedUnqualifiedTuningArgs {
    /// Rejects any ambiguous unqualified or retired alias CLI flag with exit code `2` and an actionable `Hint:`.
    pub fn validate(
        &self,
        subcommand: &str,
        threshold_hint: &str,
        model_hint: &str,
        batch_hint: Option<&str>,
        concurrency_hint: &str,
    ) -> Result<(), CliError> {
        if self.rejected_threshold.is_some() {
            return Err(CliError::usage(format!(
                "Ambiguous unqualified flag `--threshold` is disallowed on `canon-clerk {subcommand}`.\n  \
                 Hint: Pass stage-prefixed {threshold_hint}."
            )));
        }
        if self.rejected_model.is_some() {
            return Err(CliError::usage(format!(
                "Ambiguous unqualified flag `--model` is disallowed on `canon-clerk {subcommand}`.\n  \
                 Hint: Pass `--provider <slug>` or stage-prefixed {model_hint}."
            )));
        }
        for flag in [
            ("--batch-size", &self.rejected_batch_size),
            ("--max-canons-per-call", &self.rejected_max_canons_per_call),
        ] {
            if flag.1.is_some() {
                let hint = batch_hint.unwrap_or(
                    "per-stage `--apprise-batch-size <COUNT>` or `--docket-batch-size <COUNT>`",
                );
                return Err(CliError::usage(format!(
                    "Unqualified flag `{}` is disallowed on `canon-clerk {subcommand}`.\n  \
                     Hint: Pass {hint}.",
                    flag.0
                )));
            }
        }
        for flag in [
            ("--concurrency", &self.rejected_concurrency),
            (
                "--max-concurrent-calls",
                &self.rejected_max_concurrent_calls,
            ),
        ] {
            if flag.1.is_some() {
                return Err(CliError::usage(format!(
                    "Unqualified flag `{}` is disallowed on `canon-clerk {subcommand}`.\n  \
                     Hint: Pass stage-prefixed {concurrency_hint}.",
                    flag.0
                )));
            }
        }
        Ok(())
    }
}

/// Rejects retired `CANON_CLERK_MAX_CANONS_PER_CALL` and `CANON_CLERK_MAX_CONCURRENT_CALLS` env vars.
pub(crate) fn check_retired_tuning_env_vars(
    env: &BTreeMap<String, String>,
) -> Result<(), CliError> {
    if env
        .get("CANON_CLERK_MAX_CANONS_PER_CALL")
        .is_some_and(|v| !v.trim().is_empty())
    {
        return Err(CliError::usage(
            "Retired environment variable `CANON_CLERK_MAX_CANONS_PER_CALL` is no longer supported.\n  \
             Hint: Unset `CANON_CLERK_MAX_CANONS_PER_CALL` and set stage-prefixed `CANON_CLERK_APPRISE_BATCH_SIZE` or `CANON_CLERK_DOCKET_BATCH_SIZE`.",
        ));
    }
    if env
        .get("CANON_CLERK_MAX_CONCURRENT_CALLS")
        .is_some_and(|v| !v.trim().is_empty())
    {
        return Err(CliError::usage(
            "Retired environment variable `CANON_CLERK_MAX_CONCURRENT_CALLS` is no longer supported.\n  \
             Hint: Unset `CANON_CLERK_MAX_CONCURRENT_CALLS` and set stage-prefixed `CANON_CLERK_<STAGE>_CONCURRENCY`.",
        ));
    }
    Ok(())
}

fn insert_opt(
    map: &mut BTreeMap<HeuristicNode, String>,
    node: HeuristicNode,
    val: &Option<String>,
) {
    if let Some(raw) = val {
        map.insert(node, raw.clone());
    }
}

fn reject_out_of_closure_model_flag(
    flag: &str,
    val: &Option<String>,
    subcommand: &str,
    valid_flags_hint: &str,
) -> Result<(), CliError> {
    if val.is_some() {
        return Err(CliError::usage(format!(
            "Out-of-closure flag `{flag}` is not valid for `canon-clerk {subcommand}`.\n  \
             Hint: Pass {valid_flags_hint} on `canon-clerk {subcommand}`."
        )));
    }
    Ok(())
}
