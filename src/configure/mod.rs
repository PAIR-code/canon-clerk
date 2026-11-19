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

mod error;
mod profile_schema;
mod protocol;
mod providers;
pub mod store;
mod thinking;

pub use error::ConfigureError;
pub(crate) use error::check_retired_env_vars;
pub use protocol::{
    AuthPolicy, DEFAULT_GEMINI_BASE_URL, DEFAULT_TYPESAFE_BASE_URL, MissingCredentialSpec,
    ProtocolDriver, ProviderAuth, ProviderConnection, is_valid_provider_slug,
    slug_to_upper_env_stem, validate_http_base_url,
};
pub(crate) use protocol::{
    credential_verification_hint_for_slug, is_gemini_decision_model_name,
    is_valid_gemini_model_name,
};
pub use providers::{
    ModelCapability, PROVIDER_CAPABILITIES, ProviderCapability, ProviderProfile, ProviderRegistry,
    provider_capability,
};
pub(crate) use providers::{resolve_provider_api_key, verified_credential_labels};
pub use store::{
    StoredConfig, StoredDefaultsConfig, StoredNodeConfig, StoredProviderConfig,
    load_host_config_file, resolve_default_config_path,
};
pub use thinking::parse_thinking_level;

use crate::caseload::{
    Caseload, CaseloadConfig, DEFAULT_PROVIDER, HeuristicNode, ResolvedNodeConfig,
    TYPESAFE_PROVIDER, ThinkingLevel,
};
use std::collections::{BTreeMap, BTreeSet};
use thinking::{resolve_global_thinking_layers, resolve_node_thinking_options};

/// Environment variable name for overriding the active default provider (`google`, `typesafe`, or custom slug).
pub const ENV_PROVIDER: &str = "CANON_CLERK_PROVIDER";

/// Environment variable name for overriding `apprise`'s `<provider>:<model>` specifier.
pub const ENV_APPRISE_MODEL: &str = "CANON_CLERK_APPRISE_MODEL";

/// Environment variable name for overriding `docket`'s `<provider>:<model>` specifier.
pub const ENV_DOCKET_MODEL: &str = "CANON_CLERK_DOCKET_MODEL";

/// Environment variable name for overriding `admit`'s `<provider>:<model>` specifier.
pub const ENV_ADMIT_MODEL: &str = "CANON_CLERK_ADMIT_MODEL";

/// Environment variable name for overriding `vet`'s `<provider>:<model>` specifier.
pub const ENV_VET_MODEL: &str = "CANON_CLERK_VET_MODEL";

/// Environment variable name for overriding `audit`'s `<provider>:<model>` specifier.
pub const ENV_AUDIT_MODEL: &str = "CANON_CLERK_AUDIT_MODEL";

/// Environment variable name for the Google Gemini API key.
pub const ENV_GEMINI_API_KEY: &str = "GEMINI_API_KEY";

/// Environment variable name for the TypeSafe AI (Jev) API key.
pub const ENV_TYPESAFE_API_KEY: &str = "TYPESAFE_API_KEY";

/// Environment variable name for overriding the Google Generative Language API base URL.
pub const ENV_GEMINI_BASE_URL: &str = "GEMINI_BASE_URL";

/// Environment variable name for overriding the TypeSafe AI System One API base URL.
pub const ENV_TYPESAFE_BASE_URL: &str = "TYPESAFE_BASE_URL";

/// Environment variable name for overriding the qualitative thinking level (`minimal|low|medium|high`).
pub const ENV_THINKING_LEVEL: &str = "CANON_CLERK_THINKING_LEVEL";

/// Environment variable name for overriding the maximum reasoning budget in tokens.
pub const ENV_REASONING_BUDGET: &str = "CANON_CLERK_REASONING_BUDGET";

/// Target node resolution and credential verification scope for [`execute_configure`] (`configure.md` §4).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConfigureScope {
    /// Standalone `configure` and `probe`: resolves all provisioned heuristic nodes for the active configuration.
    #[default]
    AllProvisioned,
    /// Converged evaluation closure: resolves and verifies credentials strictly for executed heuristic nodes.
    Closure(BTreeSet<HeuristicNode>),
}

