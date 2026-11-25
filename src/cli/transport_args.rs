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

//! Composable CLI argument group for pooled HTTP transport timeout, retry, and cooldown bounds
//! across heuristic evaluation subcommands (`docs/architecture/transport.md` §6).

use crate::cli::CliError;
use crate::configure::StoredConfig;
use crate::provider::{
    DEFAULT_GENERATION_TIMEOUT_MS, DEFAULT_LOCAL_GENERATION_TIMEOUT_MS, DEFAULT_MAX_BACKOFF_MS,
    DEFAULT_MAX_RETRIES, ENV_MAX_BACKOFF_MS, ENV_MAX_RETRIES, ENV_TIMEOUT_MS, RetryPolicy,
    default_timeout_ms_for_model,
};
use clap::Args;
use std::collections::BTreeMap;

/// Composable CLI argument group for pooled HTTP transport timeout, retry, and per-origin rate-limit cooldown
/// bounds across heuristic evaluation subcommands (`apprise`, `docket`, `admit`, `vet`, `audit`).
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderTransportArgs {
    /// Per-attempt wire timeout in milliseconds (`>= 1`; default: `60000` cloud / `180000` `local:*`; env: `CANON_CLERK_TIMEOUT_MS`; config: `defaults.timeoutMs`).
    #[arg(
        long = "timeout",
        value_name = "MS",
        allow_hyphen_values = true,
        help_heading = "Provider Transport & Retry",
        help = format!(
            "Per-attempt wire timeout in milliseconds (default: {DEFAULT_GENERATION_TIMEOUT_MS} cloud, {DEFAULT_LOCAL_GENERATION_TIMEOUT_MS} local; fallback env: {ENV_TIMEOUT_MS})"
        )
    )]
    pub timeout: Option<String>,

    /// Maximum retry attempts per model call on transient `429`/`5xx`/timeout errors (`0` disables retries; default: `2`; env: `CANON_CLERK_MAX_RETRIES`).
    #[arg(
        long = "max-retries",
        value_name = "COUNT",
        allow_hyphen_values = true,
        help_heading = "Provider Transport & Retry",
        help = format!(
            "Maximum retry attempts per model call on transient 429/5xx/timeout errors; 0 disables retries (default: {DEFAULT_MAX_RETRIES}; fallback env: {ENV_MAX_RETRIES})"
        )
    )]
    pub max_retries: Option<String>,

    /// Maximum per-wait rate-limit cooldown or retry backoff in milliseconds (`0` fails fast on `429` waits; default: `90000`; env: `CANON_CLERK_MAX_BACKOFF_MS`).
    #[arg(
        long = "max-backoff-ms",
        value_name = "MS",
        allow_hyphen_values = true,
        help_heading = "Provider Transport & Retry",
        help = format!(
            "Maximum per-wait rate-limit cooldown or retry backoff in milliseconds; 0 fails fast on 429 waits (default: {DEFAULT_MAX_BACKOFF_MS}; fallback env: {ENV_MAX_BACKOFF_MS})"
        )
    )]
    pub max_backoff_ms: Option<String>,
}

impl ProviderTransportArgs {
    /// Resolves an explicit per-attempt wire timeout override following standard specificity precedence
    /// (`CLI --timeout -> CANON_CLERK_TIMEOUT_MS -> config.json defaults.timeoutMs`), returning `None` when
    /// unset so each stage applies its profile-calibrated default (`60,000ms` cloud / `180,000ms` `local:*`).
    pub fn resolve_explicit_timeout_ms(
        &self,
        env: &BTreeMap<String, String>,
        stored: Option<&StoredConfig>,
    ) -> Result<Option<u64>, CliError> {
        if let Some(raw_flag) = self.timeout.as_deref() {
            let parsed = raw_flag.trim().parse::<u64>().map_err(|_| {
                CliError::usage(format!(
                    "Invalid `--timeout` value `{raw_flag}`: expected a positive integer in milliseconds (`>= 1`).\n  \
                     Hint: Pass `--timeout <MS>` (default: `{DEFAULT_GENERATION_TIMEOUT_MS}` cloud, `{DEFAULT_LOCAL_GENERATION_TIMEOUT_MS}` local) or set `{ENV_TIMEOUT_MS}=<MS>`."
                ))
            })?;
            if parsed == 0 {
                return Err(CliError::usage(format!(
                    "Invalid `--timeout` value `0`: timeout must be greater than 0ms.\n  \
                     Hint: Pass a positive millisecond timeout via `--timeout <MS>` (default: `{DEFAULT_GENERATION_TIMEOUT_MS}` cloud, `{DEFAULT_LOCAL_GENERATION_TIMEOUT_MS}` local) or `{ENV_TIMEOUT_MS}`."
                )));
            }
            return Ok(Some(parsed));
        }

        if let Some(raw_env) = env
            .get(ENV_TIMEOUT_MS)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            let parsed = raw_env.parse::<u64>().map_err(|_| {
                CliError::usage(format!(
                    "Invalid timeout `{raw_env}` in `{ENV_TIMEOUT_MS}`: expected a positive integer in milliseconds (`>= 1`).\n  \
                     Hint: Set `{ENV_TIMEOUT_MS}={DEFAULT_GENERATION_TIMEOUT_MS}` (or `{DEFAULT_LOCAL_GENERATION_TIMEOUT_MS}` for local) or override via `--timeout <MS>`."
                ))
            })?;
            if parsed == 0 {
                return Err(CliError::usage(format!(
                    "Invalid timeout `0` in `{ENV_TIMEOUT_MS}`: timeout must be greater than 0ms.\n  \
                     Hint: Set a positive millisecond timeout in `{ENV_TIMEOUT_MS}` or override via `--timeout <MS>`."
                )));
            }
            return Ok(Some(parsed));
        }

