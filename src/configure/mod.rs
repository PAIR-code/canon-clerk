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
mod providers;
pub mod store;
mod thinking;

pub use error::ConfigureError;
pub(crate) use error::check_retired_env_vars;
pub use providers::{
    ModelCapability, PROVIDER_CAPABILITIES, ProviderCapability, provider_capability,
};
pub(crate) use providers::{
    resolve_active_provider, resolve_provider_api_key, verified_credential_env_vars,
    verify_models_credentials,
};
pub use store::{
    StoredConfig, StoredDefaultsConfig, StoredNodeConfig, StoredProviderConfig,
    load_host_config_file, resolve_default_config_path,
};
pub use thinking::parse_thinking_level;

use crate::caseload::{
    Caseload, CaseloadConfig, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, DEFAULT_GOOGLE_GENERATOR_MODEL,
    DEFAULT_PROVIDER, DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL, HeuristicNode, ResolvedNodeConfig,
    TYPESAFE_PROVIDER, ThinkingLevel,
};
use std::collections::{BTreeMap, BTreeSet};
use thinking::{resolve_global_thinking_layers, resolve_node_thinking_options};

/// Environment variable name for overriding the active default provider (`google`, `typesafe`).
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

/// Environment variable name for overriding the qualitative thinking level (`minimal|low|medium|high`).
pub const ENV_THINKING_LEVEL: &str = "CANON_CLERK_THINKING_LEVEL";

/// Environment variable name for overriding the maximum reasoning budget in tokens.
pub const ENV_REASONING_BUDGET: &str = "CANON_CLERK_REASONING_BUDGET";

/// Target node resolution and credential verification scope for [`execute_configure`] (`configure.md` §4).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConfigureScope {
    /// Standalone `configure` and `probe`: resolves all provisioned heuristic nodes for the active
    /// configuration (all 5 on `google`; the 4 discriminative nodes on `typesafe` when `audit` is unpinned).
    #[default]
    AllProvisioned,
    /// Converged evaluation closure: resolves and verifies credentials strictly for the heuristic
    /// nodes executed during the current invocation.
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
}

/// Extracts the canonical provider slug (`"google"`, `"typesafe"`, etc.) from a `<provider>:<model>` specifier.
pub(crate) fn model_provider(specifier: &str) -> &str {
    match specifier.trim().split_once(':') {
        Some((prov, _)) if !prov.is_empty() => prov,
        _ => DEFAULT_PROVIDER,
    }
}

fn suggested_specifier_example(trimmed: &str) -> String {
    if is_valid_gemini_model_name(trimmed) {
        format!("{DEFAULT_PROVIDER}:{trimmed}")
    } else if is_valid_jev_model_name(trimmed) {
        format!("{TYPESAFE_PROVIDER}:{trimmed}")
    } else {
        DEFAULT_GOOGLE_DISCRIMINATOR_MODEL.to_string()
    }
}

