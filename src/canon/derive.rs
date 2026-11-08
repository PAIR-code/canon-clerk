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

//! Deterministic metadata derivation rules for Canon frontmatter (`SPEC.md` §4).

use super::frontmatter::RawFrontmatter;
use super::lexer::{CanonParseError, CanonToken, CanonTokenKind, tokenize_canon};
use super::sourcing::{CanonScope, CanonSourceInfo, scope_pattern};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Default path trigger glob for global canons when `triggers:` is omitted (`SPEC.md` §4.1).
pub const DEFAULT_GLOBAL_TRIGGER_PATTERN: &str = "**/*";

/// Fallback machine identifier when both frontmatter `id` and source path are absent.
pub const DEFAULT_FALLBACK_ID: &str = "unnamed-canon";

/// Supported context exhibit plane tokens (`SPEC.md` §4.1 & §4.2.4).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum InspectToken {
    Diff,
    PrTitle,
    PrBody,
    CommitMessages,
    LinkedIssues,
}

impl InspectToken {
    /// Parses a bare inspect token name (without a trailing `?` rider).
    pub fn parse_token(raw: &str) -> Option<Self> {
        match raw.trim() {
            "diff" => Some(Self::Diff),
            "pr_title" => Some(Self::PrTitle),
            "pr_body" => Some(Self::PrBody),
            "commit_messages" => Some(Self::CommitMessages),
            "linked_issues" => Some(Self::LinkedIssues),
            _ => None,
        }
    }

    /// Returns the canonical snake_case token name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Diff => "diff",
            Self::PrTitle => "pr_title",
            Self::PrBody => "pr_body",
            Self::CommitMessages => "commit_messages",
            Self::LinkedIssues => "linked_issues",
        }
    }
}

/// Normalized exhibit plane descriptor with optionality rider support (`SPEC.md` §4.2.4).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectPlane {
    /// Target context exhibit token.
    pub token: InspectToken,
    /// `true` when declared with the `?` optionality rider (e.g. `"pr_title?"`).
    pub optional: bool,
}

/// Default `inspect` exhibit planes (`["diff", "pr_title?", "pr_body?"]`) per `SPEC.md` §4.1.
pub const DEFAULT_INSPECT_PLANES: &[InspectPlane] = &[
    InspectPlane {
        token: InspectToken::Diff,
        optional: false,
    },
    InspectPlane {
        token: InspectToken::PrTitle,
        optional: true,
    },
    InspectPlane {
        token: InspectToken::PrBody,
        optional: true,
    },
];

/// Normalized Canon metadata attributes (`SPEC.md` §4.1).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CanonMetadata {
    /// Canonical machine identifier slug.
    pub id: String,
    /// Human-readable canon title.
    pub title: String,
    /// Path globs defining file modifications that activate this canon.
    pub triggers: Vec<String>,
    /// Path globs that must exist in the Target File Tree for this canon to run.
    pub exists: Vec<String>,
    /// Context exhibit planes supplied during evaluation.
    pub inspect: Vec<InspectPlane>,
    /// Normalized kebab-case categorical labels.
    pub tags: Vec<String>,
    /// Path globs of repository files injected as grounding context.
    pub references: Vec<String>,
}

/// Normalizes a string into canonical lowercase kebab-case (`SPEC.md` §4.2.1, §4.2.5):
/// - Lowercases ASCII characters
/// - Replaces whitespace, slashes (`/`), and underscores (`_`) with hyphens (`-`)
/// - Strips characters outside `[a-z0-9-]`
/// - Collapses consecutive hyphens and trims leading/trailing hyphens
pub fn to_kebab_case(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_hyphen = true;

    for ch in input.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            last_was_hyphen = false;
        } else if (ch.is_whitespace() || ch == '/' || ch == '_' || ch == '-') && !last_was_hyphen {
            out.push('-');
            last_was_hyphen = true;
        }
    }

    if out.ends_with('-') {
        out.pop();
    }
    out
}

fn strip_numeric_id_prefix(id: &str) -> &str {
    let candidate = id.strip_prefix("canon-").unwrap_or(id);
    let digit_count = candidate.bytes().take_while(u8::is_ascii_digit).count();
    if digit_count > 0 {
        let after_digits = &candidate[digit_count..];
        if let Some(rest) = after_digits
            .strip_prefix('-')
            .or_else(|| after_digits.strip_prefix('_'))
        {
            return rest;
        }
    }
    id
}

