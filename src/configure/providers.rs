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

//! Declarative provider capability registry, custom `providers.<slug>` profile resolution,
//! `baseUrl`/credential cascades, and unified [`ProviderConnection`] construction (`configure.md` §3–4).

use crate::caseload::{
    CaseloadConfig, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, DEFAULT_GOOGLE_GENERATOR_MODEL,
    DEFAULT_LOCAL_DISCRIMINATOR_MODEL, DEFAULT_PROVIDER, DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL,
    HeuristicNode, LOCAL_PROVIDER, TYPESAFE_PROVIDER,
};
use crate::configure::profile_schema::{
    build_custom_profile, merge_builtin_profile, split_and_check_specifier_syntax,
};
use crate::configure::protocol::{
    AuthPolicy, DEFAULT_GEMINI_BASE_URL, DEFAULT_LOCAL_BASE_URL, DEFAULT_TYPESAFE_BASE_URL,
    MissingCredentialSpec, ProtocolDriver, ProviderAuth, ProviderConnection,
    is_valid_provider_slug, slug_to_upper_env_stem, validate_http_base_url,
};
use crate::configure::{
    ConfigureError, ConfigureOptions, ENV_GEMINI_API_KEY, ENV_GEMINI_BASE_URL, ENV_LOCAL_API_KEY,
    ENV_LOCAL_BASE_URL, ENV_PROVIDER, ENV_TYPESAFE_API_KEY, ENV_TYPESAFE_BASE_URL, StoredConfig,
    model_provider, resolve_non_empty,
};
use std::collections::{BTreeMap, BTreeSet};

/// Domain evaluation capability port satisfied by a model (`docs/architecture/overview.md` §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCapability {
    /// Parallel binary-question (`Noul`) classifier valid on `apprise`, `docket`, `admit`, and `vet`.
    Discriminator,
    /// Structured JSON generative model valid on `audit` as well as screening/vetting overrides.
    Generator,
}

/// Declarative descriptor for a built-in provider profile (`docs/architecture/nodes/configure.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCapability {
    /// Canonical provider slug (`"google"`, `"typesafe"`).
    pub slug: &'static str,
    /// Human-readable provider display name (`"Google Gemini"`, `"TypeSafe AI"`).
    pub display_name: &'static str,
    /// Wire protocol driver (`Gemini` or `SystemOne`).
    pub protocol: ProtocolDriver,
    /// Credential enforcement policy.
    pub auth_policy: AuthPolicy,
    /// Default `http://` or `https://` endpoint base URL.
    pub default_base_url: &'static str,
    /// CLI flag overriding the provider's API key (`"--gemini-api-key"`, `"--typesafe-api-key"`).
    pub cli_flag: &'static str,
    /// Primary environment variable providing the provider's API key (`"GEMINI_API_KEY"`, `"TYPESAFE_API_KEY"`).
    pub env_var: &'static str,
    /// CLI flag overriding the provider's base URL (`"--gemini-base-url"`, `"--typesafe-base-url"`).
    pub cli_base_url_flag: &'static str,
    /// Primary environment variable overriding the provider's base URL (`"GEMINI_BASE_URL"`, `"TYPESAFE_BASE_URL"`).
    pub base_url_env_var: &'static str,
    /// Canonical `~/.config/canon-clerk/config.json` key path (`"providers.<slug>.apiKey"`).
    pub config_key_path: &'static str,
    /// Default canonical `Discriminator` `<provider>:<model>` specifier, if offered by this provider.
    pub default_discriminator: Option<&'static str>,
    /// Default canonical `Generator` `<provider>:<model>` specifier, if offered by this provider.
    pub default_generator: Option<&'static str>,
}

