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

//! Domain error taxonomy and retired environment variable guards for `configure`.

use super::{
    ENV_AUDIT_MODEL, ENV_GEMINI_API_KEY, ENV_PROVIDER, ENV_REASONING_BUDGET, ENV_THINKING_LEVEL,
    ENV_TYPESAFE_API_KEY, MissingCredentialSpec,
};
use crate::caseload::{
    DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, DEFAULT_GOOGLE_GENERATOR_MODEL,
    DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL, HeuristicNode,
};
use std::collections::BTreeMap;
use std::fmt;

/// Domain errors produced during `configure` execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigureError {
    /// Required provider API key credential is missing across CLI flags, environment variables, and `config.json`.
    MissingProviderApiKey { slug: String },
    /// Unrecognized `--provider` / `CANON_CLERK_PROVIDER` / `defaults.provider` slug.
    UnsupportedProviderSelector { provider: String },
    /// Model specifier uses an unrecognized provider prefix not matching built-in or configured `providers.<slug>` profiles.
    UnsupportedProvider { provider: String, specifier: String },
    /// Model specifier is missing `<provider>:<model>` syntax, contains whitespace, or names an unrecognized model.
    InvalidModelSpecifier {
        specifier: String,
        reason: String,
        example: String,
    },
    /// Provider `baseUrl` override (CLI flag, environment variable, or `config.json`) is malformed.
    InvalidBaseUrl {
        slug: String,
        source: String,
        raw: String,
        reason: String,
    },
    /// Required screening/vetting node has no model pinned and the active custom profile defines no default model.
    UnprovisionedNodeModel {
        node: HeuristicNode,
        provider: String,
    },
    /// `audit` was invoked on a discriminator-only provider without an explicit `audit` model pin.
    UnprovisionedAuditModel { provider: String },
    /// Discriminative decision model (`<slug>:gemini-decision-*` or `systemone` `<slug>:<model>`) was configured on `audit`.
    DiscriminativeAuditUnsupported { specifier: String },
    /// Per-node extended thinking was configured in `config.json` on a `Discriminator`-bound node.
    DiscriminativeNodeThinkingUnsupported {
        node: HeuristicNode,
        specifier: String,
    },
    /// Thinking level cannot be parsed as `minimal`, `low`, `medium`, or `high`.
    InvalidThinkingLevel { raw: String },
    /// Reasoning budget environment variable cannot be parsed as a non-negative integer.
    InvalidReasoningBudget { raw: String },
    /// Mutually exclusive `thinking_level` and `reasoning_budget` were specified at the same layer.
    ConflictingThinkingOptions { source: String },
    /// User configuration file contains a retired flat root key or structured model shim.
    RetiredConfigKey {
        path: String,
        key: String,
        hint: String,
    },
    /// Environment contains a retired tier environment variable (`CANON_CLERK_SCREENER_MODEL` or `CANON_CLERK_AUDITOR_MODEL`).
    RetiredEnvVar { var: String, hint: String },
    /// User configuration file has group or world access permissions on a POSIX host.
    InsecureConfigPermissions { path: String, mode_octal: String },
    /// User configuration file resides inside the repository workspace directory tree.
    ConfigInsideWorkspace { path: String },
    /// User configuration file could not be read or parsed against `{ providers, defaults, nodes }`.
    MalformedConfigFile { path: String, message: String },
}

impl ConfigureError {
    /// Constructs a table-driven [`Self::MissingProviderApiKey`] error for `slug`.
    pub fn missing_api_key_for_slug(slug: &str) -> Self {
        Self::MissingProviderApiKey {
            slug: slug.to_string(),
        }
    }
}

