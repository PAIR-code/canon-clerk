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

//! Validation and construction of built-in and custom `providers.<slug>` profile entries (`configure.md` §3.5).

use crate::caseload::{DEFAULT_GOOGLE_DISCRIMINATOR_MODEL, DEFAULT_PROVIDER, TYPESAFE_PROVIDER};
use crate::configure::protocol::{
    ProtocolDriver, is_valid_gemini_model_name, is_valid_jev_model_name, validate_http_base_url,
};
use crate::configure::providers::{ModelCapability, ProviderCapability, ProviderProfile};
use crate::configure::{ConfigureError, StoredProviderConfig};

pub(super) fn merge_builtin_profile(
    builtin: &ProviderCapability,
    entry: &StoredProviderConfig,
    path: &str,
) -> Result<ProviderProfile, ConfigureError> {
    if let Some(raw_proto) = entry
        .protocol
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let parsed = raw_proto.parse::<ProtocolDriver>().map_err(|_| {
            ConfigureError::MalformedConfigFile {
                path: path.to_string(),
                message: format!(
                    "unknown `providers.{}.protocol` value `{raw_proto}` (expected `\"gemini\"` or `\"systemone\"`).",
                    builtin.slug
                ),
            }
        })?;
        if parsed != builtin.protocol {
            return Err(ConfigureError::MalformedConfigFile {
                path: path.to_string(),
                message: format!(
                    "built-in provider `{}` requires `protocol: \"{}\"` (got `\"{parsed}\"`); declare a custom profile slug under `providers.<slug>` instead.",
                    builtin.slug, builtin.protocol
                ),
            });
        }
    }

    let base_url = match entry.base_url.as_deref() {
        Some(raw_url) => validate_http_base_url(raw_url).map_err(|reason| {
            ConfigureError::MalformedConfigFile {
                path: path.to_string(),
                message: format!(
                    "invalid `providers.{}.baseUrl` `{raw_url}`: {reason}",
                    builtin.slug
                ),
            }
        })?,
        None => builtin.default_base_url.to_string(),
    };

    let mut profile = ProviderProfile::from_builtin(builtin);
    profile.base_url = base_url;
    apply_profile_default_models(&mut profile, entry, path)?;
    Ok(profile)
}

pub(super) fn build_custom_profile(
    slug: &str,
    entry: &StoredProviderConfig,
    path: &str,
) -> Result<ProviderProfile, ConfigureError> {
    let raw_proto = entry
        .protocol
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ConfigureError::MalformedConfigFile {
            path: path.to_string(),
            message: format!(
                "custom provider `providers.{slug}` is missing required `protocol` (`\"systemone\"` or `\"gemini\"`)."
            ),
        })?;
    let protocol = raw_proto.parse::<ProtocolDriver>().map_err(|_| {
        ConfigureError::MalformedConfigFile {
            path: path.to_string(),
            message: format!(
                "unknown `providers.{slug}.protocol` value `{raw_proto}` (expected `\"systemone\"` or `\"gemini\"`)."
            ),
        }
    })?;

    let raw_url = entry
        .base_url
        .as_deref()
        .ok_or_else(|| ConfigureError::MalformedConfigFile {
            path: path.to_string(),
            message: format!(
                "custom provider `providers.{slug}` is missing required `baseUrl` (for example, `\"http://127.0.0.1:8080/v1\"`)."
            ),
        })?;
    let base_url =
        validate_http_base_url(raw_url).map_err(|reason| ConfigureError::MalformedConfigFile {
            path: path.to_string(),
            message: format!("invalid `providers.{slug}.baseUrl` `{raw_url}`: {reason}"),
        })?;

    let mut profile = ProviderProfile {
        slug: slug.to_string(),
        protocol,
        auth_policy: protocol.default_custom_auth_policy(),
        base_url,
        default_discriminator: None,
        default_generator: None,
    };
    apply_profile_default_models(&mut profile, entry, path)?;
    Ok(profile)
}

fn apply_profile_default_models(
    profile: &mut ProviderProfile,
    entry: &StoredProviderConfig,
    path: &str,
) -> Result<(), ConfigureError> {
    if let Some(raw_disc) = entry.default_discriminator.as_deref() {
        let norm = validate_profile_default_specifier(
            profile,
            raw_disc,
            "defaultDiscriminator",
            ModelCapability::Discriminator,
            path,
        )?;
        profile.default_discriminator = Some(norm);
    }
    if let Some(raw_gen) = entry.default_generator.as_deref() {
        if !profile.protocol.supports_generator() {
            return Err(ConfigureError::MalformedConfigFile {
                path: path.to_string(),
                message: format!(
                    "`providers.{}.defaultGenerator` is not supported on `{}` protocol profiles (`{}` is discriminative-only).",
                    profile.slug, profile.protocol, profile.protocol
                ),
            });
        }
        let norm = validate_profile_default_specifier(
            profile,
            raw_gen,
            "defaultGenerator",
            ModelCapability::Generator,
            path,
        )?;
        profile.default_generator = Some(norm);
    }
    Ok(())
}

fn validate_profile_default_specifier(
    profile: &ProviderProfile,
    raw: &str,
    field_name: &str,
    expected_cap: ModelCapability,
    path: &str,
) -> Result<String, ConfigureError> {
    let (prov, model_id) = split_and_check_specifier_syntax(raw).map_err(|err| {
        ConfigureError::MalformedConfigFile {
            path: path.to_string(),
            message: format!(
                "invalid `providers.{}.{field_name}` `{raw}`: {err}",
                profile.slug
            ),
        }
    })?;
    if prov != profile.slug {
        return Err(ConfigureError::MalformedConfigFile {
            path: path.to_string(),
            message: format!(
                "`providers.{}.{field_name}` (`{raw}`) must use matching `{}:<model>` prefix (got `{prov}:`).",
                profile.slug, profile.slug
            ),
        });
    }
    profile
        .protocol
        .validate_model_id(&profile.slug, model_id)
        .map_err(|(reason, _)| ConfigureError::MalformedConfigFile {
            path: path.to_string(),
            message: format!(
                "invalid `providers.{}.{field_name}` `{raw}`: {reason}",
                profile.slug
            ),
        })?;
    let actual_cap = profile.protocol.classify_model_id(model_id);
    if actual_cap != expected_cap {
        return Err(ConfigureError::MalformedConfigFile {
            path: path.to_string(),
            message: format!(
                "`providers.{}.{field_name}` (`{raw}`) classifies as `{actual_cap:?}` rather than `{expected_cap:?}`.",
                profile.slug
            ),
        });
    }
    Ok(format!("{prov}:{model_id}"))
}

pub(crate) fn split_and_check_specifier_syntax(raw: &str) -> Result<(&str, &str), ConfigureError> {
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
    Ok((provider, model_part))
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
