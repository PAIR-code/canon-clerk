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

//! YAML frontmatter extraction and source-coordinate mapping for Canon files (`SPEC.md` §2.2).

use super::lexer::{CanonParseError, CanonToken, CanonTokenKind, SourceSpan, tokenize_canon};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Deterministic map of raw top-level YAML frontmatter properties.
pub type RawFrontmatter = BTreeMap<String, serde_yaml::Value>;

/// Source coordinates and raw value for a single top-level YAML frontmatter key occurrence.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct FrontmatterKeySpan {
    /// Top-level YAML property name (e.g. `"id"`, `"triggers"`, `"inspect"`).
    pub key: String,
    /// 1-based source coordinates of the key token.
    pub key_span: SourceSpan,
    /// 1-based source coordinates of the value expression.
    pub value_span: SourceSpan,
    /// 1-based source coordinates for each sequence element when the value is a YAML list.
    pub item_spans: Vec<SourceSpan>,
    /// Unvalidated YAML value associated with this key occurrence.
    pub raw_value: serde_yaml::Value,
}

/// Result of extracting YAML frontmatter and Markdown body from a Canon document.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ExtractedFrontmatter {
    /// Raw unvalidated YAML mapping if a non-empty frontmatter block was present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_frontmatter: Option<RawFrontmatter>,
    /// Frontmatter mapping (defaults to empty map `{}` when frontmatter is omitted or empty).
    pub frontmatter: RawFrontmatter,
    /// Ordered list of top-level key occurrences (including repeated keys) with 1-based spans.
    pub key_spans: Vec<FrontmatterKeySpan>,
    /// 1-based span of the enclosing `---` frontmatter block if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontmatter_span: Option<SourceSpan>,
    /// Remaining Markdown body text after frontmatter removal.
    pub body: String,
}

struct RawTopLevelEntry<'a> {
    key: String,
    start_idx: usize,
    end_idx: usize,
    lines: &'a [&'a str],
}

fn is_top_level_key_line(line: &str) -> Option<&str> {
    if line.is_empty() || line.starts_with(char::is_whitespace) || line.starts_with('#') {
        return None;
    }
    if line.starts_with("- ") || line == "-" {
        return None;
    }
    let (key_part, rest) = line.split_once(':')?;
    let key = key_part.trim();
    if key.is_empty() {
        return None;
    }
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(key)
}

