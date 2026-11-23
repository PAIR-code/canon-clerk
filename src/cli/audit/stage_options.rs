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

//! Stage-option resolution and required-node computation helpers for the `audit` CLI cascade.

use super::AuditArgs;
use super::guards::resolve_audit_concurrency;
use super::stages::AuditCascadeThresholds;
use crate::admit::AdmitOptions;
use crate::audit::{AuditOptions, DEFAULT_AUDIT_TIMEOUT_MS};
use crate::caseload::{CaseloadConfig, HeuristicNode};
use crate::cli::CliError;
use crate::cli::admit::guards::resolve_admit_concurrency;
use crate::cli::docket::guards::{resolve_docket_batch_size, resolve_docket_concurrency};
use crate::cli::vet::guards::resolve_vet_concurrency;
use crate::configure::StoredConfig;
use crate::docket::DocketOptions;
use crate::provider::{DEFAULT_GENERATION_TIMEOUT_MS, default_timeout_ms_for_model};
use crate::vet::VetOptions;
use std::collections::BTreeMap;

pub(super) struct ResolvedStageOptions {
    pub(super) explicit_docket_batch_size: Option<usize>,
    pub(super) docket_concurrency: Option<usize>,
    pub(super) admit_opts: AdmitOptions,
    pub(super) vet_opts: VetOptions,
    pub(super) audit_opts: AuditOptions,
}

impl ResolvedStageOptions {
    pub(super) fn docket_opts(
        &self,
        cfg_snapshot: &CaseloadConfig,
        docket_threshold: f64,
        explicit_timeout_ms: Option<u64>,
    ) -> DocketOptions {
        let docket_model = cfg_snapshot
            .model_for(HeuristicNode::Docket)
            .unwrap_or_default();
        DocketOptions {
            threshold: docket_threshold,
            timeout_ms: explicit_timeout_ms
                .unwrap_or_else(|| default_timeout_ms_for_model(docket_model)),
            max_canons_per_call: self.explicit_docket_batch_size,
            max_concurrent_calls: self.docket_concurrency,
        }
    }

    pub(super) fn admit_opts(
        &self,
        cfg_snapshot: &CaseloadConfig,
        explicit_timeout_ms: Option<u64>,
    ) -> AdmitOptions {
        let admit_model = cfg_snapshot
            .model_for(HeuristicNode::Admit)
            .unwrap_or_default();
        AdmitOptions {
            timeout_ms: explicit_timeout_ms
                .unwrap_or_else(|| default_timeout_ms_for_model(admit_model)),
            ..self.admit_opts.clone()
        }
    }

    pub(super) fn vet_opts(
        &self,
        cfg_snapshot: &CaseloadConfig,
        explicit_timeout_ms: Option<u64>,
    ) -> VetOptions {
        let vet_model = cfg_snapshot
            .model_for(HeuristicNode::Vet)
            .unwrap_or_default();
        VetOptions {
            timeout_ms: explicit_timeout_ms
                .unwrap_or_else(|| default_timeout_ms_for_model(vet_model)),
            ..self.vet_opts.clone()
        }
    }

    pub(super) fn audit_opts(
        &self,
        cfg_snapshot: &CaseloadConfig,
        explicit_timeout_ms: Option<u64>,
    ) -> AuditOptions {
        let audit_model = cfg_snapshot
            .model_for(HeuristicNode::Audit)
            .unwrap_or_default();
        AuditOptions {
            timeout_ms: explicit_timeout_ms
                .unwrap_or_else(|| default_timeout_ms_for_model(audit_model)),
            ..self.audit_opts.clone()
        }
    }
}

pub(super) fn compute_required_nodes(
    docket_pre_populated: bool,
    evidence_pre_populated: bool,
    vetting_pre_populated: bool,
) -> Vec<HeuristicNode> {
    let mut nodes = Vec::new();
    if !docket_pre_populated && !evidence_pre_populated {
        nodes.push(HeuristicNode::Docket);
    }
    if !evidence_pre_populated {
        nodes.push(HeuristicNode::Admit);
    }
    if !vetting_pre_populated {
        nodes.push(HeuristicNode::Vet);
    }
    nodes.push(HeuristicNode::Audit);
    nodes
}

pub(super) fn build_stage_options(
    args: &AuditArgs,
    env: &BTreeMap<String, String>,
    stored: Option<&StoredConfig>,
    thresholds: AuditCascadeThresholds,
) -> Result<ResolvedStageOptions, CliError> {
    Ok(ResolvedStageOptions {
        explicit_docket_batch_size: resolve_docket_batch_size(args.docket_batch_size, env, stored)?,
        docket_concurrency: resolve_docket_concurrency(args.docket_concurrency, env, stored)?,
        admit_opts: AdmitOptions {
            threshold: thresholds.admit_threshold,
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
            max_concurrent_calls: resolve_admit_concurrency(args.admit_concurrency, env, stored)?,
        },
        vet_opts: VetOptions {
            threshold: thresholds.vet_threshold,
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
            max_concurrent_calls: resolve_vet_concurrency(args.vet_concurrency, env, stored)?,
            ..VetOptions::default()
        },
        audit_opts: AuditOptions {
            threshold: thresholds.audit_threshold,
            timeout_ms: DEFAULT_AUDIT_TIMEOUT_MS,
            max_concurrent_calls: resolve_audit_concurrency(args.audit_concurrency, env, stored)?,
        },
    })
}
