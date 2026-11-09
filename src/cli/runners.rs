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

//! Shared upstream stage runners for Branch A (Filing) and Branch B (Environment) pipelines.

use crate::caseload::Caseload;
use crate::cli::CliError;
use crate::cli::args::{CanonSelectionArgs, FilingSourceArgs, ProviderConfigArgs};
use crate::configure::{
    ConfigureOptions, execute_configure, load_host_config_file, resolve_default_config_path,
};
use crate::discover::{DiscoverError, DiscoverOptions, execute_discover_on_disk};
use crate::intake::{IntakeError, execute_intake};
use crate::validate::{ValidateError, ValidateOptions, execute_validate_on_disk};
use std::collections::BTreeMap;
use std::env;
use std::io::Read;
use std::path::Path;

/// Maps a domain [`DiscoverError`] to a typed [`CliError`] preserving exit code conventions (`1` IO/runtime, `2` usage).
pub fn map_discover_error(err: DiscoverError) -> CliError {
    match err {
        DiscoverError::MissingInputSource
        | DiscoverError::InvalidGlob(_)
        | DiscoverError::PathEscapesWorkspace { .. } => CliError::usage(err.to_string()),
        DiscoverError::Io { .. } => CliError::runtime(err.to_string()),
    }
}

/// Executes Branch A `intake` stage, honoring fast-forward skip semantics if `.intake` is already present.
pub fn run_branch_a_intake(
    filing: &FilingSourceArgs,
    incoming: Option<Caseload>,
    stdin: &mut dyn Read,
) -> Result<Caseload, CliError> {
    let intake_opts = filing.resolve_intake_options(stdin)?;

    if intake_opts.has_designated_source() {
        execute_intake(&intake_opts, incoming).map_err(|err| match err {
            IntakeError::MissingInputSource | IntakeError::MalformedDiff(_) => {
                CliError::usage(err.to_string())
            }
        })
    } else if let Some(existing) = incoming {
        Ok(existing)
    } else {
        Err(CliError::usage(IntakeError::MissingInputSource.to_string()))
    }
}

/// Executes the Branch A `[intake, discover]` transitive closure with fast-forward skipping.
pub fn run_branch_a_discover(
    filing: &FilingSourceArgs,
    canons: &CanonSelectionArgs,
    incoming: Option<Caseload>,
    workspace_root: &Path,
    stdin: &mut dyn Read,
) -> Result<Caseload, CliError> {
    let intake_opts = filing.resolve_intake_options(stdin)?;

    let caseload_after_intake = if intake_opts.has_designated_source() {
        execute_intake(&intake_opts, incoming).map_err(|err| match err {
            IntakeError::MissingInputSource | IntakeError::MalformedDiff(_) => {
                CliError::usage(err.to_string())
            }
        })?
    } else if let Some(existing) = incoming {
        existing
    } else if canons.all_canons {
        Caseload::new()
    } else {
        return Err(CliError::usage(
            DiscoverError::MissingInputSource.to_string(),
        ));
    };

    if caseload_after_intake.discovery.is_some() && canons.is_empty() {
        Ok(caseload_after_intake)
    } else {
        let discover_opts = DiscoverOptions {
            all_canons: canons.all_canons,
            canon_patterns: canons.canon.clone(),
        };
        execute_discover_on_disk(&discover_opts, caseload_after_intake, workspace_root)
            .map_err(map_discover_error)
    }
}

/// Executes the Branch A `[intake, discover, validate]` transitive closure with fast-forward skipping.
pub fn run_branch_a_validate(
    filing: &FilingSourceArgs,
    canons: &CanonSelectionArgs,
    incoming: Option<Caseload>,
    workspace_root: &Path,
    stdin: &mut dyn Read,
    max_warnings: Option<usize>,
) -> Result<Caseload, CliError> {
    let intake_opts = filing.resolve_intake_options(stdin)?;

    let caseload_after_intake = if intake_opts.has_designated_source() {
        execute_intake(&intake_opts, incoming).map_err(|err| match err {
            IntakeError::MissingInputSource | IntakeError::MalformedDiff(_) => {
                CliError::usage(err.to_string())
            }
        })?
    } else if let Some(existing) = incoming {
        existing
    } else if canons.all_canons || !canons.canon.is_empty() {
        Caseload::new()
    } else {
        return Err(CliError::usage(ValidateError::MissingDiscovery.to_string()));
    };

    let caseload_after_discover = if caseload_after_intake.discovery.is_some() && canons.is_empty()
    {
        caseload_after_intake
    } else {
        let discover_opts = DiscoverOptions {
            all_canons: canons.all_canons
                || (caseload_after_intake.intake.is_none() && !canons.canon.is_empty()),
            canon_patterns: canons.canon.clone(),
        };
        execute_discover_on_disk(&discover_opts, caseload_after_intake, workspace_root)
            .map_err(map_discover_error)?
    };

    if caseload_after_discover.validation.is_some() && canons.is_empty() && max_warnings.is_none() {
        Ok(caseload_after_discover)
    } else {
        let validate_opts = ValidateOptions { max_warnings };
        execute_validate_on_disk(&validate_opts, caseload_after_discover, workspace_root).map_err(
            |err| match err {
                ValidateError::MissingDiscovery => CliError::usage(err.to_string()),
                ValidateError::Discover(d) => map_discover_error(d),
            },
        )
    }
}