impl ProviderCapability {
    /// Returns the default canonical `<provider>:<model>` specifier for `node` (`configure.md` §4.2).
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
        protocol: ProtocolDriver::Gemini,
        auth_policy: AuthPolicy::RequiredApiKey,
        default_base_url: DEFAULT_GEMINI_BASE_URL,
        cli_flag: "--gemini-api-key",
        env_var: ENV_GEMINI_API_KEY,
        cli_base_url_flag: "--gemini-base-url",
        base_url_env_var: ENV_GEMINI_BASE_URL,
        config_key_path: "providers.google.apiKey",
        default_discriminator: Some(DEFAULT_GOOGLE_DISCRIMINATOR_MODEL),
        default_generator: Some(DEFAULT_GOOGLE_GENERATOR_MODEL),
    },
    ProviderCapability {
        slug: TYPESAFE_PROVIDER,
        display_name: "TypeSafe AI",
        protocol: ProtocolDriver::SystemOne,
        auth_policy: AuthPolicy::RequiredApiKey,
        default_base_url: DEFAULT_TYPESAFE_BASE_URL,
        cli_flag: "--typesafe-api-key",
        env_var: ENV_TYPESAFE_API_KEY,
        cli_base_url_flag: "--typesafe-base-url",
        base_url_env_var: ENV_TYPESAFE_BASE_URL,
        config_key_path: "providers.typesafe.apiKey",
        default_discriminator: Some(DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL),
        default_generator: None,
    },
    ProviderCapability {
        slug: LOCAL_PROVIDER,
        display_name: "Local System One",
        protocol: ProtocolDriver::SystemOne,
        auth_policy: AuthPolicy::OptionalApiKey,
        default_base_url: DEFAULT_LOCAL_BASE_URL,
        cli_flag: "--local-api-key",
        env_var: ENV_LOCAL_API_KEY,
        cli_base_url_flag: "--local-base-url",
        base_url_env_var: ENV_LOCAL_BASE_URL,
        config_key_path: "providers.local.apiKey",
        default_discriminator: Some(DEFAULT_LOCAL_DISCRIMINATOR_MODEL),
        default_generator: None,
    },
];

/// Looks up the declarative built-in [`ProviderCapability`] entry for a canonical provider slug.
pub fn provider_capability(slug: &str) -> Option<&'static ProviderCapability> {
    PROVIDER_CAPABILITIES.iter().find(|cap| cap.slug == slug)
}

/// Resolved built-in or user-defined (`providers.<slug>`) provider profile (`configure.md` §4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderProfile {
    /// Canonical lowercase kebab-case provider slug.
    pub slug: String,
    /// Wire protocol driver spoken by this profile.
    pub protocol: ProtocolDriver,
    /// Credential enforcement policy (`RequiredApiKey` or `OptionalApiKey`).
    pub auth_policy: AuthPolicy,
    /// Profile-level base URL (`providers.<slug>.baseUrl` ≻ built-in default).
    pub base_url: String,
    /// Default canonical `Discriminator` `<slug>:<model>` specifier, if configured.
    pub default_discriminator: Option<String>,
    /// Default canonical `Generator` `<slug>:<model>` specifier, if configured.
    pub default_generator: Option<String>,
}

impl ProviderProfile {
    pub(super) fn from_builtin(cap: &ProviderCapability) -> Self {
        Self {
            slug: cap.slug.to_string(),
            protocol: cap.protocol,
            auth_policy: cap.auth_policy,
            base_url: cap.default_base_url.to_string(),
            default_discriminator: cap.default_discriminator.map(ToOwned::to_owned),
            default_generator: cap.default_generator.map(ToOwned::to_owned),
        }
    }

    /// Returns the default `<slug>:<model>` specifier for `node`, if defined on this profile.
    pub fn default_model_for(&self, node: HeuristicNode) -> Option<&str> {
        match node {
            HeuristicNode::Apprise
            | HeuristicNode::Docket
            | HeuristicNode::Admit
            | HeuristicNode::Vet => self.default_discriminator.as_deref(),
            HeuristicNode::Audit => self.default_generator.as_deref(),
        }
    }
}

/// Merged table-driven registry of built-in (`google`, `typesafe`) and custom (`config.json`) profiles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRegistry {
    profiles: BTreeMap<String, ProviderProfile>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

impl ProviderRegistry {
    /// Constructs a [`ProviderRegistry`] containing only built-in profiles (`google`, `typesafe`).
    pub fn builtin() -> Self {
        let mut profiles = BTreeMap::new();
        for cap in PROVIDER_CAPABILITIES {
            profiles.insert(cap.slug.to_string(), ProviderProfile::from_builtin(cap));
        }
        Self { profiles }
    }

    /// Builds and validates a [`ProviderRegistry`] merging built-in profiles with `stored.providers`.
    pub fn from_stored(stored: Option<&StoredConfig>) -> Result<Self, ConfigureError> {
        Self::from_stored_with_path(stored, None)
    }

    pub(crate) fn from_stored_with_path(
        stored: Option<&StoredConfig>,
        source_path: Option<&str>,
    ) -> Result<Self, ConfigureError> {
        let mut registry = Self::builtin();
        let Some(cfg) = stored else {
            return Ok(registry);
        };
        let path = source_path.unwrap_or("~/.config/canon-clerk/config.json");

        for (slug, entry) in &cfg.providers {
            if !is_valid_provider_slug(slug) {
                return Err(ConfigureError::MalformedConfigFile {
                    path: path.to_string(),
                    message: format!(
                        "invalid provider slug `{slug}` under `providers` (expected lowercase kebab-case `[a-z0-9]+(-[a-z0-9]+)*`, e.g. `local-dev` or `google-corp`)."
                    ),
                });
            }
            let resolved = match provider_capability(slug) {
                Some(builtin) => merge_builtin_profile(builtin, entry, path)?,
                None => build_custom_profile(slug, entry, path)?,
            };
            registry.profiles.insert(slug.clone(), resolved);
        }
        Ok(registry)
    }