impl fmt::Display for ConfigureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingProviderApiKey { slug } => {
                f.write_str(&MissingCredentialSpec::for_slug(slug).format_diagnostic(None))
            }
            Self::UnsupportedProviderSelector { provider } => write!(
                f,
                "Unsupported provider `{provider}`.\n  \
                 Hint: Pass `--provider google` or `--provider typesafe` (or set `{ENV_PROVIDER}` / `defaults.provider`), \
                 or declare `providers.{provider}` (`{{ \"protocol\": \"systemone\" | \"gemini\", \"baseUrl\": \"...\" }}`) in `~/.config/canon-clerk/config.json`."
            ),
            Self::UnsupportedProvider {
                provider,
                specifier,
            } => write!(
                f,
                "Unsupported model provider `{provider}` in specifier `{specifier}`.\n  \
                 Hint: Built-in providers are `google` (e.g. `{DEFAULT_GOOGLE_DISCRIMINATOR_MODEL}`, `{DEFAULT_GOOGLE_GENERATOR_MODEL}`) \
                 and `typesafe` (e.g. `{DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL}`), or declare a custom `providers.{provider}` profile in `~/.config/canon-clerk/config.json`."
            ),
            Self::InvalidModelSpecifier {
                specifier,
                reason,
                example,
            } => write!(
                f,
                "Invalid model specifier `{specifier}`: {reason}\n  \
                 Hint: Specify a canonical `<provider>:<model>` identifier (for example, `{example}`)."
            ),
            Self::InvalidBaseUrl {
                slug,
                source,
                raw,
                reason,
            } => write!(
                f,
                "Invalid base URL `{raw}` for provider `{slug}` ({source}): {reason}\n  \
                 Hint: Provide a valid `http://` or `https://` endpoint URL (for example, `http://127.0.0.1:8080/v1` or `https://generativelanguage.googleapis.com/v1beta`)."
            ),
            Self::UnprovisionedNodeModel { node, provider } => write!(
                f,
                "Provider `{provider}` does not define a default model for `{node}`.\n  \
                 Hint: Pass `{} <{provider}:model>` (or set `{}` / `nodes.{node}.model`, or configure `providers.{provider}.defaultDiscriminator` in `~/.config/canon-clerk/config.json`).",
                node.cli_model_flag(),
                node.env_model_var()
            ),
            Self::UnprovisionedAuditModel { provider } => write!(
                f,
                "Provider `{provider}` does not define a default generative model for `audit` \
                 (`audit` requires structured generative output for exception rulings, remediation guidance, and line annotations).\n  \
                 Hint: Pass `--audit-model {DEFAULT_GOOGLE_GENERATOR_MODEL}` (or set `{ENV_AUDIT_MODEL}` / `nodes.audit.model` \
                 in `~/.config/canon-clerk/config.json`) alongside `{ENV_GEMINI_API_KEY}`, or run `canon-clerk vet --provider {provider}` \
                 for discriminative compliance vetting."
            ),
            Self::DiscriminativeAuditUnsupported { specifier } => write!(
                f,
                "Discriminative model `{specifier}` cannot be configured on `audit` \
                 (`audit` requires structured generative output for exception rulings, remediation guidance, and line annotations).\n  \
                 Hint: Configure `{specifier}` on screening/vetting nodes (`--docket-model`, `--admit-model`, `--vet-model`, or `--provider <slug>`) \
                 and keep a generative model such as `{DEFAULT_GOOGLE_GENERATOR_MODEL}` on `--audit-model`."
            ),
            Self::DiscriminativeNodeThinkingUnsupported { node, specifier } => write!(
                f,
                "Extended thinking options on `nodes.{node}` are not supported for discriminative model `{specifier}`.\n  \
                 Hint: Remove `nodes.{node}.thinkingLevel` / `nodes.{node}.reasoningBudget` from `config.json` (or move shared thinking defaults under `defaults` / `nodes.audit`)."
            ),
            Self::InvalidThinkingLevel { raw }
            | Self::InvalidReasoningBudget { raw }
            | Self::ConflictingThinkingOptions { source: raw } => self.fmt_thinking_error(f, raw),
            Self::RetiredConfigKey { .. }
            | Self::RetiredEnvVar { .. }
            | Self::InsecureConfigPermissions { .. }
            | Self::ConfigInsideWorkspace { .. }
            | Self::MalformedConfigFile { .. } => self.fmt_store_or_env_error(f),
        }
    }
}