/// Resolves host config file, environment variables, and CLI overrides into Branch B [`ConfigureOptions`].
pub fn resolve_branch_b_options(
    provider: &ProviderConfigArgs,
    env: &BTreeMap<String, String>,
) -> Result<ConfigureOptions, CliError> {
    let cwd = env::current_dir().ok();
    let stored_config = match resolve_default_config_path(env) {
        Some(cfg_path) => load_host_config_file(&cfg_path, cwd.as_deref())
            .map_err(|err| CliError::usage(err.to_string()))?,
        None => None,
    };

    Ok(ConfigureOptions {
        screener_model: provider.screener_model.clone(),
        auditor_model: provider.auditor_model.clone(),
        reasoning_budget: provider.reasoning_budget,
        gemini_api_key: provider.gemini_api_key.clone(),
        env: env.clone(),
        stored_config,
    })
}

/// Executes Branch B `configure` stage, fast-forwarding if `.config` is present and no overrides are specified.
pub fn run_branch_b_configure(
    options: &ConfigureOptions,
    incoming: Option<Caseload>,
) -> Result<Caseload, CliError> {
    if let Some(existing) = incoming.as_ref()
        && existing.config.is_some()
        && !options.has_explicit_overrides()
    {
        Ok(existing.clone())
    } else {
        execute_configure(options, incoming).map_err(|err| CliError::usage(err.to_string()))
    }
}

/// Extracts and validates the active Gemini API key from resolved [`ConfigureOptions`].
pub fn resolve_gemini_api_key(options: &ConfigureOptions) -> Result<String, CliError> {
    crate::configure::resolve_gemini_api_key(options)
        .map_err(|err| CliError::usage(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::CaseloadConfig;
    use std::io::Cursor;

    #[test]
    fn test_map_discover_error_exit_codes() {
        let missing = DiscoverError::MissingInputSource;
        assert_eq!(map_discover_error(missing).exit_code(), 2);

        let glob_err = DiscoverError::InvalidGlob(
            crate::discover::GlobPattern::parse("[unclosed").unwrap_err(),
        );
        assert_eq!(map_discover_error(glob_err).exit_code(), 2);

        let io_err = DiscoverError::Io {
            path: "foo".to_string(),
            message: "not found".to_string(),
        };
        assert_eq!(map_discover_error(io_err).exit_code(), 1);
    }

    #[test]
    fn test_run_branch_a_intake_fast_forward_when_already_populated() {
        let existing = Caseload::new();
        let filing = FilingSourceArgs::default();
        let mut stdin = Cursor::new(b"");
        let result = run_branch_a_intake(&filing, Some(existing.clone()), &mut stdin)
            .expect("fast-forward skip on intake");
        assert_eq!(result, existing);
    }

    #[test]
    fn test_run_branch_b_configure_fast_forward_when_already_populated() {
        let existing = Caseload::new().with_config(CaseloadConfig {
            screener_model: "google:gemini-2.5-flash-lite".to_string(),
            auditor_model: "google:gemini-2.5-pro".to_string(),
            reasoning_budget: Some(1024),
        });
        let options = ConfigureOptions {
            screener_model: None,
            auditor_model: None,
            reasoning_budget: None,
            gemini_api_key: None,
            env: BTreeMap::new(),
            stored_config: None,
        };
        let result = run_branch_b_configure(&options, Some(existing.clone()))
            .expect("fast-forward skip on config");
        assert_eq!(result, existing);
    }
}