    /// Looks up a resolved [`ProviderProfile`] by slug.
    pub fn get(&self, slug: &str) -> Option<&ProviderProfile> {
        self.profiles.get(slug)
    }

    /// Resolves the active default [`ProviderProfile`] along `--provider` ≻ `CANON_CLERK_PROVIDER` ≻ `defaults.provider` ≻ `existing` ≻ `"google"`.
    pub(crate) fn resolve_active_provider(
        &self,
        options: &ConfigureOptions,
        stored: Option<&StoredConfig>,
        existing: Option<&CaseloadConfig>,
    ) -> Result<&ProviderProfile, ConfigureError> {
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

        self.get(raw_slug)
            .ok_or_else(|| ConfigureError::UnsupportedProviderSelector {
                provider: raw_slug.to_string(),
            })
    }

    /// Validates and normalizes a `<provider>:<model>` specifier against registered profiles and protocol rules.
    pub fn normalize_specifier(&self, raw: &str) -> Result<String, ConfigureError> {
        let (provider, model_part) = split_and_check_specifier_syntax(raw)?;
        let Some(profile) = self.get(provider) else {
            return Err(ConfigureError::UnsupportedProvider {
                provider: provider.to_string(),
                specifier: raw.trim().to_string(),
            });
        };
        profile
            .protocol
            .validate_model_id(provider, model_part)
            .map_err(|(reason, example)| ConfigureError::InvalidModelSpecifier {
                specifier: raw.to_string(),
                reason,
                example,
            })?;
        Ok(format!("{provider}:{model_part}"))
    }

    /// Classifies a normalized `<provider>:<model>` specifier via its profile's [`ProtocolDriver`].
    pub fn classify_specifier(&self, specifier: &str) -> ModelCapability {
        let trimmed = specifier.trim();
        if let Some((prov, model_id)) = trimmed.split_once(':')
            && let Some(profile) = self.get(prov)
        {
            return profile.protocol.classify_model_id(model_id);
        }
        crate::configure::model_capability(trimmed)
    }

    /// Resolves a live [`ProviderConnection`] (`protocol`, `base_url`, `auth`) for `slug`.
    pub fn resolve_connection(
        &self,
        slug: &str,
        options: &ConfigureOptions,
    ) -> Result<ProviderConnection, ConfigureError> {
        let profile =
            self.get(slug)
                .ok_or_else(|| ConfigureError::UnsupportedProviderSelector {
                    provider: slug.to_string(),
                })?;

        let base_url = resolve_profile_base_url(profile, options)?;
        let secret = resolve_profile_api_key(slug, options);
        let auth = match (secret, profile.auth_policy, profile.protocol) {
            (Some(k), _, ProtocolDriver::Gemini) => ProviderAuth::GoogApiKey(k),
            (Some(k), _, ProtocolDriver::SystemOne) => ProviderAuth::Bearer(k),
            (None, AuthPolicy::OptionalApiKey, _) => ProviderAuth::None,
            (None, AuthPolicy::RequiredApiKey, _) => {
                return Err(ConfigureError::missing_api_key_for_slug(slug));
            }
        };

        Ok(ProviderConnection {
            slug: profile.slug.clone(),
            protocol: profile.protocol,
            base_url,
            auth,
        })
    }

    /// Resolves deduplicated [`ProviderConnection`]s for all unique providers referenced by `models`.
    pub fn resolve_closure_connections<'a, I>(
        &self,
        models: I,
        options: &ConfigureOptions,
    ) -> Result<BTreeMap<String, ProviderConnection>, ConfigureError>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut connections = BTreeMap::new();
        for model in models {
            let slug = model_provider(model);
            if !connections.contains_key(slug) {
                let conn = self.resolve_connection(slug, options)?;
                connections.insert(slug.to_string(), conn);
            }
        }
        Ok(connections)
    }
}