impl ConfigureScope {
    /// Constructs a [`ConfigureScope::Closure`] from an iterator of [`HeuristicNode`]s.
    pub fn closure<I: IntoIterator<Item = HeuristicNode>>(nodes: I) -> Self {
        Self::Closure(nodes.into_iter().collect())
    }
}

/// Domain input parameters, environment snapshot, and optional user config store for `configure`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigureOptions {
    /// Explicit active default provider override (`--provider <slug>`).
    pub provider: Option<String>,
    /// Explicit per-node CLI model overrides (`--<node>-model <provider:model>`).
    pub node_models: BTreeMap<HeuristicNode, String>,
    /// Explicit qualitative thinking level override (`--thinking-level`).
    pub thinking_level: Option<ThinkingLevel>,
    /// Explicit reasoning budget override in tokens (`--reasoning-budget`).
    pub reasoning_budget: Option<u32>,
    /// Explicit Google Gemini API key override (`--gemini-api-key`).
    pub gemini_api_key: Option<String>,
    /// Explicit TypeSafe AI API key override (`--typesafe-api-key`).
    pub typesafe_api_key: Option<String>,
    /// Explicit Google Generative Language API base URL override (`--gemini-base-url`).
    pub gemini_base_url: Option<String>,
    /// Explicit TypeSafe AI System One API base URL override (`--typesafe-base-url`).
    pub typesafe_base_url: Option<String>,
    /// Target heuristic node resolution and credential verification scope.
    pub scope: ConfigureScope,
    /// Snapshot of runtime environment variables injected by the driving adapter.
    pub env: BTreeMap<String, String>,
    /// Optional parsed user-level configuration (`~/.config/canon-clerk/config.json`).
    pub stored_config: Option<StoredConfig>,
}

impl ConfigureOptions {
    /// Returns `true` when any explicit CLI or programmatic override flag was supplied.
    pub fn has_explicit_overrides(&self) -> bool {
        self.provider.is_some()
            || !self.node_models.is_empty()
            || self.thinking_level.is_some()
            || self.reasoning_budget.is_some()
            || self.gemini_api_key.is_some()
            || self.typesafe_api_key.is_some()
            || self.gemini_base_url.is_some()
            || self.typesafe_base_url.is_some()
    }

    /// Inserts an explicit CLI model override for `node`.
    pub fn with_node_model(mut self, node: HeuristicNode, model: impl Into<String>) -> Self {
        self.node_models.insert(node, model.into());
        self
    }

    pub(crate) fn cli_api_key_for(&self, provider_slug: &str) -> Option<&str> {
        match provider_slug {
            DEFAULT_PROVIDER => self.gemini_api_key.as_deref(),
            TYPESAFE_PROVIDER => self.typesafe_api_key.as_deref(),
            _ => None,
        }
    }

    pub(crate) fn cli_base_url_for(&self, provider_slug: &str) -> Option<(&'static str, &str)> {
        match provider_slug {
            DEFAULT_PROVIDER => self
                .gemini_base_url
                .as_deref()
                .map(|v| ("--gemini-base-url", v)),
            TYPESAFE_PROVIDER => self
                .typesafe_base_url
                .as_deref()
                .map(|v| ("--typesafe-base-url", v)),
            _ => None,
        }
    }

    fn validate_explicit_cli_base_urls(&self) -> Result<(), ConfigureError> {
        for (slug, flag_name, raw_opt) in [
            (DEFAULT_PROVIDER, "--gemini-base-url", &self.gemini_base_url),
            (
                TYPESAFE_PROVIDER,
                "--typesafe-base-url",
                &self.typesafe_base_url,
            ),
        ] {
            if let Some(raw) = raw_opt {
                validate_http_base_url(raw).map_err(|reason| ConfigureError::InvalidBaseUrl {
                    slug: slug.to_string(),
                    source: format!("CLI flag `{flag_name}`"),
                    raw: raw.clone(),
                    reason,
                })?;
            }
        }
        Ok(())
    }
}