/// Converts a kebab-case identifier into Title Case, stripping leading numeric prefixes.
pub fn id_to_title_case(id: &str) -> String {
    let cleaned = strip_numeric_id_prefix(id);
    let words: Vec<String> = cleaned
        .split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let mut capitalized = String::with_capacity(word.len());
                    capitalized.push(first.to_ascii_uppercase());
                    for ch in chars {
                        capitalized.push(ch.to_ascii_lowercase());
                    }
                    capitalized
                }
                None => String::new(),
            }
        })
        .collect();

    if words.is_empty() {
        "Untitled Canon".to_string()
    } else {
        words.join(" ")
    }
}

/// Extracts the first `#` or `##` heading from a Stage 1 [`CanonToken`] slice.
pub fn extract_first_heading_from_tokens(tokens: &[CanonToken]) -> Option<String> {
    tokens.iter().find_map(|token| {
        if let CanonTokenKind::Heading { level, text } = &token.kind
            && *level <= 2
            && !text.eq_ignore_ascii_case("rule")
            && !text.trim().is_empty()
        {
            Some(text.trim().to_string())
        } else {
            None
        }
    })
}

/// Extracts the first `#` or `##` heading from a Markdown body string.
pub fn extract_first_heading(markdown_body: &str) -> Option<String> {
    let tokens = tokenize_canon(markdown_body).ok()?;
    extract_first_heading_from_tokens(&tokens)
}

/// Derives the canonical machine `id` (`SPEC.md` §4.2.1):
/// 1. Explicit frontmatter `id:` normalized to kebab-case
/// 2. File stem of the canon normalized to kebab-case (excluding directory paths)
/// 3. Fallback `"unnamed-canon"`
pub fn derive_id(
    frontmatter_id: Option<&serde_yaml::Value>,
    source_info: Option<&CanonSourceInfo>,
) -> String {
    if let Some(serde_yaml::Value::String(raw_id)) = frontmatter_id {
        let slug = to_kebab_case(raw_id);
        if !slug.is_empty() {
            return slug;
        }
    }

    if let Some(info) = source_info {
        let slug = to_kebab_case(&info.file_stem);
        if !slug.is_empty() {
            return slug;
        }
    }

    DEFAULT_FALLBACK_ID.to_string()
}