impl ConfigureError {
    fn fmt_thinking_error(&self, f: &mut fmt::Formatter<'_>, raw: &str) -> fmt::Result {
        match self {
            Self::InvalidThinkingLevel { .. } => write!(
                f,
                "Invalid thinking level `{raw}`: expected one of `minimal`, `low`, `medium`, or `high`.\n  \
                 Hint: Pass `--thinking-level <minimal|low|medium|high>` or set `{ENV_THINKING_LEVEL}=<LEVEL>`."
            ),
            Self::InvalidReasoningBudget { .. } => write!(
                f,
                "Invalid reasoning budget `{raw}`: expected a non-negative integer token count.\n  \
                 Hint: Pass an integer token ceiling via `--reasoning-budget <TOKENS>` \
                 or `{ENV_REASONING_BUDGET}=<TOKENS>`."
            ),
            Self::ConflictingThinkingOptions { .. } => write!(
                f,
                "Conflicting extended-thinking options in {raw}: `--thinking-level` and `--reasoning-budget` are mutually exclusive.\n  \
                 Hint: Specify either `--thinking-level <minimal|low|medium|high>` or `--reasoning-budget <TOKENS>`, not both."
            ),
            _ => Ok(()),
        }
    }

    fn fmt_store_or_env_error(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RetiredConfigKey { path, key, hint } => write!(
                f,
                "Retired configuration key `{key}` in `{path}`.\n  Hint: {hint}"
            ),
            Self::RetiredEnvVar { var, hint } => write!(
                f,
                "Retired environment variable `{var}` is no longer supported.\n  Hint: {hint}"
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
                 `~/.config/canon-clerk/config.json` (mode `0600`) or via `{ENV_GEMINI_API_KEY}` / `{ENV_TYPESAFE_API_KEY}`."
            ),
            Self::MalformedConfigFile { path, message } => write!(
                f,
                "Failed to parse configuration file `{path}`: {message}\n  \
                 Hint: Ensure `{path}` contains valid JSON restricted to `providers`, `defaults`, and `nodes`."
            ),
            _ => Ok(()),
        }
    }
}

impl std::error::Error for ConfigureError {}

pub(crate) fn check_retired_env_vars(env: &BTreeMap<String, String>) -> Result<(), ConfigureError> {
    if env
        .get("CANON_CLERK_SCREENER_MODEL")
        .is_some_and(|v| !v.trim().is_empty())
    {
        return Err(ConfigureError::RetiredEnvVar {
            var: "CANON_CLERK_SCREENER_MODEL".to_string(),
            hint: format!(
                "Unset `CANON_CLERK_SCREENER_MODEL` and use `{ENV_PROVIDER}` (e.g. `typesafe`) or per-node \
                 `CANON_CLERK_APPRISE_MODEL`, `CANON_CLERK_DOCKET_MODEL`, `CANON_CLERK_ADMIT_MODEL`, / `CANON_CLERK_VET_MODEL`."
            ),
        });
    }
    if env
        .get("CANON_CLERK_AUDITOR_MODEL")
        .is_some_and(|v| !v.trim().is_empty())
    {
        return Err(ConfigureError::RetiredEnvVar {
            var: "CANON_CLERK_AUDITOR_MODEL".to_string(),
            hint: format!(
                "Unset `CANON_CLERK_AUDITOR_MODEL` and use `{ENV_AUDIT_MODEL}=<provider:model>` (for example, `{DEFAULT_GOOGLE_GENERATOR_MODEL}`)."
            ),
        });
    }
    Ok(())
}