/// Validates and normalizes a canonical `<provider>:<model>` specifier (`configure.md` §3.4).
///
/// Bare short-name prefix inference (`gemini-*`, `jev-*`) is strictly disallowed and fails fast
/// with [`ConfigureError::InvalidModelSpecifier`] and an actionable example hint.
pub fn normalize_model_specifier(raw: &str) -> Result<String, ConfigureError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ConfigureError::InvalidModelSpecifier {
            specifier: raw.to_string(),
            reason: "model specifier cannot be empty.".to_string(),
            example: DEFAULT_GOOGLE_DISCRIMINATOR_MODEL.to_string(),
        });
    }

    if trimmed.chars().any(char::is_whitespace) {
        return Err(ConfigureError::InvalidModelSpecifier {
            specifier: raw.to_string(),
            reason: "model specifier must not contain whitespace.".to_string(),
            example: DEFAULT_GOOGLE_DISCRIMINATOR_MODEL.to_string(),
        });
    }

    let Some((provider, model_part)) = trimmed.split_once(':') else {
        let example = suggested_specifier_example(trimmed);
        return Err(ConfigureError::InvalidModelSpecifier {
            specifier: raw.to_string(),
            reason: format!("expected `<provider>:<model>` format (for example, `{example}`)."),
            example,
        });
    };

    if provider.is_empty() || model_part.is_empty() {
        return Err(ConfigureError::InvalidModelSpecifier {
            specifier: raw.to_string(),
            reason: "provider and model segments around `:` must both be non-empty.".to_string(),
            example: DEFAULT_GOOGLE_DISCRIMINATOR_MODEL.to_string(),
        });
    }
    if model_part.contains(':') {
        return Err(ConfigureError::InvalidModelSpecifier {
            specifier: raw.to_string(),
            reason: "model specifier must contain at most one `:` delimiter.".to_string(),
            example: DEFAULT_GOOGLE_DISCRIMINATOR_MODEL.to_string(),
        });
    }

    match provider {
        DEFAULT_PROVIDER => {
            if is_valid_gemini_model_name(model_part) {
                Ok(format!("{DEFAULT_PROVIDER}:{model_part}"))
            } else {
                Err(ConfigureError::InvalidModelSpecifier {
                    specifier: raw.to_string(),
                    reason: format!(
                        "unrecognized Google model `{model_part}` (expected `gemini-decision-1`, `gemini-4-hydrogen`, `gemini-4-carbon`, or another exact `gemini-*` upstream ID)."
                    ),
                    example: DEFAULT_GOOGLE_GENERATOR_MODEL.to_string(),
                })
            }
        }
        TYPESAFE_PROVIDER => {
            if is_valid_jev_model_name(model_part) {
                Ok(format!("{TYPESAFE_PROVIDER}:{model_part}"))
            } else {
                Err(ConfigureError::InvalidModelSpecifier {
                    specifier: raw.to_string(),
                    reason: format!(
                        "unrecognized TypeSafe model `{model_part}` (expected `jev-*`, e.g. `jev-latest`)."
                    ),
                    example: DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL.to_string(),
                })
            }
        }
        _ => Err(ConfigureError::UnsupportedProvider {
            provider: provider.to_string(),
            specifier: trimmed.to_string(),
        }),
    }
}

pub(crate) fn is_gemini_decision_model_name(name: &str) -> bool {
    is_valid_prefixed_model_name(name, "gemini-decision-")
}

/// Classifies a canonical `<provider>:<model>` specifier into its [`ModelCapability`].
pub fn model_capability(specifier: &str) -> ModelCapability {
    let trimmed = specifier.trim();
    match trimmed.split_once(':') {
        Some((DEFAULT_PROVIDER, model_part)) if is_gemini_decision_model_name(model_part) => {
            ModelCapability::Discriminator
        }
        Some((TYPESAFE_PROVIDER, model_part)) if !model_part.is_empty() => {
            ModelCapability::Discriminator
        }
        _ => ModelCapability::Generator,
    }
}

/// Returns `true` when `specifier` names a `Discriminator` `Noul` decision model
/// (`google:gemini-decision-*` or `typesafe:*`).
pub fn is_discriminative_model(specifier: &str) -> bool {
    model_capability(specifier) == ModelCapability::Discriminator
}

fn is_valid_gemini_model_name(name: &str) -> bool {
    if name.starts_with("gemini-decision") {
        return is_gemini_decision_model_name(name);
    }
    is_valid_prefixed_model_name(name, "gemini-")
}

fn is_valid_jev_model_name(name: &str) -> bool {
    is_valid_prefixed_model_name(name, "jev-")
}

fn is_valid_prefixed_model_name(name: &str, prefix: &str) -> bool {
    let Some(suffix) = name.strip_prefix(prefix) else {
        return false;
    };
    !suffix.is_empty()
        && suffix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_')
}

