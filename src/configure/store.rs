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

//! Host user-level configuration file (`~/.config/canon-clerk/config.json`) loader, `{ providers, defaults, nodes }`
//! schema validators, and workspace/permission security guards (`docs/architecture/nodes/configure.md` §3–4).

use crate::caseload::{DEFAULT_PROVIDER, HeuristicNode, TYPESAFE_PROVIDER};
use crate::configure::{ConfigureError, ProviderRegistry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Provider profile entry under `providers.<slug>` in `~/.config/canon-clerk/config.json`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StoredProviderConfig {
    /// Wire protocol driver (`"gemini"` or `"systemone"`). Optional on built-in slugs; required on custom profiles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,

    /// Endpoint base URL (`http://` or `https://`). Optional on built-in slugs; required on custom profiles.
    #[serde(
        default,
        rename = "baseUrl",
        alias = "base_url",
        skip_serializing_if = "Option::is_none"
    )]
    pub base_url: Option<String>,

    /// Provider API key (`apiKey` or `api_key`).
    #[serde(
        default,
        rename = "apiKey",
        alias = "api_key",
        skip_serializing_if = "Option::is_none"
    )]
    pub api_key: Option<String>,

    /// Default canonical `<slug>:<model>` `Discriminator` specifier for this profile.
    #[serde(
        default,
        rename = "defaultDiscriminator",
        alias = "default_discriminator",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_discriminator: Option<String>,

    /// Default canonical `<slug>:<model>` `Generator` specifier for this profile (`gemini` protocol only).
    #[serde(
        default,
        rename = "defaultGenerator",
        alias = "default_generator",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_generator: Option<String>,
}

/// Global default operational knobs under `defaults` in `~/.config/canon-clerk/config.json`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StoredDefaultsConfig {
    /// Active default provider slug (`"google"` or `"typesafe"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Default qualitative thinking level (`"minimal"`, `"low"`, `"medium"`, `"high"`).
    #[serde(
        default,
        rename = "thinkingLevel",
        alias = "thinking_level",
        skip_serializing_if = "Option::is_none"
    )]
    pub thinking_level: Option<String>,

    /// Default reasoning budget ceiling in tokens.
    #[serde(
        default,
        rename = "reasoningBudget",
        alias = "reasoning_budget",
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning_budget: Option<u32>,

    /// Default per-attempt wire timeout in milliseconds for heuristic evaluation calls (`>= 1`).
    #[serde(
        default,
        rename = "timeoutMs",
        alias = "timeout_ms",
        skip_serializing_if = "Option::is_none"
    )]
    pub timeout_ms: Option<u64>,

    /// Default maximum retry attempts per heuristic provider call (`>= 0`).
    #[serde(
        default,
        rename = "maxRetries",
        alias = "max_retries",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_retries: Option<u32>,

    /// Default maximum per-wait cooldown or retry backoff in milliseconds (`>= 0`).
    #[serde(
        default,
        rename = "maxBackoffMs",
        alias = "max_backoff_ms",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_backoff_ms: Option<u64>,
}

/// Per-node model and operational tuning overrides under `nodes.<node>` in `~/.config/canon-clerk/config.json`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StoredNodeConfig {
    /// Canonical `<provider>:<model>` specifier pinned for this node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Stage classification score threshold in `[0.0, 1.0]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,

    /// Maximum candidate canons per call (`apprise` and `docket` only).
    #[serde(
        default,
        rename = "batchSize",
        alias = "batch_size",
        skip_serializing_if = "Option::is_none"
    )]
    pub batch_size: Option<usize>,

    /// Maximum concurrent in-flight model calls for this stage (`>= 1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<usize>,

    /// Per-node qualitative thinking level (`"minimal"`, `"low"`, `"medium"`, `"high"`).
    #[serde(
        default,
        rename = "thinkingLevel",
        alias = "thinking_level",
        skip_serializing_if = "Option::is_none"
    )]
    pub thinking_level: Option<String>,

    /// Per-node reasoning budget ceiling in tokens.
    #[serde(
        default,
        rename = "reasoningBudget",
        alias = "reasoning_budget",
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning_budget: Option<u32>,
}

