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

//! Core domain execution engine for the `configure` Caseload DAG stage (`docs/architecture/nodes/configure.md`).

pub mod store;

pub use store::{
    StoredConfig, StoredProviderConfig, StoredTierModel, load_host_config_file,
    resolve_default_config_path,
};

use crate::caseload::{
    Caseload, CaseloadConfig, DEFAULT_AUDITOR_MODEL, DEFAULT_PROVIDER, DEFAULT_SCREENER_MODEL,
};
use std::collections::BTreeMap;
use std::fmt;

/// Environment variable name for the Google Gemini API key.
pub const ENV_GEMINI_API_KEY: &str = "GEMINI_API_KEY";

/// Environment variable name for overriding the screener model specifier.
pub const ENV_SCREENER_MODEL: &str = "CANON_CLERK_SCREENER_MODEL";

/// Environment variable name for overriding the auditor model specifier.
pub const ENV_AUDITOR_MODEL: &str = "CANON_CLERK_AUDITOR_MODEL";

/// Environment variable name for overriding the maximum reasoning budget in tokens.
pub const ENV_REASONING_BUDGET: &str = "CANON_CLERK_REASONING_BUDGET";

/// Canonical Google Gemini Flash model specifier expanded from the `flash` shorthand alias.
pub const DEFAULT_FLASH_MODEL: &str = "google:gemini-flash-latest";

/// Domain input parameters, environment snapshot, and optional user config store for `configure`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigureOptions {
    /// Explicit screener model override (`--screener-model`).
    pub screener_model: Option<String>,
    /// Explicit auditor model override (`--auditor-model`).
    pub auditor_model: Option<String>,
    /// Explicit reasoning budget override in tokens (`--reasoning-budget`).
    pub reasoning_budget: Option<u32>,
    /// Explicit Google Gemini API key override (`--gemini-api-key`).
    pub gemini_api_key: Option<String>,
    /// Snapshot of runtime environment variables injected by the driving adapter.
    pub env: BTreeMap<String, String>,
    /// Optional parsed user-level configuration (`~/.config/canon-clerk/config.json`).
    pub stored_config: Option<StoredConfig>,
}

impl ConfigureOptions {
    /// Returns `true` when any explicit CLI or programmatic override flag was supplied.
    pub fn has_explicit_overrides(&self) -> bool {
        self.screener_model.is_some()
            || self.auditor_model.is_some()
            || self.reasoning_budget.is_some()
            || self.gemini_api_key.is_some()
    }
}

/// Domain errors produced during `configure` execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigureError {
    /// Required `GEMINI_API_KEY` credential is missing from flags, environment, and user config.
    MissingApiKey,
    /// Model specifier uses an unsupported provider prefix (only `google` is supported).
    UnsupportedProvider { provider: String, specifier: String },
    /// Model specifier is empty, contains whitespace, or does not name a valid Gemini model.
    InvalidModelSpecifier { specifier: String, reason: String },
    /// Reasoning budget environment variable cannot be parsed as a non-negative integer.
    InvalidReasoningBudget { raw: String },
    /// User configuration file has group or world access permissions on a POSIX host.
    InsecureConfigPermissions { path: String, mode_octal: String },
    /// User configuration file resides inside the repository workspace directory tree.
    ConfigInsideWorkspace { path: String },
    /// User configuration file could not be read or parsed as valid JSON.
    MalformedConfigFile { path: String, message: String },
}

