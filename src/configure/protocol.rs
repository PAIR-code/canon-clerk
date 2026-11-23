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

//! Wire-protocol drivers (`gemini`, `systemone`), authentication policies, base URL validation,
//! and resolved [`ProviderConnection`] transport descriptors (`docs/architecture/nodes/configure.md` §3–4).

use crate::caseload::{
    DEFAULT_GOOGLE_GENERATOR_MODEL, DEFAULT_LOCAL_DISCRIMINATOR_MODEL, DEFAULT_PROVIDER,
    DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL, LOCAL_PROVIDER, TYPESAFE_PROVIDER,
};
use crate::configure::{
    ENV_GEMINI_API_KEY, ENV_LOCAL_API_KEY, ENV_TYPESAFE_API_KEY, ModelCapability,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Default Google Generative Language API v1beta base URL.
pub const DEFAULT_GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

/// Default TypeSafe AI v1 REST API base URL.
pub const DEFAULT_TYPESAFE_BASE_URL: &str = "https://api.typesafe.ai/v1";

/// Default built-in local System One daemon base URL (`llama-server` loopback endpoint).
pub const DEFAULT_LOCAL_BASE_URL: &str = "http://127.0.0.1:8080/v1";

/// Standardized wire-protocol driver spoken by a provider profile (`configure.md` §4.1).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ProtocolDriver {
    /// Google Generative Language REST protocol (`:generateContent` + `/decisions`, `x-goog-api-key`).
    Gemini,
    /// System One `Noul` REST protocol (`POST {baseUrl}/systemone`, optional or required `Bearer`).
    SystemOne,
}

impl ProtocolDriver {
    /// Returns the canonical lowercase protocol identifier (`"gemini"` or `"systemone"`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gemini => "gemini",
            Self::SystemOne => "systemone",
        }
    }

    /// Returns `true` when this wire protocol supports `Generator` structured generation turns.
    pub const fn supports_generator(self) -> bool {
        matches!(self, Self::Gemini)
    }

    /// Returns the default [`AuthPolicy`] applied to custom `providers.<slug>` profiles using this protocol.
    pub(crate) const fn default_custom_auth_policy(self) -> AuthPolicy {
        match self {
            Self::Gemini => AuthPolicy::RequiredApiKey,
            Self::SystemOne => AuthPolicy::OptionalApiKey,
        }
    }

    /// Classifies an unprefixed `<model>` identifier into its [`ModelCapability`] under this protocol.
    pub(crate) fn classify_model_id(self, model_id: &str) -> ModelCapability {
        match self {
            Self::Gemini if is_gemini_decision_model_name(model_id) => {
                ModelCapability::Discriminator
            }
            Self::Gemini => ModelCapability::Generator,
            Self::SystemOne => ModelCapability::Discriminator,
        }
    }

    /// Validates an unprefixed `<model>` segment for `slug` under this protocol, returning `(reason, example)` on error.
    pub(crate) fn validate_model_id(
        self,
        slug: &str,
        model_id: &str,
    ) -> Result<(), (String, String)> {
        match self {
            Self::Gemini => {
                if is_valid_gemini_model_name(model_id) {
                    Ok(())
                } else {
                    let example = if slug == DEFAULT_PROVIDER {
                        DEFAULT_GOOGLE_GENERATOR_MODEL.to_string()
                    } else {
                        format!("{slug}:gemini-4-carbon")
                    };
                    Err((
                        format!(
                            "unrecognized Gemini model `{model_id}` on `{slug}` (expected `gemini-decision-1`, `gemini-4-hydrogen`, `gemini-4-carbon`, or another exact `gemini-*` upstream ID)."
                        ),
                        example,
                    ))
                }
            }
            Self::SystemOne if slug == TYPESAFE_PROVIDER => {
                if is_valid_jev_model_name(model_id) {
                    Ok(())
                } else {
                    Err((
                        format!(
                            "unrecognized TypeSafe model `{model_id}` (expected `jev-*`, e.g. `jev-latest`)."
                        ),
                        DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL.to_string(),
                    ))
                }
            }
            Self::SystemOne => {
                if is_valid_custom_systemone_model_name(model_id) {
                    Ok(())
                } else {
                    let example = if slug == LOCAL_PROVIDER {
                        DEFAULT_LOCAL_DISCRIMINATOR_MODEL.to_string()
                    } else {
                        format!("{slug}:kev-4b")
                    };
                    Err((
                        format!(
                            "invalid `systemone` model identifier `{model_id}` on `{slug}` (expected a non-empty `[A-Za-z0-9._/-]+` model ID such as `kev-4b` or `kev-0.8b`)."
                        ),
                        example,
                    ))
                }
            }
        }
    }
}