fn resolve_profile_base_url(
    profile: &ProviderProfile,
    options: &ConfigureOptions,
) -> Result<String, ConfigureError> {
    if let Some((flag_name, raw_cli)) = options.cli_base_url_for(&profile.slug) {
        return validate_http_base_url(raw_cli).map_err(|reason| ConfigureError::InvalidBaseUrl {
            slug: profile.slug.clone(),
            source: format!("CLI flag `{flag_name}`"),
            raw: raw_cli.to_string(),
            reason,
        });
    }

    for env_key in base_url_env_candidates(&profile.slug) {
        if let Some(raw_env) = options
            .env
            .get(&env_key)
            .map(String::as_str)
            .filter(|s| !s.trim().is_empty())
        {
            return validate_http_base_url(raw_env).map_err(|reason| {
                ConfigureError::InvalidBaseUrl {
                    slug: profile.slug.clone(),
                    source: format!("environment variable `{env_key}`"),
                    raw: raw_env.to_string(),
                    reason,
                }
            });
        }
    }

    Ok(profile.base_url.clone())
}

fn base_url_env_candidates(slug: &str) -> Vec<String> {
    let stem = slug_to_upper_env_stem(slug);
    let mut keys = vec![format!("CANON_CLERK_{stem}_BASE_URL")];
    if slug == DEFAULT_PROVIDER {
        keys.push("CANON_CLERK_GEMINI_BASE_URL".to_string());
        keys.push(ENV_GEMINI_BASE_URL.to_string());
    }
    keys.push(format!("{stem}_BASE_URL"));
    keys
}

fn api_key_env_candidates(slug: &str) -> Vec<String> {
    let stem = slug_to_upper_env_stem(slug);
    let mut keys = vec![format!("CANON_CLERK_{stem}_API_KEY")];
    if slug == DEFAULT_PROVIDER {
        keys.push("CANON_CLERK_GEMINI_API_KEY".to_string());
        keys.push(ENV_GEMINI_API_KEY.to_string());
    }
    keys.push(format!("{stem}_API_KEY"));
    keys
}

pub(crate) fn resolve_profile_api_key(slug: &str, options: &ConfigureOptions) -> Option<String> {
    if let Some(cli_val) = options
        .cli_api_key_for(slug)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(cli_val.to_string());
    }
    for env_key in api_key_env_candidates(slug) {
        if let Some(env_val) = options
            .env
            .get(&env_key)
            .map(String::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(env_val.to_string());
        }
    }
    options
        .stored_config
        .as_ref()
        .and_then(|s| s.provider_api_key(slug))
        .map(ToOwned::to_owned)
}

/// Resolves and validates a required API key for `slug`.
pub(crate) fn resolve_provider_api_key(
    slug: &str,
    options: &ConfigureOptions,
) -> Result<String, ConfigureError> {
    let registry = ProviderRegistry::from_stored(options.stored_config.as_ref())?;
    if registry.get(slug).is_none() {
        return Err(ConfigureError::UnsupportedProviderSelector {
            provider: slug.to_string(),
        });
    }
    resolve_profile_api_key(slug, options)
        .ok_or_else(|| ConfigureError::missing_api_key_for_slug(slug))
}