        if let Some(stored_val) = stored.and_then(|s| s.defaults.timeout_ms) {
            if stored_val == 0 {
                return Err(CliError::usage(format!(
                    "Invalid `defaults.timeoutMs` value `0` in user configuration: timeout must be greater than 0ms.\n  \
                     Hint: Configure a positive millisecond timeout for `defaults.timeoutMs` or override via `--timeout <MS>` / `{ENV_TIMEOUT_MS}`."
                )));
            }
            return Ok(Some(stored_val));
        }

        Ok(None)
    }

    /// Resolves the effective per-attempt wire timeout for `model_specifier`, honoring explicit overrides or
    /// falling back to `180,000ms` for built-in `local:*` and `60,000ms` for cloud profiles.
    pub fn resolve_timeout_ms_for_model(
        &self,
        env: &BTreeMap<String, String>,
        stored: Option<&StoredConfig>,
        model_specifier: &str,
    ) -> Result<u64, CliError> {
        Ok(self
            .resolve_explicit_timeout_ms(env, stored)?
            .unwrap_or_else(|| default_timeout_ms_for_model(model_specifier)))
    }

    /// Resolves [`RetryPolicy`] following standard specificity precedence (`CLI flag -> env -> config.json -> default`).
    pub fn resolve_retry_policy(
        &self,
        env: &BTreeMap<String, String>,
        stored: Option<&StoredConfig>,
    ) -> Result<RetryPolicy, CliError> {
        let max_retries = self.resolve_max_retries(env, stored)?;
        let max_backoff_ms = self.resolve_max_backoff_ms(env, stored)?;
        Ok(RetryPolicy::new(max_retries, max_backoff_ms))
    }

    fn resolve_max_retries(
        &self,
        env: &BTreeMap<String, String>,
        stored: Option<&StoredConfig>,
    ) -> Result<u32, CliError> {
        if let Some(raw_flag) = self.max_retries.as_deref() {
            return raw_flag.trim().parse::<u32>().map_err(|_| {
                CliError::usage(format!(
                    "Invalid `--max-retries` value `{raw_flag}`: expected a non-negative integer (`>= 0`).\n  \
                     Hint: Pass `--max-retries <COUNT>` (default: `{DEFAULT_MAX_RETRIES}`; pass `0` to disable retries) or set `{ENV_MAX_RETRIES}=<COUNT>`."
                ))
            });
        }

        if let Some(raw_env) = env
            .get(ENV_MAX_RETRIES)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            return raw_env.parse::<u32>().map_err(|_| {
                CliError::usage(format!(
                    "Invalid retry count `{raw_env}` in `{ENV_MAX_RETRIES}`: expected a non-negative integer (`>= 0`).\n  \
                     Hint: Set `{ENV_MAX_RETRIES}={DEFAULT_MAX_RETRIES}` (or `0` to disable retries) or override via `--max-retries <COUNT>`."
                ))
            });
        }

        Ok(stored
            .and_then(|s| s.defaults.max_retries)
            .unwrap_or(DEFAULT_MAX_RETRIES))
    }

    fn resolve_max_backoff_ms(
        &self,
        env: &BTreeMap<String, String>,
        stored: Option<&StoredConfig>,
    ) -> Result<u64, CliError> {
        if let Some(raw_flag) = self.max_backoff_ms.as_deref() {
            return raw_flag.trim().parse::<u64>().map_err(|_| {
                CliError::usage(format!(
                    "Invalid `--max-backoff-ms` value `{raw_flag}`: expected a non-negative integer in milliseconds (`>= 0`).\n  \
                     Hint: Pass `--max-backoff-ms <MS>` (default: `{DEFAULT_MAX_BACKOFF_MS}`; pass `0` to fail fast on rate limits) or set `{ENV_MAX_BACKOFF_MS}=<MS>`."
                ))
            });
        }

        if let Some(raw_env) = env
            .get(ENV_MAX_BACKOFF_MS)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            return raw_env.parse::<u64>().map_err(|_| {
                CliError::usage(format!(
                    "Invalid backoff cap `{raw_env}` in `{ENV_MAX_BACKOFF_MS}`: expected a non-negative integer in milliseconds (`>= 0`).\n  \
                     Hint: Set `{ENV_MAX_BACKOFF_MS}={DEFAULT_MAX_BACKOFF_MS}` (or `0` to fail fast on rate limits) or override via `--max-backoff-ms <MS>`."
                ))
            });
        }

        Ok(stored
            .and_then(|s| s.defaults.max_backoff_ms)
            .unwrap_or(DEFAULT_MAX_BACKOFF_MS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_transport_args_specificity_cascade_and_actionable_validation_hints() {
        let empty_env = BTreeMap::new();
        let default_policy = ProviderTransportArgs::default()
            .resolve_retry_policy(&empty_env, None)
            .expect("default retry policy");
        assert_eq!(default_policy.max_retries, 2);
        assert_eq!(default_policy.max_backoff_ms, 90_000);

        let mut stored = StoredConfig::default();
        stored.defaults.max_retries = Some(4);
        stored.defaults.max_backoff_ms = Some(45_000);
        let from_store = ProviderTransportArgs::default()
            .resolve_retry_policy(&empty_env, Some(&stored))
            .expect("stored retry policy");
        assert_eq!(from_store, RetryPolicy::new(4, 45_000));

        let mut env = BTreeMap::new();
        env.insert(ENV_MAX_RETRIES.to_string(), "1".to_string());
        env.insert(ENV_MAX_BACKOFF_MS.to_string(), "15000".to_string());
        let from_env = ProviderTransportArgs::default()
            .resolve_retry_policy(&env, Some(&stored))
            .expect("env overrides stored");
        assert_eq!(from_env, RetryPolicy::new(1, 15_000));

        let explicit_zero = ProviderTransportArgs {
            timeout: None,
            max_retries: Some("0".to_string()),
            max_backoff_ms: Some("0".to_string()),
        }
        .resolve_retry_policy(&env, Some(&stored))
        .expect("CLI overrides env and accepts 0");
        assert_eq!(explicit_zero, RetryPolicy::new(0, 0));

        let bad_flag = ProviderTransportArgs {
            timeout: None,
            max_retries: Some("-1".to_string()),
            max_backoff_ms: None,
        }
        .resolve_retry_policy(&empty_env, None)
        .expect_err("negative --max-retries must fail");
        assert_eq!(bad_flag.exit_code(), 2);
        assert!(bad_flag.to_string().contains("Hint:"));

        let mut bad_env = BTreeMap::new();
        bad_env.insert(ENV_MAX_BACKOFF_MS.to_string(), "not-a-number".to_string());
        let bad_env_err = ProviderTransportArgs::default()
            .resolve_retry_policy(&bad_env, None)
            .expect_err("invalid CANON_CLERK_MAX_BACKOFF_MS must fail");
        assert_eq!(bad_env_err.exit_code(), 2);
        assert!(bad_env_err.to_string().contains("Hint:"));
    }

    #[test]
    fn test_provider_transport_args_timeout_cascade_and_profile_defaults() {
        let empty_env = BTreeMap::new();
        let default_args = ProviderTransportArgs::default();
        assert_eq!(
            default_args
                .resolve_timeout_ms_for_model(&empty_env, None, "google:gemini-decision-1")
                .expect("cloud default timeout"),
            DEFAULT_GENERATION_TIMEOUT_MS
        );
        assert_eq!(
            default_args
                .resolve_timeout_ms_for_model(&empty_env, None, "local:kev-4b")
                .expect("local default timeout"),
            DEFAULT_LOCAL_GENERATION_TIMEOUT_MS
        );

        let mut stored = StoredConfig::default();
        stored.defaults.timeout_ms = Some(120_000);
        assert_eq!(
            default_args
                .resolve_timeout_ms_for_model(&empty_env, Some(&stored), "local:kev-4b")
                .expect("stored defaults.timeoutMs"),
            120_000
        );

        let mut env = BTreeMap::new();
        env.insert(ENV_TIMEOUT_MS.to_string(), "95000".to_string());
        assert_eq!(
            default_args
                .resolve_timeout_ms_for_model(&env, Some(&stored), "google:gemini-4-carbon")
                .expect("env overrides stored"),
            95_000
        );

        let cli_override = ProviderTransportArgs {
            timeout: Some("240000".to_string()),
            ..ProviderTransportArgs::default()
        };
        assert_eq!(
            cli_override
                .resolve_timeout_ms_for_model(&env, Some(&stored), "local:kev-4b")
                .expect("cli overrides env"),
            240_000
        );

        let zero_err = ProviderTransportArgs {
            timeout: Some("0".to_string()),
            ..ProviderTransportArgs::default()
        }
        .resolve_explicit_timeout_ms(&empty_env, None)
        .expect_err("zero --timeout must fail");
        assert_eq!(zero_err.exit_code(), 2);
        assert!(zero_err.to_string().contains("Hint:"));
    }
}