/// Resolves the Google Gemini API key from explicit overrides, `GEMINI_API_KEY` environment variable,
/// or `providers.google.apiKey` in `config.json`.
pub fn resolve_gemini_api_key(options: &ConfigureOptions) -> Result<String, ConfigureError> {
    resolve_provider_api_key(DEFAULT_PROVIDER, options)
}

/// Resolves the TypeSafe AI API key from explicit overrides, `TYPESAFE_API_KEY` environment variable,
/// or `providers.typesafe.apiKey` in `config.json`.
pub(crate) fn resolve_typesafe_api_key(
    options: &ConfigureOptions,
) -> Result<String, ConfigureError> {
    resolve_provider_api_key(TYPESAFE_PROVIDER, options)
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
    options: &ConfigureOptions,
    stored: Option<&StoredConfig>,
    existing: Option<&CaseloadConfig>,
    preserve_existing_model: bool,
) -> Result<Option<String>, ConfigureError> {
    if let Some(raw_cli) = options.node_models.get(&node) {
        return normalize_model_specifier(raw_cli).map(Some);
    }
    if let Some(raw_env) = options
        .env
        .get(node.env_model_var())
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return normalize_model_specifier(raw_env).map(Some);
    }
    if let Some(raw_stored) = stored.and_then(|s| s.node_model(node)) {
        return normalize_model_specifier(raw_stored).map(Some);
    }
    if preserve_existing_model
        && let Some(raw_existing) = existing
            .and_then(|c| c.model_for(node))
            .map(str::trim)
            .filter(|s| !s.is_empty())
    {
        return normalize_model_specifier(raw_existing).map(Some);
    }
    Ok(None)
}