/// Extracts the canonical provider slug (`"google"`, `"typesafe"`, `"local-dev"`, etc.) from `<provider>:<model>`.
pub(crate) fn model_provider(specifier: &str) -> &str {
    match specifier.trim().split_once(':') {
        Some((prov, _)) if !prov.is_empty() => prov,
        _ => DEFAULT_PROVIDER,
    }
}

/// Validates and normalizes a canonical `<provider>:<model>` specifier against built-in providers (`configure.md` §3.4).
pub fn normalize_model_specifier(raw: &str) -> Result<String, ConfigureError> {
    ProviderRegistry::builtin().normalize_specifier(raw)
}

/// Classifies a canonical `<provider>:<model>` specifier into its [`ModelCapability`].
///
/// - `<slug>:gemini-decision-*` $\to$ [`ModelCapability::Discriminator`]
/// - Other `<slug>:gemini-*` $\to$ [`ModelCapability::Generator`]
/// - Non-`gemini-*` `<slug>:<model>` (`typesafe:jev-*`, `local-dev:kev-0.8b`, etc.) $\to$ [`ModelCapability::Discriminator`]
/// - Bare or empty fallback $\to$ [`ModelCapability::Generator`]
pub fn model_capability(specifier: &str) -> ModelCapability {
    let trimmed = specifier.trim();
    match trimmed.split_once(':') {
        Some((prov, model_part)) if !prov.is_empty() && !model_part.is_empty() => {
            if is_gemini_decision_model_name(model_part) {
                ModelCapability::Discriminator
            } else if is_valid_gemini_model_name(model_part) {
                ModelCapability::Generator
            } else {
                ModelCapability::Discriminator
            }
        }
        _ => ModelCapability::Generator,
    }
}

/// Returns `true` when `specifier` names a `Discriminator` `Noul` decision model.
pub fn is_discriminative_model(specifier: &str) -> bool {
    model_capability(specifier) == ModelCapability::Discriminator
}

/// Resolves the Google Gemini API key from explicit overrides, environment variables, or `config.json`.
pub fn resolve_gemini_api_key(options: &ConfigureOptions) -> Result<String, ConfigureError> {
    resolve_provider_api_key(DEFAULT_PROVIDER, options)
}

/// Resolves the active [`ProviderConnection`] map for all closure-bound nodes on `config`.
pub fn resolve_provider_connections(
    options: &ConfigureOptions,
    config: &CaseloadConfig,
) -> Result<BTreeMap<String, ProviderConnection>, ConfigureError> {
    options.validate_explicit_cli_base_urls()?;
    let registry = ProviderRegistry::from_stored(options.stored_config.as_ref())?;
    let active_models: Vec<&str> = match &options.scope {
        ConfigureScope::AllProvisioned => config.nodes.values().map(|n| n.model.as_str()).collect(),
        ConfigureScope::Closure(required) => config
            .nodes
            .iter()
            .filter(|(node, _)| required.contains(node) || options.node_models.contains_key(node))
            .map(|(_, n)| n.model.as_str())
            .collect(),
    };
    registry.resolve_closure_connections(active_models, options)
}

/// Returns `true` when `existing` already satisfies `options.scope` with zero explicit CLI/programmatic overrides.
pub(crate) fn can_fast_forward_existing_config(
    options: &ConfigureOptions,
    existing: Option<&CaseloadConfig>,
) -> bool {
    if options.has_explicit_overrides() {
        return false;
    }
    let Some(cfg) = existing else {
        return false;
    };
    match &options.scope {
        ConfigureScope::AllProvisioned => !cfg.nodes.is_empty(),
        ConfigureScope::Closure(required) => {
            required.iter().all(|node| cfg.nodes.contains_key(node))
        }
    }
}

fn resolve_pinned_node_model(
    node: HeuristicNode,
    registry: &ProviderRegistry,
    options: &ConfigureOptions,
    stored: Option<&StoredConfig>,
    existing: Option<&CaseloadConfig>,
    preserve_existing_model: bool,
) -> Result<Option<String>, ConfigureError> {
    if let Some(raw_cli) = options.node_models.get(&node) {
        return registry.normalize_specifier(raw_cli).map(Some);
    }
    if let Some(raw_env) = options
        .env
        .get(node.env_model_var())
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return registry.normalize_specifier(raw_env).map(Some);
    }
    if let Some(raw_stored) = stored.and_then(|s| s.node_model(node)) {
        return registry.normalize_specifier(raw_stored).map(Some);
    }
    if preserve_existing_model
        && let Some(raw_existing) = existing
            .and_then(|c| c.model_for(node))
            .map(str::trim)
            .filter(|s| !s.is_empty())
    {
        return registry.normalize_specifier(raw_existing).map(Some);
    }
    Ok(None)
}