impl fmt::Display for ProtocolDriver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ProtocolDriver {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "gemini" => Ok(Self::Gemini),
            "systemone" => Ok(Self::SystemOne),
            other => Err(other.to_string()),
        }
    }
}

/// Declarative credential enforcement policy for a provider profile (`configure.md` §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthPolicy {
    /// Fails fast with exit code `2` when no non-empty API key resolves.
    RequiredApiKey,
    /// Sends `Authorization: Bearer <key>` when an API key resolves, or `ProviderAuth::None` when omitted.
    OptionalApiKey,
}

/// Resolved outbound authentication credential for a [`ProviderConnection`].
#[derive(Clone, PartialEq, Eq)]
pub enum ProviderAuth {
    /// Unauthenticated transport (`AuthPolicy::OptionalApiKey` with no configured API key).
    None,
    /// Google Generative Language API `x-goog-api-key` header.
    GoogApiKey(String),
    /// Standard `Authorization: Bearer <token>` header.
    Bearer(String),
}

impl ProviderAuth {
    /// Returns `true` when an explicit non-empty API key or bearer token is present.
    pub fn has_secret(&self) -> bool {
        match self {
            Self::None => false,
            Self::GoogApiKey(k) | Self::Bearer(k) => !k.trim().is_empty(),
        }
    }

    /// Returns the underlying secret string slice when present (`None` for [`Self::None`]).
    pub fn secret(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::GoogApiKey(k) | Self::Bearer(k) => Some(k.as_str()),
        }
    }
}

impl fmt::Debug for ProviderAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => f.write_str("ProviderAuth::None"),
            Self::GoogApiKey(_) => f.write_str("ProviderAuth::GoogApiKey(\"<redacted>\")"),
            Self::Bearer(_) => f.write_str("ProviderAuth::Bearer(\"<redacted>\")"),
        }
    }
}

/// Resolved transport binding for a single provider profile `<slug>`, shared identically across
/// [`crate::provider::MultiModelProvider`] and [`crate::probe::MultiProviderEndpointProber`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConnection {
    /// Canonical provider profile slug (`"google"`, `"typesafe"`, `"local-dev"`, etc.).
    pub slug: String,
    /// Wire protocol driver spoken by this endpoint.
    pub protocol: ProtocolDriver,
    /// Validated `http://` or `https://` base URL.
    pub base_url: String,
    /// Resolved authentication credential (`GoogApiKey`, `Bearer`, or `None`).
    pub auth: ProviderAuth,
}

/// Table-driven diagnostic metadata for missing-credential errors across `configure`, `provider`, and `probe`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MissingCredentialSpec {
    /// Human-readable provider label (`"Google Gemini"`, `"TypeSafe AI"`, or `"provider `<slug>`"`).
    pub(crate) display_name: String,
    /// Primary environment variable name (`"GEMINI_API_KEY"`, `"TYPESAFE_API_KEY"`, `"<UPPER_SLUG>_API_KEY"`).
    pub(crate) primary_env: String,
    /// Optional higher-precedence `CANON_CLERK_<UPPER_SLUG>_API_KEY` alias for custom profiles.
    pub(crate) prefixed_env: Option<String>,
    /// Canonical `config.json` key path (`"providers.<slug>.apiKey"`).
    pub(crate) config_key_path: String,
    /// Optional built-in CLI flag (`"--gemini-api-key"`, `"--typesafe-api-key"`).
    pub(crate) cli_flag: Option<&'static str>,
}