impl fmt::Display for ConfigureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingApiKey => write!(
                f,
                "Missing required Google Gemini API key (`{ENV_GEMINI_API_KEY}`).\n  \
                 Hint: Export `{ENV_GEMINI_API_KEY}=\"<your-api-key>\"`, configure \
                 `providers.google.apiKey` in `~/.config/canon-clerk/config.json` (mode `0600`), \
                 or pass `--gemini-api-key <KEY>`."
            ),
            Self::UnsupportedProvider {
                provider,
                specifier,
            } => write!(
                f,
                "Unsupported model provider `{provider}` in specifier `{specifier}`.\n  \
                 Hint: Canon Clerk currently supports only the `google` provider \
                 (e.g. `{DEFAULT_SCREENER_MODEL}` or `{DEFAULT_AUDITOR_MODEL}`)."
            ),
            Self::InvalidModelSpecifier { specifier, reason } => write!(
                f,
                "Invalid model specifier `{specifier}`: {reason}\n  \
                 Hint: Specify a canonical Google Gemini model (e.g. `{DEFAULT_SCREENER_MODEL}`, \
                 `{DEFAULT_AUDITOR_MODEL}`, or shorthand aliases `flash-lite`, `flash`, `pro`)."
            ),
            Self::InvalidReasoningBudget { raw } => write!(
                f,
                "Invalid reasoning budget `{raw}`: expected a non-negative integer token count.\n  \
                 Hint: Pass an integer token ceiling via `--reasoning-budget <TOKENS>` \
                 or `{ENV_REASONING_BUDGET}=<TOKENS>`."
            ),
            Self::InsecureConfigPermissions { path, mode_octal } => write!(
                f,
                "Configuration file `{path}` has insecure permissions (`{mode_octal}`).\n  \
                 Hint: Restrict file access to owner-only by running: `chmod 600 \"{path}\"`."
            ),
            Self::ConfigInsideWorkspace { path } => write!(
                f,
                "Configuration file `{path}` resides inside the repository workspace.\n  \
                 Hint: Store credentials exclusively outside the repository in \
                 `~/.config/canon-clerk/config.json` (mode `0600`) or via `{ENV_GEMINI_API_KEY}`."
            ),
            Self::MalformedConfigFile { path, message } => write!(
                f,
                "Failed to parse configuration file `{path}`: {message}\n  \
                 Hint: Ensure `{path}` contains valid JSON matching the Canon Clerk config schema."
            ),
        }
    }
}

impl std::error::Error for ConfigureError {}

/// Normalizes a raw model alias or specifier into a canonical `google:<model>` identifier.
///
/// Supported forms:
/// - Shorthand aliases: `flash-lite`, `flash`, `pro` (with or without `google:` prefix)
/// - Unqualified Gemini models: `gemini-*` -> `google:gemini-*`
/// - Canonical Google specifiers: `google:gemini-*`
pub fn normalize_model_specifier(raw: &str) -> Result<String, ConfigureError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ConfigureError::InvalidModelSpecifier {
            specifier: raw.to_string(),
            reason: "model specifier cannot be empty.".to_string(),
        });
    }

    if trimmed.chars().any(char::is_whitespace) {
        return Err(ConfigureError::InvalidModelSpecifier {
            specifier: raw.to_string(),
            reason: "model specifier must not contain whitespace.".to_string(),
        });
    }

    let (provider, model_part) = match trimmed.split_once(':') {
        Some((prov, model)) => {
            if prov.is_empty() || model.is_empty() {
                return Err(ConfigureError::InvalidModelSpecifier {
                    specifier: raw.to_string(),
                    reason: "provider and model segments around `:` must both be non-empty."
                        .to_string(),
                });
            }
            if model.contains(':') {
                return Err(ConfigureError::InvalidModelSpecifier {
                    specifier: raw.to_string(),
                    reason: "model specifier must contain at most one `:` delimiter.".to_string(),
                });
            }
            (prov, model)
        }
        None => (DEFAULT_PROVIDER, trimmed),
    };

    if provider != DEFAULT_PROVIDER {
        return Err(ConfigureError::UnsupportedProvider {
            provider: provider.to_string(),
            specifier: trimmed.to_string(),
        });
    }

    match model_part {
        "flash-lite" => Ok(DEFAULT_SCREENER_MODEL.to_string()),
        "flash" => Ok(DEFAULT_FLASH_MODEL.to_string()),
        "pro" => Ok(DEFAULT_AUDITOR_MODEL.to_string()),
        name if is_valid_gemini_model_name(name) => Ok(format!("{DEFAULT_PROVIDER}:{name}")),
        _ => Err(ConfigureError::InvalidModelSpecifier {
            specifier: raw.to_string(),
            reason: format!(
                "unrecognized Google model `{model_part}` (expected `gemini-*` or alias `flash-lite`, `flash`, `pro`)."
            ),
        }),
    }
}

fn is_valid_gemini_model_name(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix("gemini-") else {
        return false;
    };
    !suffix.is_empty()
        && suffix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_')
}