fn target_nodes_for_scope(options: &ConfigureOptions) -> Vec<HeuristicNode> {
    match &options.scope {
        ConfigureScope::AllProvisioned => HeuristicNode::ALL.to_vec(),
        ConfigureScope::Closure(required) => {
            let mut combined: BTreeSet<HeuristicNode> = required.clone();
            combined.extend(options.node_models.keys().copied());
            combined.into_iter().collect()
        }
    }
}

fn handle_unprovisioned_node(
    node: HeuristicNode,
    scope: &ConfigureScope,
    provider_slug: &str,
) -> Result<(), ConfigureError> {
    match scope {
        ConfigureScope::AllProvisioned => Ok(()),
        ConfigureScope::Closure(required) if required.contains(&node) => {
            if node == HeuristicNode::Audit {
                Err(ConfigureError::UnprovisionedAuditModel {
                    provider: provider_slug.to_string(),
                })
            } else {
                Err(ConfigureError::UnprovisionedNodeModel {
                    node,
                    provider: provider_slug.to_string(),
                })
            }
        }
        ConfigureScope::Closure(_) => Ok(()),
    }
}

/// Executes the pure, deterministic `configure` stage and populates `.config` on the [`Caseload`].
pub fn execute_configure(
    options: &ConfigureOptions,
    caseload: Option<Caseload>,
) -> Result<Caseload, ConfigureError> {
    let caseload = caseload.unwrap_or_default();
    check_retired_env_vars(&options.env)?;
    options.validate_explicit_cli_base_urls()?;
    let stored = options.stored_config.as_ref();
    if let Some(s) = stored {
        s.validate_schema(None)?;
    }

    if can_fast_forward_existing_config(options, caseload.config.as_ref()) {
        return Ok(caseload);
    }

    let registry = ProviderRegistry::from_stored(stored)?;
    let existing = caseload.config.as_ref();
    let active_profile = registry.resolve_active_provider(options, stored, existing)?;
    let global_thinking = resolve_global_thinking_layers(options, stored)?;
    let preserve_existing_model = !existing.is_some_and(|c| c.provider != active_profile.slug);

    let mut nodes: BTreeMap<HeuristicNode, ResolvedNodeConfig> = match &options.scope {
        ConfigureScope::AllProvisioned => BTreeMap::new(),
        ConfigureScope::Closure(_) => existing.map(|c| c.nodes.clone()).unwrap_or_default(),
    };

    let mut models_requiring_connections = Vec::new();
    for node in target_nodes_for_scope(options) {
        let pinned = resolve_pinned_node_model(
            node,
            &registry,
            options,
            stored,
            existing,
            preserve_existing_model,
        )?;
        let model = match pinned.or_else(|| {
            active_profile
                .default_model_for(node)
                .map(ToOwned::to_owned)
        }) {
            Some(m) => m,
            None => {
                handle_unprovisioned_node(node, &options.scope, &active_profile.slug)?;
                continue;
            }
        };

        let capability = registry.classify_specifier(&model);
        if node == HeuristicNode::Audit && capability == ModelCapability::Discriminator {
            return Err(ConfigureError::DiscriminativeAuditUnsupported { specifier: model });
        }

        let existing_node = existing.and_then(|c| c.node(node));
        let (thinking_level, reasoning_budget) = resolve_node_thinking_options(
            node,
            &model,
            capability,
            global_thinking,
            stored,
            existing_node,
        )?;

        let verify_conn = match &options.scope {
            ConfigureScope::AllProvisioned => true,
            ConfigureScope::Closure(required) => {
                required.contains(&node) || options.node_models.contains_key(&node)
            }
        };
        if verify_conn {
            models_requiring_connections.push(model.clone());
        }

        nodes.insert(
            node,
            ResolvedNodeConfig::with_thinking(model, thinking_level, reasoning_budget),
        );
    }

    registry.resolve_closure_connections(
        models_requiring_connections.iter().map(String::as_str),
        options,
    )?;

    let config = CaseloadConfig {
        provider: active_profile.slug.clone(),
        nodes,
    };

    Ok(caseload.with_config(config))
}