impl MissingCredentialSpec {
    /// Constructs a [`MissingCredentialSpec`] for any built-in or custom provider slug.
    pub(crate) fn for_slug(slug: &str) -> Self {
        match slug {
            DEFAULT_PROVIDER => Self {
                display_name: "Google Gemini".to_string(),
                primary_env: ENV_GEMINI_API_KEY.to_string(),
                prefixed_env: None,
                config_key_path: "providers.google.apiKey".to_string(),
                cli_flag: Some("--gemini-api-key"),
            },
            TYPESAFE_PROVIDER => Self {
                display_name: "TypeSafe AI".to_string(),
                primary_env: ENV_TYPESAFE_API_KEY.to_string(),
                prefixed_env: None,
                config_key_path: "providers.typesafe.apiKey".to_string(),
                cli_flag: Some("--typesafe-api-key"),
            },
            LOCAL_PROVIDER => Self {
                display_name: "Local System One".to_string(),
                primary_env: ENV_LOCAL_API_KEY.to_string(),
                prefixed_env: Some("CANON_CLERK_LOCAL_API_KEY".to_string()),
                config_key_path: "providers.local.apiKey".to_string(),
                cli_flag: Some("--local-api-key"),
            },
            custom => {
                let stem = slug_to_upper_env_stem(custom);
                Self {
                    display_name: format!("provider `{custom}`"),
                    primary_env: format!("{stem}_API_KEY"),
                    prefixed_env: Some(format!("CANON_CLERK_{stem}_API_KEY")),
                    config_key_path: format!("providers.{custom}.apiKey"),
                    cli_flag: None,
                }
            }
        }
    }

    /// Formats a standardized missing-credential error message with an actionable `Hint:`.
    pub(crate) fn format_diagnostic(&self, context_suffix: Option<&str>) -> String {
        let suffix = context_suffix.map_or(String::new(), |ctx| format!(" for {ctx}"));
        let export_hint = match &self.prefixed_env {
            Some(prefixed) => {
                format!(
                    "`{prefixed}=\"<your-api-key>\"` (or `{}=\"<your-api-key>\"`)",
                    self.primary_env
                )
            }
            None => format!("`{}=\"<your-api-key>\"`", self.primary_env),
        };
        let flag_clause = match self.cli_flag {
            Some(flag) => format!(", or pass `{flag} <KEY>`"),
            None => String::new(),
        };
        format!(
            "Missing required {} API key (`{}`){suffix}.\n  \
             Hint: Export {export_hint}, configure `{}` in `~/.config/canon-clerk/config.json` (mode `0600`){flag_clause}.",
            self.display_name, self.primary_env, self.config_key_path
        )
    }
}

/// Formats an actionable HTTP `401`/`403` remediation hint tailored to the provider slug of `model_specifier`.
pub(crate) fn credential_verification_hint_for_slug(slug: &str) -> String {
    let spec = MissingCredentialSpec::for_slug(slug);
    match (&spec.prefixed_env, spec.cli_flag) {
        (_, Some(flag)) => format!(
            "Verify `{}`, `{}` in `~/.config/canon-clerk/config.json`, or `{flag}`.",
            spec.primary_env, spec.config_key_path
        ),
        (Some(prefixed), None) => format!(
            "Verify `{prefixed}`, `{}`, or `{}` in `~/.config/canon-clerk/config.json`.",
            spec.primary_env, spec.config_key_path
        ),
        (None, None) => format!(
            "Verify `{}` or `{}` in `~/.config/canon-clerk/config.json`.",
            spec.primary_env, spec.config_key_path
        ),
    }
}

/// Converts a kebab-case provider `<slug>` into its `UPPER_SNAKE_CASE` environment variable stem (`local-dev` -> `LOCAL_DEV`).
pub(super) fn slug_to_upper_env_stem(slug: &str) -> String {
    slug.trim()
        .chars()
        .map(|c| {
            if c == '-' {
                '_'
            } else {
                c.to_ascii_uppercase()
            }
        })
        .collect()
}

/// Returns `true` when `slug` is a valid lowercase kebab-case provider identifier (`[a-z0-9]+(-[a-z0-9]+)*`).
pub(super) fn is_valid_provider_slug(slug: &str) -> bool {
    if slug.is_empty() || slug.starts_with('-') || slug.ends_with('-') || slug.contains("--") {
        return false;
    }
    slug.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Validates and normalizes an `http://` or `https://` endpoint base URL (`configure.md` §4.1).
pub(super) fn validate_http_base_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("base URL cannot be empty.".to_string());
    }
    if trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("base URL must not contain whitespace or control characters.".to_string());
    }
    let after_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .ok_or_else(|| "expected `http://` or `https://` URL scheme.".to_string())?;

    if after_scheme.contains('?') || after_scheme.contains('#') {
        return Err("base URL must not contain `?` query or `#` fragment components.".to_string());
    }

    let authority = after_scheme.split('/').next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return Err(
            "base URL must include a valid host authority without embedded credentials."
                .to_string(),
        );
    }

    let host_port_ok = if let Some(bracket_rest) = authority.strip_prefix('[') {
        match bracket_rest.split_once(']') {
            Some((ipv6, "")) => !ipv6.is_empty(),
            Some((ipv6, port_part)) => !ipv6.is_empty() && is_valid_port_suffix(port_part),
            None => false,
        }
    } else {
        match authority.rsplit_once(':') {
            Some((host, port_str)) => !host.is_empty() && is_valid_port_str(port_str),
            None => !authority.is_empty(),
        }
    };

    if !host_port_ok {
        return Err(format!("invalid host or port in `{authority}`."));
    }

    Ok(trimmed.trim_end_matches('/').to_string())
}