impl Eq for StoredNodeConfig {}

/// Orthogonal user-level configuration file schema (`~/.config/canon-clerk/config.json`).
///
/// Restricted strictly to `providers`, `defaults`, and `nodes` namespaces per
/// `.canons/configuration/configuration-file-formats-must-conform-to-industry-standards.md`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StoredConfig {
    /// Provider-keyed authentication settings (e.g. `providers.google.apiKey`).
    #[serde(default)]
    pub providers: BTreeMap<String, StoredProviderConfig>,

    /// Global provider, thinking, and retry defaults.
    #[serde(default)]
    pub defaults: StoredDefaultsConfig,

    /// Heuristic node overrides keyed by canonical node name (`apprise`, `docket`, `admit`, `vet`, `audit`).
    #[serde(default)]
    pub nodes: BTreeMap<HeuristicNode, StoredNodeConfig>,
}

impl StoredConfig {
    /// Returns the non-empty API key configured under `providers.<slug>.apiKey`.
    pub(crate) fn provider_api_key(&self, slug: &str) -> Option<&str> {
        self.providers
            .get(slug)
            .and_then(|p| p.api_key.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// Returns the non-empty Google API key stored under `providers.google.apiKey`.
    pub fn google_api_key(&self) -> Option<&str> {
        self.provider_api_key(DEFAULT_PROVIDER)
    }

    /// Returns the non-empty TypeSafe AI API key stored under `providers.typesafe.apiKey`.
    pub fn typesafe_api_key(&self) -> Option<&str> {
        self.provider_api_key(TYPESAFE_PROVIDER)
    }

    /// Returns the non-empty default provider slug under `defaults.provider`.
    pub fn default_provider(&self) -> Option<&str> {
        self.defaults
            .provider
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// Returns the [`StoredNodeConfig`] entry for `node`, if present.
    pub fn node(&self, node: HeuristicNode) -> Option<&StoredNodeConfig> {
        self.nodes.get(&node)
    }

    /// Returns the non-empty `nodes.<node>.model` specifier, if present.
    pub fn node_model(&self, node: HeuristicNode) -> Option<&str> {
        self.nodes
            .get(&node)
            .and_then(|n| n.model.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// Returns `nodes.<node>.threshold`, if present.
    pub fn node_threshold(&self, node: HeuristicNode) -> Option<f64> {
        self.nodes.get(&node).and_then(|n| n.threshold)
    }

    /// Returns `nodes.<node>.batchSize`, if present.
    pub fn node_batch_size(&self, node: HeuristicNode) -> Option<usize> {
        self.nodes.get(&node).and_then(|n| n.batch_size)
    }

    /// Returns `nodes.<node>.concurrency`, if present.
    pub fn node_concurrency(&self, node: HeuristicNode) -> Option<usize> {
        self.nodes.get(&node).and_then(|n| n.concurrency)
    }

    /// Validates semantic constraints on parsed [`StoredConfig`] fields (custom `providers.<slug>` profiles and `batchSize` stage scoping).
    pub(crate) fn validate_schema(&self, source_path: Option<&str>) -> Result<(), ConfigureError> {
        let path = source_path.unwrap_or("~/.config/canon-clerk/config.json");
        ProviderRegistry::from_stored_with_path(Some(self), Some(path))?;
        if self.defaults.timeout_ms == Some(0) {
            return Err(ConfigureError::MalformedConfigFile {
                path: path.to_string(),
                message:
                    "`defaults.timeoutMs` must be a positive integer in milliseconds (`>= 1`)."
                        .to_string(),
            });
        }
        for (node, cfg) in &self.nodes {
            if cfg.batch_size.is_some() && !node.supports_batch_size() {
                return Err(ConfigureError::MalformedConfigFile {
                    path: path.to_string(),
                    message: format!(
                        "`nodes.{node}.batchSize` is not supported on `{node}` (`batchSize` applies only to `nodes.apprise` and `nodes.docket`)."
                    ),
                });
            }
        }
        Ok(())
    }
}

/// Resolves the canonical user-level configuration file path outside the workspace:
/// - `$XDG_CONFIG_HOME/canon-clerk/config.json` when `XDG_CONFIG_HOME` is set and non-empty
/// - `$HOME/.config/canon-clerk/config.json` on Unix / macOS
/// - `%APPDATA%\canon-clerk\config.json` on Windows
pub fn resolve_default_config_path(env: &BTreeMap<String, String>) -> Option<PathBuf> {
    if let Some(xdg) = env
        .get("XDG_CONFIG_HOME")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return Some(PathBuf::from(xdg).join("canon-clerk").join("config.json"));
    }

    if let Some(home) = env.get("HOME").map(|s| s.trim()).filter(|s| !s.is_empty()) {
        return Some(
            PathBuf::from(home)
                .join(".config")
                .join("canon-clerk")
                .join("config.json"),
        );
    }

    if let Some(appdata) = env
        .get("APPDATA")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return Some(
            PathBuf::from(appdata)
                .join("canon-clerk")
                .join("config.json"),
        );
    }

    None
}

/// Loads and validates a user-level configuration file from `config_path`.
///
/// Enforces workspace isolation, POSIX owner-only permissions (`0600`/`0400`), and strict
/// `{ providers, defaults, nodes }` schema validation (`configure.md` §3).
pub fn load_host_config_file(
    config_path: &Path,
    workspace_root: Option<&Path>,
) -> Result<Option<StoredConfig>, ConfigureError> {
    if let Some(ws_root) = workspace_root
        && is_path_inside_workspace(config_path, ws_root)
    {
        return Err(ConfigureError::ConfigInsideWorkspace {
            path: config_path.display().to_string(),
        });
    }

    if !config_path.exists() {
        return Ok(None);
    }

    verify_posix_permissions(config_path)?;

    let path_str = config_path.display().to_string();
    let raw =
        fs::read_to_string(config_path).map_err(|err| ConfigureError::MalformedConfigFile {
            path: path_str.clone(),
            message: err.to_string(),
        })?;

    let parsed = parse_stored_config_str(&raw, &path_str)?;
    Ok(Some(parsed))
}

fn retired_root_key_migration_hint(key: &str) -> Option<&'static str> {
    match key {
        "screenerModel" | "screener_model" => Some(
            "Move screening model pins under `nodes.<node>.model` (e.g. `\"nodes\": { \"docket\": { \"model\": \"typesafe:jev-latest\" } }`) or set `\"defaults\": { \"provider\": \"typesafe\" }`.",
        ),
        "auditorModel" | "auditor_model" => Some(
            "Move the judicial model pin to `nodes.audit.model` (e.g. `\"nodes\": { \"audit\": { \"model\": \"google:gemini-4-carbon\" } }`).",
        ),
        "thinkingLevel" | "thinking_level" | "reasoningBudget" | "reasoning_budget" => Some(
            "Move extended-thinking defaults under `defaults.thinkingLevel` / `defaults.reasoningBudget` (or per-node `nodes.<node>.thinkingLevel` / `nodes.<node>.reasoningBudget`).",
        ),
        "maxRetries" | "max_retries" | "maxBackoffMs" | "max_backoff_ms" => Some(
            "Move transport retry bounds under `defaults.maxRetries` and `defaults.maxBackoffMs`.",
        ),
        "appriseBatchSize"
        | "apprise_batch_size"
        | "appriseConcurrency"
        | "apprise_concurrency"
        | "docketBatchSize"
        | "docket_batch_size"
        | "docketConcurrency"
        | "docket_concurrency"
        | "admitConcurrency"
        | "admit_concurrency"
        | "vetConcurrency"
        | "vet_concurrency"
        | "auditConcurrency"
        | "audit_concurrency"
        | "maxCanonsPerCall"
        | "max_canons_per_call"
        | "maxConcurrentCalls"
        | "max_concurrent_calls" => Some(
            "Move stage batching and concurrency settings under `nodes.<node>.batchSize` (`apprise`, `docket`) and `nodes.<node>.concurrency`.",
        ),
        _ => None,
    }
}

pub(crate) fn parse_stored_config_str(
    raw: &str,
    path_str: &str,
) -> Result<StoredConfig, ConfigureError> {
    let val: Value =
        serde_json::from_str(raw).map_err(|err| ConfigureError::MalformedConfigFile {
            path: path_str.to_string(),
            message: err.to_string(),
        })?;

    let Some(root_obj) = val.as_object() else {
        return Err(ConfigureError::MalformedConfigFile {
            path: path_str.to_string(),
            message: "top-level configuration JSON value must be an object (`{ \"providers\": ..., \"defaults\": ..., \"nodes\": ... }`).".to_string(),
        });
    };

    for key in root_obj.keys() {
        if let Some(hint) = retired_root_key_migration_hint(key) {
            return Err(ConfigureError::RetiredConfigKey {
                path: path_str.to_string(),
                key: key.clone(),
                hint: hint.to_string(),
            });
        }
        if !matches!(key.as_str(), "providers" | "defaults" | "nodes") {
            return Err(ConfigureError::MalformedConfigFile {
                path: path_str.to_string(),
                message: format!(
                    "unknown top-level key `{key}` (expected only `providers`, `defaults`, and `nodes`)."
                ),
            });
        }
    }

    if let Some(nodes_obj) = root_obj.get("nodes").and_then(Value::as_object) {
        for (node_key, node_val) in nodes_obj {
            if node_key.parse::<HeuristicNode>().is_err() {
                return Err(ConfigureError::MalformedConfigFile {
                    path: path_str.to_string(),
                    message: format!(
                        "unknown heuristic node `{node_key}` under `nodes` (expected `apprise`, `docket`, `admit`, `vet`, or `audit`)."
                    ),
                });
            }
            if let Some(model_val) = node_val.as_object().and_then(|o| o.get("model"))
                && !model_val.is_null()
                && !model_val.is_string()
            {
                return Err(ConfigureError::RetiredConfigKey {
                    path: path_str.to_string(),
                    key: format!("nodes.{node_key}.model"),
                    hint: "Specify a canonical `<provider>:<model>` string (for example, `\"google:gemini-decision-1\"` or `\"google:gemini-4-carbon\"`) rather than an object.".to_string(),
                });
            }
        }
    }

    let parsed: StoredConfig =
        serde_json::from_value(val).map_err(|err| ConfigureError::MalformedConfigFile {
            path: path_str.to_string(),
            message: err.to_string(),
        })?;
    parsed.validate_schema(Some(path_str))?;
    Ok(parsed)
}

fn is_path_inside_workspace(config_path: &Path, workspace_root: &Path) -> bool {
    let canonical_ws =
        fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());
    let canonical_cfg = fs::canonicalize(config_path).unwrap_or_else(|_| config_path.to_path_buf());
    canonical_cfg.starts_with(&canonical_ws)
}

#[cfg(unix)]
fn verify_posix_permissions(config_path: &Path) -> Result<(), ConfigureError> {
    use std::os::unix::fs::PermissionsExt;

    let metadata =
        fs::metadata(config_path).map_err(|err| ConfigureError::MalformedConfigFile {
            path: config_path.display().to_string(),
            message: err.to_string(),
        })?;
    let mode = metadata.permissions().mode() & 0o777;
    if (mode & 0o077) != 0 {
        return Err(ConfigureError::InsecureConfigPermissions {
            path: config_path.display().to_string(),
            mode_octal: format!("0o{mode:03o}"),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn verify_posix_permissions(_config_path: &Path) -> Result<(), ConfigureError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("canon-clerk-cfg-{label}-{nanos}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[cfg(unix)]
    fn write_config_with_mode(path: &Path, content: &str, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, content).expect("write config");
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("set mode");
    }

    mod path_and_schema {
        use super::*;

        #[test]
        fn test_resolve_default_config_path_prefers_xdg_then_home() {
            let mut env = BTreeMap::new();
            env.insert("HOME".to_string(), "/home/alice".to_string());
            assert_eq!(
                resolve_default_config_path(&env),
                Some(PathBuf::from("/home/alice/.config/canon-clerk/config.json"))
            );

            env.insert("XDG_CONFIG_HOME".to_string(), "/custom/xdg".to_string());
            assert_eq!(
                resolve_default_config_path(&env),
                Some(PathBuf::from("/custom/xdg/canon-clerk/config.json"))
            );
        }

        #[test]
        fn test_nonexistent_config_file_returns_none() {
            let dir = unique_temp_dir("missing");
            let cfg_path = dir.join("config.json");
            let loaded = load_host_config_file(&cfg_path, None).expect("missing file is ok");
            assert!(loaded.is_none());
            let _ = fs::remove_dir_all(&dir);
        }

        #[cfg(unix)]
        #[test]
        fn test_loads_valid_0600_config_file_with_providers_defaults_and_nodes() {
            let dir = unique_temp_dir("valid0600");
            let cfg_path = dir.join("config.json");
            let json = r#"{
                "providers": {
                    "google": { "apiKey": "synthetic-stored-key-from-config-json" },
                    "typesafe": { "apiKey": "synthetic-stored-typesafe-key" }
                },
                "defaults": {
                    "provider": "google",
                    "thinkingLevel": "low",
                    "maxRetries": 6,
                    "maxBackoffMs": 60000
                },
                "nodes": {
                    "apprise": { "batchSize": 15, "concurrency": 3 },
                    "docket": { "model": "typesafe:jev-latest", "batchSize": 48, "concurrency": 4 },
                    "admit": { "model": "typesafe:jev-latest", "concurrency": 6 },
                    "vet": { "model": "google:gemini-decision-1", "threshold": 0.85, "concurrency": 7 },
                    "audit": { "model": "google:gemini-4-carbon", "reasoningBudget": 4096, "concurrency": 5 }
                }
            }"#;
            write_config_with_mode(&cfg_path, json, 0o600);

            let loaded = load_host_config_file(&cfg_path, None)
                .expect("load valid 0600 config")
                .expect("config present");
            assert_eq!(
                loaded.google_api_key(),
                Some("synthetic-stored-key-from-config-json")
            );
            assert_eq!(
                loaded.typesafe_api_key(),
                Some("synthetic-stored-typesafe-key")
            );
            assert_eq!(loaded.default_provider(), Some("google"));
            assert_eq!(loaded.defaults.max_retries, Some(6));
            assert_eq!(loaded.defaults.max_backoff_ms, Some(60_000));
            assert_eq!(
                loaded.node_model(HeuristicNode::Docket),
                Some("typesafe:jev-latest")
            );
            assert_eq!(loaded.node_batch_size(HeuristicNode::Apprise), Some(15));
            assert_eq!(loaded.node_concurrency(HeuristicNode::Admit), Some(6));
            assert_eq!(loaded.node_threshold(HeuristicNode::Vet), Some(0.85));
            assert_eq!(
                loaded
                    .node(HeuristicNode::Audit)
                    .and_then(|n| n.reasoning_budget),
                Some(4096)
            );

            let _ = fs::remove_dir_all(&dir);
        }

        #[test]
        fn test_rejects_retired_flat_root_keys_and_unknown_keys_with_hint() {
            for retired_json in [
                r#"{"screenerModel":"google:gemini-decision-1"}"#,
                r#"{"auditorModel":"google:gemini-4-carbon"}"#,
                r#"{"thinkingLevel":"low"}"#,
                r#"{"reasoningBudget":2048}"#,
                r#"{"docketBatchSize":25}"#,
                r#"{"admitConcurrency":4}"#,
                r#"{"maxCanonsPerCall":20}"#,
                r#"{"maxConcurrentCalls":4}"#,
                r#"{"maxRetries":3}"#,
                r#"{"maxBackoffMs":45000}"#,
                r#"{"nodes":{"docket":{"model":{"provider":"google","model":"gemini-decision-1"}}}}"#,
            ] {
                let err = parse_stored_config_str(retired_json, "/tmp/config.json")
                    .expect_err(&format!("must reject `{retired_json}`"));
                assert!(matches!(err, ConfigureError::RetiredConfigKey { .. }));
                assert!(err.to_string().contains("Hint:"));
            }

            for bad_json in [
                r#"{"typoRoot":true}"#,
                r#"{"nodes":{"screener":{"model":"google:gemini-decision-1"}}}"#,
                r#"{"nodes":{"audit":{"batchSize":10}}}"#,
                r#"{"defaults":{"unknownField":1}}"#,
                r#"{"providers":{"local-dev":{"baseUrl":"http://127.0.0.1:8080/v1"}}}"#,
                r#"{"providers":{"local-dev":{"protocol":"systemone"}}}"#,
                r#"{"providers":{"local-dev":{"protocol":"openai","baseUrl":"http://127.0.0.1:8080/v1"}}}"#,
                r#"{"providers":{"local-dev":{"protocol":"systemone","baseUrl":"ftp://127.0.0.1:8080"}}}"#,
                r#"{"providers":{"google":{"protocol":"systemone"}}}"#,
                r#"{"providers":{"local-dev":{"protocol":"systemone","baseUrl":"http://127.0.0.1:8080/v1","defaultGenerator":"local-dev:kev-0.8b"}}}"#,
                r#"{"providers":{"local-dev":{"protocol":"systemone","baseUrl":"http://127.0.0.1:8080/v1","defaultDiscriminator":"typesafe:jev-latest"}}}"#,
                r#"{"providers":{"google-corp":{"protocol":"gemini","baseUrl":"https://gw.example.com/v1beta","defaultGenerator":"google-corp:gemini-decision-1"}}}"#,
                r#"{"providers":{"Bad_Slug":{"protocol":"systemone","baseUrl":"http://127.0.0.1:8080/v1"}}}"#,
            ] {
                let err = parse_stored_config_str(bad_json, "/tmp/config.json")
                    .expect_err(&format!("must reject `{bad_json}`"));
                assert!(matches!(err, ConfigureError::MalformedConfigFile { .. }));
                assert!(err.to_string().contains("Hint:"));
            }
        }
    }

    mod security_guards {
        use super::*;

        #[cfg(unix)]
        #[test]
        fn test_rejects_world_or_group_readable_config_file() {
            let dir = unique_temp_dir("insecure0644");
            let cfg_path = dir.join("config.json");
            write_config_with_mode(
                &cfg_path,
                r#"{"providers":{"google":{"apiKey":"secret"}}}"#,
                0o644,
            );

            let err =
                load_host_config_file(&cfg_path, None).expect_err("0o644 config must be rejected");
            assert!(matches!(
                err,
                ConfigureError::InsecureConfigPermissions { .. }
            ));
            assert!(err.to_string().contains("0o644"));
            assert!(err.to_string().contains("chmod 600"));

            let _ = fs::remove_dir_all(&dir);
        }

        #[cfg(unix)]
        #[test]
        fn test_rejects_config_file_located_inside_workspace() {
            let ws_dir = unique_temp_dir("workspace");
            let inside_path = ws_dir.join(".canon-clerk").join("config.json");
            write_config_with_mode(
                &inside_path,
                r#"{"providers":{"google":{"apiKey":"secret"}}}"#,
                0o600,
            );

            let err = load_host_config_file(&inside_path, Some(&ws_dir))
                .expect_err("workspace-resident config must be rejected");
            assert!(matches!(err, ConfigureError::ConfigInsideWorkspace { .. }));
            assert!(err.to_string().contains("Hint:"));

            let _ = fs::remove_dir_all(&ws_dir);
        }
    }
}