/// Derives the human-readable canon `title` (`SPEC.md` §4.2.2):
/// 1. Verbatim frontmatter `title:` (if non-blank string)
/// 2. First `#` or `##` heading in the Markdown body
/// 3. Derived `id` converted to Title Case
pub fn derive_title(
    frontmatter_title: Option<&serde_yaml::Value>,
    tokens: &[CanonToken],
    derived_id: &str,
) -> String {
    if let Some(serde_yaml::Value::String(raw_title)) = frontmatter_title {
        let trimmed = raw_title.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    if let Some(heading) = extract_first_heading_from_tokens(tokens) {
        return heading;
    }

    id_to_title_case(derived_id)
}

fn extract_string_list(value: Option<&serde_yaml::Value>) -> Vec<String> {
    match value {
        Some(serde_yaml::Value::String(s)) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![trimmed.to_string()]
            }
        }
        Some(serde_yaml::Value::Sequence(seq)) => seq
            .iter()
            .filter_map(serde_yaml::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

/// Derives `triggers` path globs (`SPEC.md` §4.2.3):
/// 1. Frontmatter `triggers:` (list of strings, scalar string, or `{ paths: [...] }`), scoped to `<scope>/` when scoped
/// 2. Scoped default `["<scope>/**"]`
/// 3. Global default `["**/*"]`
pub fn derive_triggers(
    frontmatter_triggers: Option<&serde_yaml::Value>,
    scope: Option<&CanonScope>,
) -> Result<Vec<String>, CanonParseError> {
    let raw_list = if let Some(serde_yaml::Value::Mapping(map)) = frontmatter_triggers {
        let paths_val = map.get(serde_yaml::Value::String("paths".to_string()));
        extract_string_list(paths_val)
    } else {
        extract_string_list(frontmatter_triggers)
    };

    if !raw_list.is_empty() {
        if let Some(sc) = scope {
            return raw_list
                .iter()
                .map(|pattern| scope_pattern(pattern, sc))
                .collect();
        }
        return Ok(raw_list);
    }

    if let Some(sc) = scope {
        Ok(vec![format!("{}/**", sc.0)])
    } else {
        Ok(vec![DEFAULT_GLOBAL_TRIGGER_PATTERN.to_string()])
    }
}

/// Derives `exists` state precondition globs (`SPEC.md` §4.2.6):
/// 1. Frontmatter `exists:` (coercing scalar string to 1-element list), scoped to `<scope>/` when scoped
/// 2. Default `[]`
pub fn derive_exists(
    frontmatter_exists: Option<&serde_yaml::Value>,
    scope: Option<&CanonScope>,
) -> Result<Vec<String>, CanonParseError> {
    let raw_list = extract_string_list(frontmatter_exists);
    if raw_list.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(sc) = scope {
        raw_list
            .iter()
            .map(|pattern| scope_pattern(pattern, sc))
            .collect()
    } else {
        Ok(raw_list)
    }
}

/// Derives `inspect` exhibit planes with `?` optionality riders (`SPEC.md` §4.2.4):
/// 1. Frontmatter `inspect:` (filtering to recognized `InspectToken`s with optional `?` suffix)
/// 2. Default `["diff", "pr_title?", "pr_body?"]`
pub fn derive_inspect(frontmatter_inspect: Option<&serde_yaml::Value>) -> Vec<InspectPlane> {
    let raw_list = extract_string_list(frontmatter_inspect);
    if raw_list.is_empty() {
        return DEFAULT_INSPECT_PLANES.to_vec();
    }

    let mut planes = Vec::new();
    for raw in raw_list {
        let (token_str, optional) = if let Some(stripped) = raw.strip_suffix('?') {
            (stripped.trim(), true)
        } else {
            (raw.as_str(), false)
        };
        if let Some(token) = InspectToken::parse_token(token_str) {
            planes.push(InspectPlane { token, optional });
        }
    }

    if planes.is_empty() {
        DEFAULT_INSPECT_PLANES.to_vec()
    } else {
        planes
    }
}

/// Derives normalized, deduplicated kebab-case `tags` (`SPEC.md` §4.2.5):
/// 1. Frontmatter `tags:` (coercing scalar string to 1-element list, slugifying, and deduplicating in order)
/// 2. Default `[]`
pub fn derive_tags(frontmatter_tags: Option<&serde_yaml::Value>) -> Vec<String> {
    let raw_list = extract_string_list(frontmatter_tags);
    let mut seen = HashSet::new();
    let mut tags = Vec::new();

    for raw in raw_list {
        let slug = to_kebab_case(&raw);
        if !slug.is_empty() && seen.insert(slug.clone()) {
            tags.push(slug);
        }
    }

    tags
}

/// Derives `references` grounding context path globs (`SPEC.md` §4.2.7):
/// 1. Frontmatter `references:` (coercing scalar string to 1-element list), scoped to `<scope>/` when scoped
/// 2. Default `[]`
pub fn derive_references(
    frontmatter_references: Option<&serde_yaml::Value>,
    scope: Option<&CanonScope>,
) -> Result<Vec<String>, CanonParseError> {
    let raw_list = extract_string_list(frontmatter_references);
    if raw_list.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(sc) = scope {
        raw_list
            .iter()
            .map(|pattern| scope_pattern(pattern, sc))
            .collect()
    } else {
        Ok(raw_list)
    }
}

/// Derives all normalized [`CanonMetadata`] fields from raw YAML frontmatter and Stage 1 tokens.
pub fn derive_metadata(
    frontmatter: &RawFrontmatter,
    tokens: &[CanonToken],
    source_info: Option<&CanonSourceInfo>,
    explicit_scope: Option<&CanonScope>,
) -> Result<CanonMetadata, CanonParseError> {
    let effective_scope = explicit_scope.or_else(|| source_info.and_then(|s| s.scope.as_ref()));
    let file_path = source_info.map(|s| s.source_ref.as_str());

    let id = derive_id(frontmatter.get("id"), source_info);
    let title = derive_title(frontmatter.get("title"), tokens, &id);
    let triggers = derive_triggers(frontmatter.get("triggers"), effective_scope)
        .map_err(|e| e.with_file_path(file_path))?;
    let exists = derive_exists(frontmatter.get("exists"), effective_scope)
        .map_err(|e| e.with_file_path(file_path))?;
    let inspect = derive_inspect(frontmatter.get("inspect"));
    let tags = derive_tags(frontmatter.get("tags"));
    let references = derive_references(frontmatter.get("references"), effective_scope)
        .map_err(|e| e.with_file_path(file_path))?;

    Ok(CanonMetadata {
        id,
        title,
        triggers,
        exists,
        inspect,
        tags,
        references,
    })
}