fn is_valid_port_suffix(port_part: &str) -> bool {
    port_part.strip_prefix(':').is_some_and(is_valid_port_str)
}

fn is_valid_port_str(port_str: &str) -> bool {
    port_str.parse::<u16>().is_ok_and(|p| p > 0)
}

pub(crate) fn is_gemini_decision_model_name(name: &str) -> bool {
    is_valid_prefixed_model_name(name, "gemini-decision-")
}

pub(crate) fn is_valid_gemini_model_name(name: &str) -> bool {
    if name.starts_with("gemini-decision") {
        return is_gemini_decision_model_name(name);
    }
    is_valid_prefixed_model_name(name, "gemini-")
}

pub(crate) fn is_valid_jev_model_name(name: &str) -> bool {
    is_valid_prefixed_model_name(name, "jev-")
}

pub(crate) fn is_valid_custom_systemone_model_name(name: &str) -> bool {
    if name.is_empty() || name == "jev" {
        return false;
    }
    if name.starts_with("gemini-") && !is_gemini_decision_model_name(name) {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '/'))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slug_validation_and_upper_env_stem() {
        assert!(is_valid_provider_slug("google"));
        assert!(is_valid_provider_slug("local-dev"));
        assert!(is_valid_provider_slug("lan-gpu-2"));
        assert!(!is_valid_provider_slug(""));
        assert!(!is_valid_provider_slug("-bad"));
        assert!(!is_valid_provider_slug("bad-"));
        assert!(!is_valid_provider_slug("bad--slug"));
        assert!(!is_valid_provider_slug("BadUpper"));
        assert!(!is_valid_provider_slug("bad_underscore"));
        assert_eq!(slug_to_upper_env_stem("local-dev"), "LOCAL_DEV");
        assert_eq!(slug_to_upper_env_stem("google-corp"), "GOOGLE_CORP");
    }

    #[test]
    fn test_validate_http_base_url_accepts_http_https_and_rejects_malformed() {
        assert_eq!(
            validate_http_base_url("http://127.0.0.1:8080/v1/").expect("valid loopback"),
            "http://127.0.0.1:8080/v1"
        );
        assert_eq!(
            validate_http_base_url("http://[::1]:8080/").expect("valid ipv6"),
            "http://[::1]:8080"
        );
        assert_eq!(
            validate_http_base_url("https://gemini-gateway.corp.example.com/v1beta")
                .expect("valid https"),
            "https://gemini-gateway.corp.example.com/v1beta"
        );

        for invalid in [
            "",
            "   ",
            "ftp://127.0.0.1:8080",
            "127.0.0.1:8080/v1",
            "http://",
            "http:///v1",
            "http://user:pass@127.0.0.1:8080",
            "http://127.0.0.1:0/v1",
            "http://127.0.0.1:99999/v1",
            "http://127.0.0.1:8080/v1?key=secret",
            "http://127.0.0.1:8080/v1#frag",
            "http://127.0.0.1 :8080/v1",
        ] {
            assert!(
                validate_http_base_url(invalid).is_err(),
                "`{invalid}` must be rejected"
            );
        }
    }

    #[test]
    fn test_provider_auth_redacts_debug_output() {
        let secret = "top-secret-token-value";
        let bearer = ProviderAuth::Bearer(secret.to_string());
        let goog = ProviderAuth::GoogApiKey(secret.to_string());
        assert!(!format!("{bearer:?}").contains(secret));
        assert!(!format!("{goog:?}").contains(secret));
        assert_eq!(format!("{:?}", ProviderAuth::None), "ProviderAuth::None");
    }
}