pub(crate) fn resolve_non_empty<'a>(
    override_val: Option<&'a str>,
    env_val: Option<&'a str>,
) -> Option<&'a str> {
    override_val
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| env_val.map(str::trim).filter(|s| !s.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{
        DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, DEFAULT_GOOGLE_GENERATOR_MODEL,
        DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL,
    };

    fn env_with_google_key() -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        env.insert(
            ENV_GEMINI_API_KEY.to_string(),
            "test-synthetic-gemini-key".to_string(),
        );
        env
    }

    fn env_with_both_keys() -> BTreeMap<String, String> {
        let mut env = env_with_google_key();
        env.insert(
            ENV_TYPESAFE_API_KEY.to_string(),
            "test-synthetic-typesafe-key".to_string(),
        );
        env
    }

    mod strict_specifier_grammar {
        use super::*;

        #[test]
        fn test_canonical_provider_colon_model_specifiers_normalize() {
            assert_eq!(
                normalize_model_specifier("google:gemini-4-carbon").expect("carbon"),
                DEFAULT_GOOGLE_GENERATOR_MODEL
            );
            assert_eq!(
                normalize_model_specifier("google:gemini-decision-1").expect("decision-1"),
                DEFAULT_GOOGLE_DISCRIMINATOR_MODEL
            );
            assert_eq!(
                normalize_model_specifier("  google:gemini-2.5-flash-lite  ").expect("trimmed"),
                "google:gemini-2.5-flash-lite"
            );
            assert_eq!(
                normalize_model_specifier("typesafe:jev-latest").expect("jev-latest"),
                DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL
            );
            assert_eq!(
                normalize_model_specifier("typesafe:jev-1.14.0").expect("versioned jev"),
                "typesafe:jev-1.14.0"
            );
            assert!(is_discriminative_model("google:gemini-decision-1"));
            assert!(is_discriminative_model("google-corp:gemini-decision-1"));
            assert!(is_discriminative_model("typesafe:jev-latest"));
            assert!(is_discriminative_model("local-dev:kev-0.8b"));
            assert!(is_discriminative_model("lan-gpu:clef-flash"));
            assert!(!is_discriminative_model("google:gemini-4-carbon"));
            assert!(!is_discriminative_model("google-corp:gemini-4-carbon"));
            assert!(!is_discriminative_model(""));
        }

        #[test]
        fn test_bare_short_names_and_retired_aliases_fail_fast_with_example_hint() {
            for bare in [
                "gemini-decision-1",
                "gemini-4-carbon",
                "gemini-4-hydrogen",
                "jev-latest",
                "carbon",
                "hydrogen",
                "jev",
                "google:carbon",
                "typesafe:jev",
            ] {
                let err = normalize_model_specifier(bare)
                    .expect_err(&format!("`{bare}` must be rejected"));
                assert!(matches!(err, ConfigureError::InvalidModelSpecifier { .. }));
                let rendered = err.to_string();
                assert!(rendered.contains("Hint:"));
                assert!(rendered.contains(':'));
            }
            let decision_err =
                normalize_model_specifier("gemini-decision-1").expect_err("bare decision-1");
            assert!(
                decision_err
                    .to_string()
                    .contains("for example, `google:gemini-decision-1`")
            );
        }

        #[test]
        fn test_unsupported_providers_and_malformed_specifiers_rejected() {
            let err = normalize_model_specifier("openai:gpt-4o").expect_err("openai must fail");
            assert_eq!(
                err,
                ConfigureError::UnsupportedProvider {
                    provider: "openai".to_string(),
                    specifier: "openai:gpt-4o".to_string(),
                }
            );
            assert!(err.to_string().contains("Hint:"));

            for invalid in [
                "",
                "   ",
                ":gemini-4-carbon",
                "google:",
                "typesafe:",
                "google:gemini:pro",
                "google:gemini pro",
                "google:jev-latest",
                "typesafe:gemini-4-carbon",
                "google:gemini-decision",
                "google:gemini-decision-",
                "typesafe:jev-",
            ] {
                let err = normalize_model_specifier(invalid)
                    .expect_err(&format!("`{invalid}` must fail"));
                assert!(matches!(err, ConfigureError::InvalidModelSpecifier { .. }));
                assert!(err.to_string().contains("Hint:"));
            }
        }
    }

    mod provider_and_node_cascades {
        use super::*;

        #[test]
        fn test_zero_config_google_resolves_all_five_nodes() {
            let opts = ConfigureOptions {
                env: env_with_google_key(),
                ..ConfigureOptions::default()
            };
            let caseload = execute_configure(&opts, None).expect("configure google default");
            assert_eq!(caseload.config, Some(CaseloadConfig::default()));
        }

        #[test]
        fn test_typesafe_sole_provider_provisions_four_discriminative_nodes_and_guards_audit() {
            let mut ts_env = BTreeMap::new();
            ts_env.insert(ENV_TYPESAFE_API_KEY.to_string(), "ts-secret".to_string());

            let standalone = execute_configure(
                &ConfigureOptions {
                    provider: Some("typesafe".to_string()),
                    env: ts_env.clone(),
                    ..ConfigureOptions::default()
                },
                None,
            )
            .expect("standalone configure --provider typesafe succeeds");
            let cfg = standalone.config.expect("config");
            assert_eq!(cfg.provider, "typesafe");
            assert_eq!(cfg.nodes.len(), 4);
            assert!(!cfg.nodes.contains_key(&HeuristicNode::Audit));
            assert_eq!(
                cfg.model_for(HeuristicNode::Vet),
                Some(DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL)
            );

            let audit_err = execute_configure(
                &ConfigureOptions {
                    provider: Some("typesafe".to_string()),
                    scope: ConfigureScope::closure([
                        HeuristicNode::Docket,
                        HeuristicNode::Admit,
                        HeuristicNode::Vet,
                        HeuristicNode::Audit,
                    ]),
                    env: ts_env,
                    ..ConfigureOptions::default()
                },
                None,
            )
            .expect_err("typesafe without audit model must fail on audit closure");
            assert!(matches!(
                audit_err,
                ConfigureError::UnprovisionedAuditModel { .. }
            ));
            assert!(audit_err.to_string().contains("--audit-model"));
            assert!(audit_err.to_string().contains("Hint:"));
        }

        #[test]
        fn test_zero_code_custom_keyless_systemone_and_google_hybrid_cascade() {
            let mut providers = BTreeMap::new();
            providers.insert(
                "local-dev".to_string(),
                StoredProviderConfig {
                    protocol: Some("systemone".to_string()),
                    base_url: Some("http://127.0.0.1:8080/v1".to_string()),
                    default_discriminator: Some("local-dev:kev-0.8b".to_string()),
                    ..StoredProviderConfig::default()
                },
            );
            providers.insert(
                "google".to_string(),
                StoredProviderConfig {
                    api_key: Some("stored-google-key".to_string()),
                    ..StoredProviderConfig::default()
                },
            );
            let mut stored_nodes = BTreeMap::new();
            stored_nodes.insert(
                HeuristicNode::Audit,
                StoredNodeConfig {
                    model: Some(DEFAULT_GOOGLE_GENERATOR_MODEL.to_string()),
                    ..StoredNodeConfig::default()
                },
            );
            let stored = StoredConfig {
                providers,
                defaults: StoredDefaultsConfig {
                    provider: Some("local-dev".to_string()),
                    ..StoredDefaultsConfig::default()
                },
                nodes: stored_nodes,
            };

            // 1.Vet closure uses ONLY `local-dev` -> succeeds with ZERO API keys!
            let vet_only_providers = BTreeMap::from([(
                "local-dev".to_string(),
                StoredProviderConfig {
                    protocol: Some("systemone".to_string()),
                    base_url: Some("http://127.0.0.1:8080/v1".to_string()),
                    default_discriminator: Some("local-dev:kev-0.8b".to_string()),
                    ..StoredProviderConfig::default()
                },
            )]);
            let vet_opts = ConfigureOptions {
                scope: ConfigureScope::closure([
                    HeuristicNode::Docket,
                    HeuristicNode::Admit,
                    HeuristicNode::Vet,
                ]),
                stored_config: Some(StoredConfig {
                    providers: vet_only_providers,
                    defaults: StoredDefaultsConfig {
                        provider: Some("local-dev".to_string()),
                        ..StoredDefaultsConfig::default()
                    },
                    nodes: BTreeMap::new(),
                }),
                ..ConfigureOptions::default()
            };
            let vet_caseload = execute_configure(&vet_opts, None).expect("keyless local-dev vet");
            let vet_cfg = vet_caseload.config.as_ref().expect("config");
            assert_eq!(vet_cfg.provider, "local-dev");
            assert_eq!(
                vet_cfg.model_for(HeuristicNode::Docket),
                Some("local-dev:kev-0.8b")
            );
            assert!(verified_credential_labels(vet_cfg, &vet_opts).is_empty());

            // 2. Hybrid `local-dev` screening + `google:gemini-4-carbon` audit resolves both connections!
            let hybrid_opts = ConfigureOptions {
                stored_config: Some(stored),
                ..ConfigureOptions::default()
            };
            let hybrid_caseload =
                execute_configure(&hybrid_opts, None).expect("hybrid local-dev + google");
            let hybrid_cfg = hybrid_caseload.config.expect("config");
            let conns =
                resolve_provider_connections(&hybrid_opts, &hybrid_cfg).expect("connections");
            assert_eq!(conns.len(), 2);
            assert_eq!(conns["local-dev"].auth, ProviderAuth::None);
            assert_eq!(
                conns["google"].auth,
                ProviderAuth::GoogApiKey("stored-google-key".to_string())
            );
            // Caseload JSON never persists baseUrl or secrets
            let serialized = serde_json::to_string(&hybrid_cfg).expect("serialize");
            assert!(!serialized.contains("127.0.0.1"));
            assert!(!serialized.contains("stored-google-key"));
        }

        #[test]
        fn test_four_level_node_model_cascade_and_thinking_scoping() {
            let mut providers = BTreeMap::new();
            providers.insert(
                "google".to_string(),
                StoredProviderConfig {
                    api_key: Some("stored-google-key".to_string()),
                    ..StoredProviderConfig::default()
                },
            );
            providers.insert(
                "typesafe".to_string(),
                StoredProviderConfig {
                    api_key: Some("stored-typesafe-key".to_string()),
                    ..StoredProviderConfig::default()
                },
            );
            let mut stored_nodes = BTreeMap::new();
            stored_nodes.insert(
                HeuristicNode::Docket,
                StoredNodeConfig {
                    model: Some("google:gemini-2.5-flash-lite".to_string()),
                    ..StoredNodeConfig::default()
                },
            );
            stored_nodes.insert(
                HeuristicNode::Audit,
                StoredNodeConfig {
                    model: Some("google:gemini-2.5-pro".to_string()),
                    reasoning_budget: Some(2048),
                    ..StoredNodeConfig::default()
                },
            );
            let stored = StoredConfig {
                providers,
                defaults: StoredDefaultsConfig {
                    provider: Some("typesafe".to_string()),
                    thinking_level: Some("medium".to_string()),
                    ..StoredDefaultsConfig::default()
                },
                nodes: stored_nodes,
            };

            let mut env = BTreeMap::new();
            env.insert(
                ENV_ADMIT_MODEL.to_string(),
                "google:gemini-4-hydrogen".to_string(),
            );

            let opts = ConfigureOptions {
                stored_config: Some(stored),
                env,
                ..ConfigureOptions::default()
            }
            .with_node_model(HeuristicNode::Vet, "google:gemini-decision-1");

            let resolved = execute_configure(&opts, None)
                .expect("resolve 4-level cascade")
                .config
                .expect("config");

            assert_eq!(resolved.provider, "typesafe");
            assert_eq!(
                resolved.node(HeuristicNode::Apprise),
                Some(&ResolvedNodeConfig::new("typesafe:jev-latest"))
            );
            assert_eq!(
                resolved.node(HeuristicNode::Docket),
                Some(&ResolvedNodeConfig::with_thinking(
                    "google:gemini-2.5-flash-lite",
                    Some(ThinkingLevel::Medium),
                    None
                ))
            );
            assert_eq!(
                resolved.node(HeuristicNode::Admit),
                Some(&ResolvedNodeConfig::with_thinking(
                    "google:gemini-4-hydrogen",
                    Some(ThinkingLevel::Medium),
                    None
                ))
            );
            assert_eq!(
                resolved.node(HeuristicNode::Vet),
                Some(&ResolvedNodeConfig::new("google:gemini-decision-1"))
            );
            assert_eq!(
                resolved.node(HeuristicNode::Audit),
                Some(&ResolvedNodeConfig::with_thinking(
                    "google:gemini-2.5-pro",
                    None,
                    Some(2048)
                ))
            );
        }

        #[test]
        fn test_partial_closure_caseload_chaining_preserves_upstream_nodes_and_resolves_audit_jit()
        {
            let mut ts_env = BTreeMap::new();
            ts_env.insert(ENV_TYPESAFE_API_KEY.to_string(), "ts-key".to_string());
            let after_vet = execute_configure(
                &ConfigureOptions {
                    provider: Some("typesafe".to_string()),
                    scope: ConfigureScope::closure([
                        HeuristicNode::Docket,
                        HeuristicNode::Admit,
                        HeuristicNode::Vet,
                    ]),
                    env: ts_env,
                    ..ConfigureOptions::default()
                },
                None,
            )
            .expect("vet configure");

            let after_audit = execute_configure(
                &ConfigureOptions {
                    scope: ConfigureScope::closure([HeuristicNode::Audit]),
                    env: env_with_google_key(),
                    ..ConfigureOptions::default()
                }
                .with_node_model(HeuristicNode::Audit, DEFAULT_GOOGLE_GENERATOR_MODEL),
                Some(after_vet),
            )
            .expect("downstream audit JIT configure");

            let cfg = after_audit.config.expect("config");
            assert_eq!(cfg.provider, "typesafe");
            assert_eq!(
                cfg.model_for(HeuristicNode::Docket),
                Some("typesafe:jev-latest")
            );
            assert_eq!(
                cfg.model_for(HeuristicNode::Vet),
                Some("typesafe:jev-latest")
            );
            assert_eq!(
                cfg.model_for(HeuristicNode::Audit),
                Some(DEFAULT_GOOGLE_GENERATOR_MODEL)
            );
        }

        #[test]
        fn test_discriminative_audit_and_discriminative_node_thinking_fail_fast() {
            let err = execute_configure(
                &ConfigureOptions {
                    env: env_with_both_keys(),
                    ..ConfigureOptions::default()
                }
                .with_node_model(HeuristicNode::Audit, "typesafe:jev-latest"),
                None,
            )
            .expect_err("discriminator on audit must fail");
            assert!(matches!(
                err,
                ConfigureError::DiscriminativeAuditUnsupported { .. }
            ));
            assert!(err.to_string().contains("Hint:"));

            let mut stored_nodes = BTreeMap::new();
            stored_nodes.insert(
                HeuristicNode::Docket,
                StoredNodeConfig {
                    thinking_level: Some("low".to_string()),
                    ..StoredNodeConfig::default()
                },
            );
            let think_err = execute_configure(
                &ConfigureOptions {
                    env: env_with_google_key(),
                    stored_config: Some(StoredConfig {
                        nodes: stored_nodes,
                        ..StoredConfig::default()
                    }),
                    ..ConfigureOptions::default()
                },
                None,
            )
            .expect_err("thinking on discriminator node must fail");
            assert!(matches!(
                think_err,
                ConfigureError::DiscriminativeNodeThinkingUnsupported { .. }
            ));
            assert!(think_err.to_string().contains("Hint:"));
        }
    }
}
