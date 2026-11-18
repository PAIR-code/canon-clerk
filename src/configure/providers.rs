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

//! Declarative provider capability registry, active-provider selector cascade, and closure-exact
//! credential verification (`docs/architecture/nodes/configure.md` §4).

use crate::caseload::{
    CaseloadConfig, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, DEFAULT_GOOGLE_GENERATOR_MODEL,
    DEFAULT_PROVIDER, DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL, HeuristicNode, TYPESAFE_PROVIDER,
};
use crate::configure::{
    ConfigureError, ConfigureOptions, ENV_GEMINI_API_KEY, ENV_PROVIDER, ENV_TYPESAFE_API_KEY,
    StoredConfig, model_provider, resolve_non_empty,
};
use std::collections::BTreeSet;

/// Domain evaluation capability port satisfied by a model (`docs/architecture/overview.md` §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCapability {
    /// Parallel binary-question (`Noul`) classifier valid on `apprise`, `docket`, `admit`, and `vet`.
    Discriminator,
    /// Structured JSON generative model valid on `audit` as well as screening/vetting overrides.
    Generator,
}

/// Declarative capability descriptor mapping a supported provider slug to its credential sources
/// and default per-capability models (`docs/architecture/nodes/configure.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCapability {
    /// Canonical provider slug (`"google"`, `"typesafe"`).
    pub slug: &'static str,
    /// Human-readable provider display name (`"Google Gemini"`, `"TypeSafe AI"`).
    pub display_name: &'static str,
    /// CLI flag overriding the provider's API key (`"--gemini-api-key"`, `"--typesafe-api-key"`).
    pub cli_flag: &'static str,
    /// Environment variable providing the provider's API key (`"GEMINI_API_KEY"`, `"TYPESAFE_API_KEY"`).
    pub env_var: &'static str,
    /// Canonical `~/.config/canon-clerk/config.json` key path (`"providers.<slug>.apiKey"`).
    pub config_key_path: &'static str,
    /// Default canonical `Discriminator` `<provider>:<model>` specifier, if offered by this provider.
    pub default_discriminator: Option<&'static str>,
    /// Default canonical `Generator` `<provider>:<model>` specifier, if offered by this provider.
    pub default_generator: Option<&'static str>,
}

impl ProviderCapability {
    /// Returns the default canonical `<provider>:<model>` specifier for `node` (`configure.md` §4.2):
    /// - Screening & vetting nodes (`apprise`, `docket`, `admit`, `vet`): `default_discriminator` else `default_generator`.
    /// - Judicial adjudication node (`audit`): `default_generator`.
    pub const fn default_model_for(self, node: HeuristicNode) -> Option<&'static str> {
        match node {
            HeuristicNode::Apprise
            | HeuristicNode::Docket
            | HeuristicNode::Admit
            | HeuristicNode::Vet => match self.default_discriminator {
                Some(disc) => Some(disc),
                None => self.default_generator,
            },
            HeuristicNode::Audit => self.default_generator,
        }
    }
}

/// Authoritative built-in provider capability registry (`docs/architecture/nodes/configure.md` §4).
pub const PROVIDER_CAPABILITIES: &[ProviderCapability] = &[
    ProviderCapability {
        slug: DEFAULT_PROVIDER,
        display_name: "Google Gemini",
        cli_flag: "--gemini-api-key",
        env_var: ENV_GEMINI_API_KEY,
        config_key_path: "providers.google.apiKey",
        default_discriminator: Some(DEFAULT_GOOGLE_DISCRIMINATOR_MODEL),
        default_generator: Some(DEFAULT_GOOGLE_GENERATOR_MODEL),
    },
    ProviderCapability {
        slug: TYPESAFE_PROVIDER,
        display_name: "TypeSafe AI",
        cli_flag: "--typesafe-api-key",
        env_var: ENV_TYPESAFE_API_KEY,
        config_key_path: "providers.typesafe.apiKey",
        default_discriminator: Some(DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL),
        default_generator: None,
    },
];

/// Looks up the declarative [`ProviderCapability`] entry for a canonical provider slug.
pub fn provider_capability(slug: &str) -> Option<&'static ProviderCapability> {
    PROVIDER_CAPABILITIES.iter().find(|cap| cap.slug == slug)
}

/// Resolves the active default provider slug deterministically without ambient credential sniffing:
/// `--provider <slug>` ≻ `CANON_CLERK_PROVIDER` ≻ `defaults.provider` ≻ upstream `caseload.config.provider` ≻ `"google"`.
pub(crate) fn resolve_active_provider(
    options: &ConfigureOptions,
    stored: Option<&StoredConfig>,
    existing: Option<&CaseloadConfig>,
) -> Result<&'static ProviderCapability, ConfigureError> {
    let raw_slug = resolve_non_empty(
        options.provider.as_deref(),
        options.env.get(ENV_PROVIDER).map(String::as_str),
    )
    .or_else(|| stored.and_then(StoredConfig::default_provider))
    .or_else(|| {
        existing
            .map(|c| c.provider.trim())
            .filter(|s| !s.is_empty())
    })
    .unwrap_or(DEFAULT_PROVIDER);

    provider_capability(raw_slug).ok_or_else(|| ConfigureError::UnsupportedProviderSelector {
        provider: raw_slug.to_string(),
    })
}

/// Resolves the non-empty API key for `cap` across CLI flags, environment variables, and `config.json`.
fn resolve_provider_credential<'a>(
    cap: &ProviderCapability,
    options: &'a ConfigureOptions,
) -> Option<&'a str> {
    resolve_non_empty(
        options.cli_api_key_for(cap.slug),
        options.env.get(cap.env_var).map(String::as_str),
    )
    .or_else(|| {
        options
            .stored_config
            .as_ref()
            .and_then(|s| s.provider_api_key(cap.slug))
    })
}

