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

//! Domain error types and contextual case/slice failure formatting for `audit`.

use super::{
    DEFAULT_AUDIT_MAX_CONCURRENT_CALLS, DEFAULT_AUDIT_THRESHOLD, ENV_AUDIT_CONCURRENCY,
    ENV_AUDIT_THRESHOLD,
};
use crate::caseload::DEFAULT_AUDITOR_MODEL;
use crate::provider::ProviderError;
use std::fmt;

/// Domain errors produced during `audit` execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditError {
    /// Configured threshold is non-finite or outside `[0.0, 1.0]`.
    InvalidThreshold { detail: String },
    /// Configured `max_concurrent_calls` is zero (`0`).
    InvalidConcurrency { detail: String },
    /// Case-or-Controversy violation: `.intake` is missing from the incoming `Caseload`.
    MissingIntake,
    /// Prerequisite `.discovery` namespace is missing from the incoming `Caseload`.
    MissingDiscovery,
    /// Upstream `.validation` recorded one or more candidate canon lint errors.
    ValidationFailed,
    /// Direct prerequisite `.evidence` namespace is missing from the incoming `Caseload`.
    MissingEvidence,
    /// Prerequisite `.config` namespace is missing when active cases require trial adjudication.
    MissingConfig,
    /// Configured `auditor_model` is a discriminative classifier incapable of structured decrees or line annotations.
    DiscriminativeAuditorUnsupported { specifier: String },
    /// An active canon listed in `.evidence.active_cases` is absent from the workspace snapshot.
    MissingActiveCanon { canon_ref: String },
    /// Trial slice call failed with contextual `canon_ref` and 1-based slice coordinates.
    CaseProvider {
        /// Governing canon identifier whose trial failed.
        canon_ref: String,
        /// 1-based slice index (`1..=slice_count`).
        slice_index: usize,
        /// Total slices prepared for this case (`>= 1`).
        slice_count: usize,
        /// Underlying provider timeout, transport, HTTP, block, or schema decoding error.
        source: ProviderError,
    },
    /// Unscoped structured model provider error.
    Provider(ProviderError),
}

impl AuditError {
    pub(super) fn from_case_call(
        canon_ref: &str,
        slice_index: usize,
        slice_count: usize,
        source: ProviderError,
    ) -> Self {
        Self::CaseProvider {
            canon_ref: canon_ref.to_string(),
            slice_index,
            slice_count,
            source,
        }
    }
}

impl fmt::Display for AuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidThreshold { detail } => write!(
                f,
                "Invalid audit threshold: {detail}.\n  \
                 Hint: Pass a finite threshold in `[0.0, 1.0]` via `--audit-threshold <FLOAT>` \
                 or `{ENV_AUDIT_THRESHOLD}` (default: `{DEFAULT_AUDIT_THRESHOLD}`)."
            ),
            Self::InvalidConcurrency { detail } => write!(
                f,
                "Invalid audit concurrency limit: {detail}.\n  \
                 Hint: Pass a positive integer (`>= 1`) via `--audit-concurrency <COUNT>` \
                 or `{ENV_AUDIT_CONCURRENCY}` (default: `{DEFAULT_AUDIT_MAX_CONCURRENT_CALLS}`)."
            ),
            Self::MissingIntake => write!(
                f,
                "Substantive adjudication (`audit`) requires an underlying filing in `.intake` (Case-or-Controversy invariant).\n  \
                 Hint: Pipe a diff (`--diff -`), specify target paths (`src/...`, `-`, `--all-targets`), \
                 or pass `--caseload <PATH|->` with `.intake` populated."
            ),
            Self::MissingDiscovery => write!(
                f,
                "Substantive adjudication (`audit`) requires a resolved `.discovery` namespace.\n  \
                 Hint: Run `canon-clerk discover` first or invoke `canon-clerk audit` with a filing source."
            ),
            Self::ValidationFailed => write!(
                f,
                "Cannot execute `audit` because one or more candidate canons failed static validation.\n  \
                 Hint: Run `canon-clerk validate` to inspect and repair malformed candidate canons."
            ),
            Self::MissingEvidence => write!(
                f,
                "Substantive adjudication (`audit`) requires a resolved `.evidence` namespace from `admit`.\n  \
                 Hint: Run `canon-clerk admit` first or invoke `canon-clerk audit` directly to execute screening upstream."
            ),
            Self::MissingConfig => write!(
                f,
                "Cannot execute `audit` trials without a resolved `.config` namespace on the Caseload.\n  \
                 Hint: Run `canon-clerk configure` first or invoke `canon-clerk audit` directly to resolve configuration JIT."
            ),
            Self::DiscriminativeAuditorUnsupported { specifier } => write!(
                f,
                "Discriminative model `{specifier}` cannot adjudicate `audit` trials \
                 (`audit` requires a generative model for exception rulings, remediation guidance, and line annotations).\n  \
                 Hint: Use `canon-clerk vet --screener-model jev-latest` for discriminative vetting, or configure \
                 `{DEFAULT_AUDITOR_MODEL}` (`gemini-4-carbon`) as `--auditor-model`."
            ),
            Self::MissingActiveCanon { canon_ref } => write!(
                f,
                "Active case canon `{canon_ref}` referenced in `.evidence.active_cases` was not found in the workspace snapshot.\n  \
                 Hint: Re-run `canon-clerk admit` or `canon-clerk audit` against the current workspace."
            ),
            Self::CaseProvider {
                canon_ref,
                slice_index,
                slice_count,
                source,
            } => {
                if *slice_count > 1 {
                    write!(
                        f,
                        "Audit trial failed for canon `{canon_ref}` (slice {slice_index}/{slice_count}): {source}"
                    )
                } else {
                    write!(f, "Audit trial failed for canon `{canon_ref}`: {source}")
                }
            }
            Self::Provider(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for AuditError {}

impl From<ProviderError> for AuditError {
    fn from(err: ProviderError) -> Self {
        Self::Provider(err)
    }
}