/// Executes the pure, deterministic `configure` stage and populates `.config` on the [`Caseload`].
///
/// Precedence hierarchy:
/// 1. Explicit CLI flags / programmatic overrides
/// 2. Environment variables (`CANON_CLERK_*`, `GEMINI_API_KEY`)
/// 3. User-level configuration file (`~/.config/canon-clerk/config.json`)
/// 4. Existing `caseload.config` or static defaults
pub fn execute_configure(
    options: &ConfigureOptions,
    caseload: Option<Caseload>,
) -> Result<Caseload, ConfigureError> {
    let caseload = caseload.unwrap_or_default();

    if caseload.config.is_some() && !options.has_explicit_overrides() {
        return Ok(caseload);
    }

    let stored = options.stored_config.as_ref();

    let api_key = resolve_non_empty(
        options.gemini_api_key.as_deref(),
        options.env.get(ENV_GEMINI_API_KEY).map(String::as_str),
    )
    .or_else(|| stored.and_then(StoredConfig::google_api_key))
    .ok_or(ConfigureError::MissingApiKey)?;
    let _verified_key_len = api_key.len();

    let existing_config = caseload.config.as_ref();
    let stored_screener = stored.and_then(StoredConfig::screener_specifier);
    let stored_auditor = stored.and_then(StoredConfig::auditor_specifier);

    let raw_screener = resolve_non_empty(
        options.screener_model.as_deref(),
        options.env.get(ENV_SCREENER_MODEL).map(String::as_str),
    )
    .or_else(|| {
        stored_screener
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    })
    .or_else(|| existing_config.map(|c| c.screener_model.as_str()))
    .unwrap_or(DEFAULT_SCREENER_MODEL);
    let screener_model = normalize_model_specifier(raw_screener)?;

    let raw_auditor = resolve_non_empty(
        options.auditor_model.as_deref(),
        options.env.get(ENV_AUDITOR_MODEL).map(String::as_str),
    )
    .or_else(|| {
        stored_auditor
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    })
    .or_else(|| existing_config.map(|c| c.auditor_model.as_str()))
    .unwrap_or(DEFAULT_AUDITOR_MODEL);
    let auditor_model = normalize_model_specifier(raw_auditor)?;

    let reasoning_budget = resolve_reasoning_budget(options, stored, existing_config)?;

    let config = CaseloadConfig {
        screener_model,
        auditor_model,
        reasoning_budget,
    };

    Ok(caseload.with_config(config))
}

fn resolve_non_empty<'a>(
    override_val: Option<&'a str>,
    env_val: Option<&'a str>,
) -> Option<&'a str> {
    override_val
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| env_val.map(str::trim).filter(|s| !s.is_empty()))
}