/// Returns deduplicated, lexicographically sorted credential environment variable labels verified
/// for the provisioned nodes on `config` (`empty` when all active nodes run keyless).
pub(crate) fn verified_credential_labels(
    config: &CaseloadConfig,
    options: &ConfigureOptions,
) -> Vec<String> {
    let registry =
        ProviderRegistry::from_stored(options.stored_config.as_ref()).unwrap_or_default();
    let mut vars = BTreeSet::new();
    for node_cfg in config.nodes.values() {
        let slug = model_provider(&node_cfg.model);
        let has_key = resolve_profile_api_key(slug, options).is_some();
        let required_policy = registry
            .get(slug)
            .is_some_and(|p| p.auth_policy == AuthPolicy::RequiredApiKey);
        if has_key || required_policy {
            vars.insert(MissingCredentialSpec::for_slug(slug).primary_env);
        }
    }
    vars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure::{StoredDefaultsConfig, StoredProviderConfig};

    mod provider_selector_and_custom_profiles {
        use super::*;

        #[test]
        fn test_provider_resolves_deterministically_without_ambient_api_key_sniffing() {
            let mut ts_only_env = BTreeMap::new();
            ts_only_env.insert(ENV_TYPESAFE_API_KEY.to_string(), "ts-key".to_string());
            let reg = ProviderRegistry::builtin();
            let cap = reg
                .resolve_active_provider(
                    &ConfigureOptions {
                        env: ts_only_env,
                        ..ConfigureOptions::default()
                    },
                    None,
                    None,
                )
                .expect("resolve default provider");
            assert_eq!(cap.slug, DEFAULT_PROVIDER);

            let stored = StoredConfig {
                defaults: StoredDefaultsConfig {
                    provider: Some("typesafe".to_string()),
                    ..StoredDefaultsConfig::default()
                },
                ..StoredConfig::default()
            };
            assert_eq!(
                reg.resolve_active_provider(&ConfigureOptions::default(), Some(&stored), None)
                    .expect("stored provider")
                    .slug,
                TYPESAFE_PROVIDER
            );

            let mut env_override = BTreeMap::new();
            env_override.insert(ENV_PROVIDER.to_string(), "google".to_string());
            assert_eq!(
                reg.resolve_active_provider(
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

            assert_eq!(
                reg.resolve_active_provider(
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
        fn test_custom_systemone_keyless_and_authenticated_profiles_and_baseurl_cascade() {
            let mut providers = BTreeMap::new();
            providers.insert(
                "local-dev".to_string(),
                StoredProviderConfig {
                    protocol: Some("systemone".to_string()),
                    base_url: Some("http://127.0.0.1:8080/v1/".to_string()),
                    default_discriminator: Some("local-dev:kev-0.8b".to_string()),
                    ..StoredProviderConfig::default()
                },
            );
            providers.insert(
                "google-corp".to_string(),
                StoredProviderConfig {
                    protocol: Some("gemini".to_string()),
                    base_url: Some("https://gemini-gateway.corp.example.com/v1beta".to_string()),
                    api_key: Some("corp-stored-key".to_string()),
                    default_discriminator: Some("google-corp:gemini-decision-1".to_string()),
                    default_generator: Some("google-corp:gemini-4-carbon".to_string()),
                },
            );
            let stored = StoredConfig {
                providers,
                ..StoredConfig::default()
            };
            let reg = ProviderRegistry::from_stored(Some(&stored)).expect("valid custom registry");

            let opts = ConfigureOptions {
                stored_config: Some(stored.clone()),
                ..ConfigureOptions::default()
            };
            let local_conn = reg
                .resolve_connection("local-dev", &opts)
                .expect("keyless local-dev");
            assert_eq!(local_conn.protocol, ProtocolDriver::SystemOne);
            assert_eq!(local_conn.base_url, "http://127.0.0.1:8080/v1");
            assert_eq!(local_conn.auth, ProviderAuth::None);

            let mut env = BTreeMap::new();
            env.insert(
                "CANON_CLERK_LOCAL_DEV_BASE_URL".to_string(),
                "http://127.0.0.1:9090".to_string(),
            );
            env.insert("LOCAL_DEV_API_KEY".to_string(), "lan-token".to_string());
            let env_opts = ConfigureOptions {
                env,
                stored_config: Some(stored),
                ..ConfigureOptions::default()
            };
            let overridden = reg
                .resolve_connection("local-dev", &env_opts)
                .expect("overridden local-dev");
            assert_eq!(overridden.base_url, "http://127.0.0.1:9090");
            assert_eq!(
                overridden.auth,
                ProviderAuth::Bearer("lan-token".to_string())
            );
        }

        #[test]
        fn test_builtin_local_profile_is_keyless_and_honors_base_url_and_api_key_overrides() {
            let reg = ProviderRegistry::builtin();
            let local_profile = reg.get(LOCAL_PROVIDER).expect("built-in local profile");
            assert_eq!(local_profile.protocol, ProtocolDriver::SystemOne);
            assert_eq!(local_profile.auth_policy, AuthPolicy::OptionalApiKey);
            assert_eq!(local_profile.base_url, DEFAULT_LOCAL_BASE_URL);
            assert_eq!(
                local_profile.default_discriminator.as_deref(),
                Some(DEFAULT_LOCAL_DISCRIMINATOR_MODEL)
            );
            assert_eq!(local_profile.default_generator, None);

            let keyless = reg
                .resolve_connection(LOCAL_PROVIDER, &ConfigureOptions::default())
                .expect("built-in local connects keyless by default");
            assert_eq!(keyless.base_url, "http://127.0.0.1:8080/v1");
            assert_eq!(keyless.auth, ProviderAuth::None);

            let cli_overridden = reg
                .resolve_connection(
                    LOCAL_PROVIDER,
                    &ConfigureOptions {
                        local_base_url: Some("http://127.0.0.1:18080/v1/".to_string()),
                        local_api_key: Some("local-bearer".to_string()),
                        ..ConfigureOptions::default()
                    },
                )
                .expect("built-in local honors CLI overrides");
            assert_eq!(cli_overridden.base_url, "http://127.0.0.1:18080/v1");
            assert_eq!(
                cli_overridden.auth,
                ProviderAuth::Bearer("local-bearer".to_string())
            );
        }
    }
}