fn split_top_level_entries<'a>(lines: &'a [&'a str]) -> Vec<RawTopLevelEntry<'a>> {
    let mut entries: Vec<RawTopLevelEntry<'a>> = Vec::new();
    for (idx, &line) in lines.iter().enumerate() {
        if let Some(key) = is_top_level_key_line(line) {
            if let Some(prev) = entries.last_mut() {
                prev.end_idx = idx.saturating_sub(1);
                prev.lines = &lines[prev.start_idx..=prev.end_idx];
            }
            entries.push(RawTopLevelEntry {
                key: key.to_string(),
                start_idx: idx,
                end_idx: lines.len().saturating_sub(1),
                lines: &[],
            });
        }
    }
    if let Some(last) = entries.last_mut() {
        last.end_idx = lines.len().saturating_sub(1);
        last.lines = &lines[last.start_idx..=last.end_idx];
    }
    entries
}

fn compute_entry_spans(
    entry: &RawTopLevelEntry<'_>,
    yaml_start_line: usize,
    raw_value: serde_yaml::Value,
) -> FrontmatterKeySpan {
    let open_line = entry.lines.first().copied().unwrap_or("");
    let key_line_num = yaml_start_line + entry.start_idx;
    let key_end_col = entry.key.chars().count().max(1);
    let key_span = SourceSpan {
        start_line: key_line_num,
        start_column: 1,
        end_line: key_line_num,
        end_column: key_end_col,
    };

    let after_colon = open_line
        .split_once(':')
        .map_or("", |(_, rest)| rest.trim());
    let mut item_spans = Vec::new();

    for (offset, &line) in entry.lines.iter().enumerate().skip(1) {
        let trimmed_start = line.trim_start();
        if let Some(item_rest) = trimmed_start
            .strip_prefix("- ")
            .or_else(|| (trimmed_start == "-").then_some(""))
        {
            let line_num = key_line_num + offset;
            let dash_col = line.chars().count() - trimmed_start.chars().count() + 1;
            let val_col = if item_rest.is_empty() {
                dash_col
            } else {
                line.chars().count() - item_rest.trim_start().chars().count() + 1
            };
            let end_col = line.chars().count().max(val_col);
            item_spans.push(SourceSpan {
                start_line: line_num,
                start_column: val_col,
                end_line: line_num,
                end_column: end_col,
            });
        }
    }

    let value_span = if !after_colon.is_empty() && !after_colon.starts_with('#') {
        let val_start_col = open_line.chars().count() - after_colon.chars().count() + 1;
        let val_end_col = open_line.chars().count().max(val_start_col);
        SourceSpan {
            start_line: key_line_num,
            start_column: val_start_col,
            end_line: key_line_num,
            end_column: val_end_col,
        }
    } else if let (Some(first_item), Some(last_item)) = (item_spans.first(), item_spans.last()) {
        SourceSpan {
            start_line: first_item.start_line,
            start_column: first_item.start_column,
            end_line: last_item.end_line,
            end_column: last_item.end_column,
        }
    } else {
        key_span
    };

    FrontmatterKeySpan {
        key: entry.key.clone(),
        key_span,
        value_span,
        item_spans,
        raw_value,
    }
}

fn yaml_error_to_parse_error(
    err: &serde_yaml::Error,
    yaml_start_line: usize,
    fallback_span: SourceSpan,
) -> CanonParseError {
    let span = err.location().map_or(fallback_span, |loc| {
        let line = yaml_start_line + loc.line().saturating_sub(1);
        let col = loc.column().max(1);
        SourceSpan {
            start_line: line,
            start_column: col,
            end_line: line,
            end_column: col,
        }
    });
    CanonParseError::new(
        format!("Invalid YAML in frontmatter block: {err}"),
        Some(span),
    )
}

fn parse_duplicate_tolerant_entries(
    entries: &[RawTopLevelEntry<'_>],
    yaml_start_line: usize,
    fallback_span: SourceSpan,
) -> Result<(RawFrontmatter, Vec<FrontmatterKeySpan>), CanonParseError> {
    if entries.is_empty() {
        return Err(CanonParseError::new(
            "Frontmatter content must be a valid YAML mapping/object",
            Some(fallback_span),
        ));
    }

    let mut map = BTreeMap::new();
    let mut key_spans = Vec::with_capacity(entries.len());

    for entry in entries {
        let snippet = entry.lines.join("\n");
        let entry_start_line = yaml_start_line + entry.start_idx;
        let parsed: serde_yaml::Mapping = serde_yaml::from_str(&snippet)
            .map_err(|e| yaml_error_to_parse_error(&e, entry_start_line, fallback_span))?;
        let raw_val = parsed
            .into_iter()
            .next()
            .map_or(serde_yaml::Value::Null, |(_, v)| v);
        map.insert(entry.key.clone(), raw_val.clone());
        key_spans.push(compute_entry_spans(entry, yaml_start_line, raw_val));
    }

    Ok((map, key_spans))
}

fn parse_yaml_mapping(
    yaml: &str,
    yaml_start_line: usize,
    block_span: SourceSpan,
) -> Result<Option<(RawFrontmatter, Vec<FrontmatterKeySpan>)>, CanonParseError> {
    if yaml.trim().is_empty() {
        return Ok(None);
    }

    let lines: Vec<&str> = yaml.split('\n').collect();
    let entries = split_top_level_entries(&lines);

    match serde_yaml::from_str::<serde_yaml::Value>(yaml) {
        Ok(serde_yaml::Value::Null) => Ok(None),
        Ok(serde_yaml::Value::Mapping(mapping)) => {
            let mut btree = BTreeMap::new();
            for (k, v) in mapping {
                if let Some(key_str) = k.as_str() {
                    btree.insert(key_str.to_string(), v);
                } else if let Ok(serialized_key) = serde_json::to_string(&k) {
                    btree.insert(serialized_key, v);
                }
            }
            if btree.is_empty() {
                return Ok(None);
            }
            let key_spans = if entries.is_empty() {
                btree
                    .iter()
                    .map(|(k, v)| FrontmatterKeySpan {
                        key: k.clone(),
                        key_span: block_span,
                        value_span: block_span,
                        item_spans: Vec::new(),
                        raw_value: v.clone(),
                    })
                    .collect()
            } else {
                entries
                    .iter()
                    .map(|entry| {
                        let raw_val = btree
                            .get(&entry.key)
                            .cloned()
                            .unwrap_or(serde_yaml::Value::Null);
                        compute_entry_spans(entry, yaml_start_line, raw_val)
                    })
                    .collect()
            };
            Ok(Some((btree, key_spans)))
        }
        Ok(_) => Err(CanonParseError::new(
            "Frontmatter content must be a valid YAML mapping/object",
            Some(block_span),
        )),
        Err(err) => {
            if err.to_string().contains("duplicate entry with key") && !entries.is_empty() {
                let (btree, key_spans) =
                    parse_duplicate_tolerant_entries(&entries, yaml_start_line, block_span)?;
                Ok(Some((btree, key_spans)))
            } else {
                Err(yaml_error_to_parse_error(&err, yaml_start_line, block_span))
            }
        }
    }
}

/// Extracts and parses YAML frontmatter from an already-tokenized [`CanonToken`] slice.
pub fn extract_frontmatter_from_tokens(
    tokens: &[CanonToken],
) -> Result<ExtractedFrontmatter, CanonParseError> {
    let Some(first) = tokens.first() else {
        return Ok(ExtractedFrontmatter {
            raw_frontmatter: None,
            frontmatter: BTreeMap::new(),
            key_spans: Vec::new(),
            frontmatter_span: None,
            body: String::new(),
        });
    };

    if let CanonTokenKind::Frontmatter {
        yaml,
        yaml_start_line,
    } = &first.kind
    {
        let body = tokens
            .iter()
            .skip(1)
            .map(|t| t.raw.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let parsed = parse_yaml_mapping(yaml, *yaml_start_line, first.span)?;
        if let Some((map, key_spans)) = parsed {
            Ok(ExtractedFrontmatter {
                raw_frontmatter: Some(map.clone()),
                frontmatter: map,
                key_spans,
                frontmatter_span: Some(first.span),
                body,
            })
        } else {
            Ok(ExtractedFrontmatter {
                raw_frontmatter: None,
                frontmatter: BTreeMap::new(),
                key_spans: Vec::new(),
                frontmatter_span: Some(first.span),
                body,
            })
        }
    } else {
        let body = tokens
            .iter()
            .map(|t| t.raw.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ExtractedFrontmatter {
            raw_frontmatter: None,
            frontmatter: BTreeMap::new(),
            key_spans: Vec::new(),
            frontmatter_span: None,
            body,
        })
    }
}

/// Extracts and parses YAML frontmatter from raw Canon Markdown content (`SPEC.md` §2.2).
pub fn extract_frontmatter(raw_content: &str) -> Result<ExtractedFrontmatter, CanonParseError> {
    let tokens = tokenize_canon(raw_content)?;
    extract_frontmatter_from_tokens(&tokens)
}