fn resolve_reasoning_budget(
    options: &ConfigureOptions,
    stored: Option<&StoredConfig>,
    existing_config: Option<&CaseloadConfig>,
) -> Result<Option<u32>, ConfigureError> {
    if let Some(budget) = options.reasoning_budget {
        return Ok(Some(budget));
    }

    if let Some(raw_env) = options
        .env
        .get(ENV_REASONING_BUDGET)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let parsed =
            raw_env
                .parse::<u32>()
                .map_err(|_| ConfigureError::InvalidReasoningBudget {
                    raw: raw_env.to_string(),
                })?;
        return Ok(Some(parsed));
    }

    if let Some(stored_budget) = stored.and_then(|s| s.reasoning_budget) {
        return Ok(Some(stored_budget));
    }

    Ok(existing_config.and_then(|c| c.reasoning_budget))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{CaseloadDiscovery, CaseloadIntake, CaseloadValidation};

    fn env_with_key() -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        env.insert(
            ENV_GEMINI_API_KEY.to_string(),
            "test-synthetic-gemini-key".to_string(),
        );
        env
    }

    mod model_normalization {
        use super::*;

        #[test]
        fn test_shorthand_aliases_normalize_to_canonical_google_specifiers() {
            assert_eq!(
                normalize_model_specifier("flash-lite").expect("flash-lite alias"),
                DEFAULT_SCREENER_MODEL
            );
            assert_eq!(
                normalize_model_specifier("google:flash-lite").expect("prefixed flash-lite"),
                DEFAULT_SCREENER_MODEL
            );
            assert_eq!(
                normalize_model_specifier("flash").expect("flash alias"),
                DEFAULT_FLASH_MODEL
            );
            assert_eq!(
                normalize_model_specifier("pro").expect("pro alias"),
                DEFAULT_AUDITOR_MODEL
            );
            assert_eq!(
                normalize_model_specifier("google:pro").expect("prefixed pro"),
                DEFAULT_AUDITOR_MODEL
            );
        }

        #[test]
        fn test_unqualified_and_canonical_gemini_models_normalize() {
            assert_eq!(
                normalize_model_specifier("gemini-2.5-pro").expect("unqualified gemini"),
                "google:gemini-2.5-pro"
            );
            assert_eq!(
                normalize_model_specifier("  google:gemini-2.5-flash-lite  ")
                    .expect("trimmed canonical"),
                "google:gemini-2.5-flash-lite"
            );
        }

        #[test]
        fn test_unsupported_providers_rejected_with_remediation_hint() {
            let err = normalize_model_specifier("openai:gpt-4o").expect_err("openai must fail");
            assert_eq!(
                err,
                ConfigureError::UnsupportedProvider {
                    provider: "openai".to_string(),
                    specifier: "openai:gpt-4o".to_string(),
                }
            );
            assert!(err.to_string().contains("Hint:"));
        }

        #[test]
        fn test_malformed_or_unrecognized_specifiers_rejected() {
            for invalid in [
                "",
                "   ",
                ":gemini-pro-latest",
                "google:",
                "google:gemini:pro",
                "google:gemini pro",
                "google:claude-3-5-sonnet",
                "unknown-model",
                "gemini-",
            ] {
                let err = normalize_model_specifier(invalid)
                    .expect_err(&format!("`{invalid}` must be rejected"));
                assert!(matches!(err, ConfigureError::InvalidModelSpecifier { .. }));
                assert!(err.to_string().contains("Hint:"));
            }
        }
    }

    mod precedence_and_resolution {
        use super::*;

        #[test]
        fn test_defaults_applied_when_only_api_key_is_present() {
            let opts = ConfigureOptions {
                env: env_with_key(),
                ..ConfigureOptions::default()
            };
            let caseload = execute_configure(&opts, None).expect("configure should succeed");
            let cfg = caseload.config.expect("config populated");
            assert_eq!(cfg.screener_model, DEFAULT_SCREENER_MODEL);
            assert_eq!(cfg.auditor_model, DEFAULT_AUDITOR_MODEL);
            assert_eq!(cfg.reasoning_budget, None);
        }

        #[test]
        fn test_stored_config_provides_credentials_and_tier_defaults_below_env_and_flags() {
            let mut providers = BTreeMap::new();
            providers.insert(
                "google".to_string(),
                StoredProviderConfig {
                    api_key: Some("stored-google-key".to_string()),
                },
            );
            let stored = StoredConfig {
                providers,
                screener_model: Some(StoredTierModel::Specifier("flash".to_string())),
                auditor_model: Some(StoredTierModel::Specifier("gemini-2.5-pro".to_string())),
                reasoning_budget: Some(2048),
            };

            // 1. Stored config used when env and CLI flags are absent
            let from_store = execute_configure(
                &ConfigureOptions {
                    stored_config: Some(stored.clone()),
                    ..ConfigureOptions::default()
                },
                None,
            )
            .expect("configure from stored config");
            assert_eq!(
                from_store.config,
                Some(CaseloadConfig {
                    screener_model: DEFAULT_FLASH_MODEL.to_string(),
                    auditor_model: "google:gemini-2.5-pro".to_string(),
                    reasoning_budget: Some(2048),
                })
            );

            // 2. Environment variables override stored config
            let mut env = BTreeMap::new();
            env.insert(ENV_SCREENER_MODEL.to_string(), "flash-lite".to_string());
            env.insert(ENV_REASONING_BUDGET.to_string(), "4096".to_string());
            let from_env = execute_configure(
                &ConfigureOptions {
                    env,
                    stored_config: Some(stored),
                    ..ConfigureOptions::default()
                },
                None,
            )
            .expect("env overrides stored config");
            assert_eq!(
                from_env.config,
                Some(CaseloadConfig {
                    screener_model: DEFAULT_SCREENER_MODEL.to_string(),
                    auditor_model: "google:gemini-2.5-pro".to_string(),
                    reasoning_budget: Some(4096),
                })
            );
        }

        #[test]
        fn test_explicit_cli_overrides_take_precedence_over_environment_variables() {
            let mut env = env_with_key();
            env.insert(
                ENV_SCREENER_MODEL.to_string(),
                "gemini-2.0-flash".to_string(),
            );
            env.insert(ENV_AUDITOR_MODEL.to_string(), "gemini-2.0-pro".to_string());
            env.insert(ENV_REASONING_BUDGET.to_string(), "1024".to_string());

            let opts = ConfigureOptions {
                screener_model: Some("flash-lite".to_string()),
                auditor_model: Some("pro".to_string()),
                reasoning_budget: Some(4096),
                gemini_api_key: None,
                env,
                stored_config: None,
            };
            let caseload = execute_configure(&opts, None).expect("configure with overrides");
            let cfg = caseload.config.expect("config populated");
            assert_eq!(cfg.screener_model, DEFAULT_SCREENER_MODEL);
            assert_eq!(cfg.auditor_model, DEFAULT_AUDITOR_MODEL);
            assert_eq!(cfg.reasoning_budget, Some(4096));
        }

        #[test]
        fn test_invalid_reasoning_budget_env_fails_with_hint() {
            let mut env = env_with_key();
            env.insert(ENV_REASONING_BUDGET.to_string(), "not-a-number".to_string());
            let opts = ConfigureOptions {
                env,
                ..ConfigureOptions::default()
            };
            let err = execute_configure(&opts, None).expect_err("invalid budget must fail");
            assert_eq!(
                err,
                ConfigureError::InvalidReasoningBudget {
                    raw: "not-a-number".to_string()
                }
            );
            assert!(err.to_string().contains("Hint:"));
        }
    }

    mod credential_validation_and_security {
        use super::*;

        #[test]
        fn test_missing_or_blank_api_key_fails_fast_with_remediation() {
            let empty_opts = ConfigureOptions::default();
            let err = execute_configure(&empty_opts, None).expect_err("missing key must fail");
            assert_eq!(err, ConfigureError::MissingApiKey);
            assert!(err.to_string().contains(ENV_GEMINI_API_KEY));
            assert!(err.to_string().contains("Hint:"));

            let mut blank_env = BTreeMap::new();
            blank_env.insert(ENV_GEMINI_API_KEY.to_string(), "   ".to_string());
            let blank_opts = ConfigureOptions {
                env: blank_env,
                ..ConfigureOptions::default()
            };
            assert_eq!(
                execute_configure(&blank_opts, None),
                Err(ConfigureError::MissingApiKey)
            );
        }

        #[test]
        fn test_api_key_never_persisted_in_serialized_caseload() {
            let secret = "synthetic-secret-value-do-not-persist-12345";
            let opts = ConfigureOptions {
                gemini_api_key: Some(secret.to_string()),
                ..ConfigureOptions::default()
            };
            let caseload = execute_configure(&opts, None).expect("configure with flag key");
            let serialized = serde_json::to_string(&caseload).expect("serialize caseload");
            assert!(
                !serialized.contains(secret),
                "Caseload JSON must never contain raw API key material"
            );
        }
    }

    mod fast_forwarding_and_immutability {
        use super::*;

        #[test]
        fn test_fast_forwards_when_config_already_present_and_no_overrides() {
            let preconfigured = Caseload::new().with_config(CaseloadConfig {
                screener_model: "google:gemini-2.5-flash-lite".to_string(),
                auditor_model: "google:gemini-2.5-pro".to_string(),
                reasoning_budget: Some(2048),
            });

            let opts = ConfigureOptions::default();
            let result =
                execute_configure(&opts, Some(preconfigured.clone())).expect("fast-forward");
            assert_eq!(result, preconfigured);
        }

        #[test]
        fn test_preserves_branch_a_namespaces_on_incoming_caseload() {
            let upstream = Caseload::new()
                .with_intake(CaseloadIntake {
                    pr_title: Some("feat: branch a".to_string()),
                    ..CaseloadIntake::default()
                })
                .with_discovery(CaseloadDiscovery {
                    candidate_canons: vec![".canons/cli/rule.md".to_string()],
                    ..CaseloadDiscovery::default()
                })
                .with_validation(CaseloadValidation::default());

            let opts = ConfigureOptions {
                env: env_with_key(),
                ..ConfigureOptions::default()
            };
            let enriched =
                execute_configure(&opts, Some(upstream.clone())).expect("configure upstream");
            assert_eq!(enriched.intake, upstream.intake);
            assert_eq!(enriched.discovery, upstream.discovery);
            assert_eq!(enriched.validation, upstream.validation);
            assert_eq!(enriched.config, Some(CaseloadConfig::default()));
        }
    }
}