/// Resolves and validates the API key for `slug` against [`PROVIDER_CAPABILITIES`].
pub(crate) fn resolve_provider_api_key(
    slug: &str,
    options: &ConfigureOptions,
) -> Result<String, ConfigureError> {
    if let Some(cap) = provider_capability(slug)
        && let Some(secret) = resolve_provider_credential(cap, options)
    {
        return Ok(secret.to_string());
    }
    Err(missing_provider_key_error(slug))
}

fn missing_provider_key_error(slug: &str) -> ConfigureError {
    match slug {
        DEFAULT_PROVIDER => ConfigureError::MissingApiKey,
        TYPESAFE_PROVIDER => ConfigureError::MissingTypesafeApiKey,
        other => ConfigureError::UnsupportedProviderSelector {
            provider: other.to_string(),
        },
    }
}

/// Verifies credentials strictly for the unique providers referenced by `models_to_verify` in canonical order.
pub(crate) fn verify_models_credentials<'a, I>(
    models_to_verify: I,
    options: &ConfigureOptions,
) -> Result<(), ConfigureError>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut seen = BTreeSet::new();
    for model in models_to_verify {
        let slug = model_provider(model);
        if seen.insert(slug) {
            resolve_provider_api_key(slug, options)?;
        }
    }
    Ok(())
}

/// Returns the deduplicated, lexicographically sorted credential environment variable identifiers
/// (`"GEMINI_API_KEY"`, `"TYPESAFE_API_KEY"`, etc.) verified for the provisioned nodes on `config`.
pub(crate) fn verified_credential_env_vars(config: &CaseloadConfig) -> Vec<&'static str> {
    let mut vars = BTreeSet::new();
    for node_cfg in config.nodes.values() {
        let slug = model_provider(&node_cfg.model);
        if let Some(cap) = provider_capability(slug) {
            vars.insert(cap.env_var);
        }
    }
    vars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure::{StoredDefaultsConfig, StoredProviderConfig};
    use std::collections::BTreeMap;

    mod provider_selector_cascade {
        use super::*;

        #[test]
        fn test_provider_resolves_deterministically_without_ambient_api_key_sniffing() {
            // Even when ONLY TYPESAFE_API_KEY is exported in env, active provider stays `google` unless selected!
            let mut ts_only_env = BTreeMap::new();
            ts_only_env.insert(ENV_TYPESAFE_API_KEY.to_string(), "ts-key".to_string());
            let cap = resolve_active_provider(
                &ConfigureOptions {
                    env: ts_only_env.clone(),
                    ..ConfigureOptions::default()
                },
                None,
                None,
            )
            .expect("resolve default provider");
            assert_eq!(cap.slug, DEFAULT_PROVIDER);

            // Stored `defaults.provider = "typesafe"` switches provider
            let stored = StoredConfig {
                defaults: StoredDefaultsConfig {
                    provider: Some("typesafe".to_string()),
                    ..StoredDefaultsConfig::default()
                },
                ..StoredConfig::default()
            };
            assert_eq!(
                resolve_active_provider(&ConfigureOptions::default(), Some(&stored), None)
                    .expect("stored provider")
                    .slug,
                TYPESAFE_PROVIDER
            );

            // `CANON_CLERK_PROVIDER` overrides `defaults.provider`
            let mut env_override = BTreeMap::new();
            env_override.insert(ENV_PROVIDER.to_string(), "google".to_string());
            assert_eq!(
                resolve_active_provider(
                    &ConfigureOptions {
                        env: env_override.clone(),
                        ..ConfigureOptions::default()
                    },
                    Some(&stored),
                    None,
                )
                .expect("env overrides stored")
                .slug,
                DEFAULT_PROVIDER
            );

            // CLI `--provider` overrides `CANON_CLERK_PROVIDER`
            assert_eq!(
                resolve_active_provider(
                    &ConfigureOptions {
                        provider: Some("typesafe".to_string()),
                        env: env_override,
                        ..ConfigureOptions::default()
                    },
                    Some(&stored),
                    None,
                )
                .expect("cli overrides env")
                .slug,
                TYPESAFE_PROVIDER
            );
        }

        #[test]
        fn test_provider_capability_defaults_and_credential_resolution() {
            let google = provider_capability(DEFAULT_PROVIDER).expect("google");
            assert_eq!(
                google.default_model_for(HeuristicNode::Docket),
                Some(DEFAULT_GOOGLE_DISCRIMINATOR_MODEL)
            );
            assert_eq!(
                google.default_model_for(HeuristicNode::Audit),
                Some(DEFAULT_GOOGLE_GENERATOR_MODEL)
            );

            let typesafe = provider_capability(TYPESAFE_PROVIDER).expect("typesafe");
            assert_eq!(
                typesafe.default_model_for(HeuristicNode::Vet),
                Some(DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL)
            );
            assert_eq!(typesafe.default_model_for(HeuristicNode::Audit), None);

            let mut providers = BTreeMap::new();
            providers.insert(
                TYPESAFE_PROVIDER.to_string(),
                StoredProviderConfig {
                    api_key: Some("stored-ts-key".to_string()),
                },
            );
            let opts = ConfigureOptions {
                stored_config: Some(StoredConfig {
                    providers,
                    ..StoredConfig::default()
                }),
                ..ConfigureOptions::default()
            };
            assert_eq!(
                resolve_provider_api_key(TYPESAFE_PROVIDER, &opts).expect("stored key"),
                "stored-ts-key"
            );
        }
    }
}