/// Executes the pure, deterministic `configure` stage and populates `.config` on the [`Caseload`].
pub fn execute_configure(
    options: &ConfigureOptions,
    caseload: Option<Caseload>,
) -> Result<Caseload, ConfigureError> {
    let caseload = caseload.unwrap_or_default();
    check_retired_env_vars(&options.env)?;
    if let Some(stored) = options.stored_config.as_ref() {
        stored.validate_schema(None)?;
    }

    if can_fast_forward_existing_config(options, caseload.config.as_ref()) {
        return Ok(caseload);
    }

    let stored = options.stored_config.as_ref();
    let existing = caseload.config.as_ref();
    let active_cap = resolve_active_provider(options, stored, existing)?;
    let global_thinking = resolve_global_thinking_layers(options, stored)?;

    let provider_explicitly_switched = existing.is_some_and(|c| c.provider != active_cap.slug);
    let preserve_existing_model = !provider_explicitly_switched;

    let mut nodes: BTreeMap<HeuristicNode, ResolvedNodeConfig> = match &options.scope {
        ConfigureScope::AllProvisioned => BTreeMap::new(),
        ConfigureScope::Closure(_) => existing.map(|c| c.nodes.clone()).unwrap_or_default(),
    };

    let target_nodes: Vec<HeuristicNode> = match &options.scope {
        ConfigureScope::AllProvisioned => HeuristicNode::ALL.to_vec(),
        ConfigureScope::Closure(required) => {
            let mut combined: BTreeSet<HeuristicNode> = required.clone();
            combined.extend(options.node_models.keys().copied());
            combined.into_iter().collect()
        }
    };

    let mut models_requiring_credentials = Vec::new();
    for node in target_nodes {
        let pinned =
            resolve_pinned_node_model(node, options, stored, existing, preserve_existing_model)?;
        let model = match pinned {
            Some(m) => m,
            None => match active_cap.default_model_for(node) {
                Some(def) => def.to_string(),
                None => match &options.scope {
                    ConfigureScope::AllProvisioned => continue,
                    ConfigureScope::Closure(required) if required.contains(&node) => {
                        return Err(ConfigureError::UnprovisionedAuditModel {
                            provider: active_cap.slug.to_string(),
                        });
                    }
                    ConfigureScope::Closure(_) => continue,
                },
            },
        };

        if node == HeuristicNode::Audit && is_discriminative_model(&model) {
            return Err(ConfigureError::DiscriminativeAuditUnsupported { specifier: model });
        }

        let existing_node = existing.and_then(|c| c.node(node));
        let (thinking_level, reasoning_budget) =
            resolve_node_thinking_options(node, &model, global_thinking, stored, existing_node)?;

        let verify_cred = match &options.scope {
            ConfigureScope::AllProvisioned => true,
            ConfigureScope::Closure(required) => {
                required.contains(&node) || options.node_models.contains_key(&node)
            }
        };
        if verify_cred {
            models_requiring_credentials.push(model.clone());
        }

        nodes.insert(
            node,
            ResolvedNodeConfig::with_thinking(model, thinking_level, reasoning_budget),
        );
    }

    verify_models_credentials(
        models_requiring_credentials.iter().map(String::as_str),
        options,
    )?;

    let config = CaseloadConfig {
        provider: active_cap.slug.to_string(),
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
            assert!(is_discriminative_model("typesafe:jev-latest"));
            assert!(!is_discriminative_model("google:gemini-4-carbon"));
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

            // 1. Standalone `configure` (`AllProvisioned`) with `--provider typesafe` succeeds without GEMINI_API_KEY!
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

            // 2. Invoking `audit` closure on `--provider typesafe` without `--audit-model` fails fast (exit 2)
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
        fn test_four_level_node_model_cascade_and_thinking_scoping() {
            let mut providers = BTreeMap::new();
            providers.insert(
                "google".to_string(),
                StoredProviderConfig {
                    api_key: Some("stored-google-key".to_string()),
                },
            );
            providers.insert(
                "typesafe".to_string(),
                StoredProviderConfig {
                    api_key: Some("stored-typesafe-key".to_string()),
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
            // Level 4 (provider default): apprise -> typesafe:jev-latest (discriminator -> thinking None)
            assert_eq!(
                resolved.node(HeuristicNode::Apprise),
                Some(&ResolvedNodeConfig::new("typesafe:jev-latest"))
            );
            // Level 3 (config.json nodes.docket): google:gemini-2.5-flash-lite (generator -> inherits defaults.thinkingLevel = medium)
            assert_eq!(
                resolved.node(HeuristicNode::Docket),
                Some(&ResolvedNodeConfig::with_thinking(
                    "google:gemini-2.5-flash-lite",
                    Some(ThinkingLevel::Medium),
                    None
                ))
            );
            // Level 2 (CANON_CLERK_ADMIT_MODEL): google:gemini-4-hydrogen (generator -> inherits defaults.thinkingLevel = medium)
            assert_eq!(
                resolved.node(HeuristicNode::Admit),
                Some(&ResolvedNodeConfig::with_thinking(
                    "google:gemini-4-hydrogen",
                    Some(ThinkingLevel::Medium),
                    None
                ))
            );
            // Level 1 (CLI --vet-model): google:gemini-decision-1 (discriminator -> thinking None)
            assert_eq!(
                resolved.node(HeuristicNode::Vet),
                Some(&ResolvedNodeConfig::new("google:gemini-decision-1"))
            );
            // Level 3 (nodes.audit): google:gemini-2.5-pro with per-node reasoningBudget=2048 superseding defaults.thinkingLevel
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
            // Simulate `canon-clerk vet --provider typesafe --output-caseload c.json`
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

            // Now run downstream `canon-clerk audit --caseload c.json --audit-model google:gemini-4-carbon`
            // in an environment that has ONLY GEMINI_API_KEY (upstream docket/admit/vet are already complete!)
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
                    thinking_level: Some("high".to_string()),
                    ..StoredNodeConfig::default()
                },
            );
            let err_thinking = execute_configure(
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
            .expect_err("per-node thinking on discriminator must fail");
            assert!(matches!(
                err_thinking,
                ConfigureError::DiscriminativeNodeThinkingUnsupported { .. }
            ));
            assert!(err_thinking.to_string().contains("Hint:"));
        }
    }
}
