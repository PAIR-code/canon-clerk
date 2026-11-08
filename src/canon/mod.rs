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

//! Two-stage Canon Markdown and YAML frontmatter parser and AST (`SPEC.md` §2–§6).

mod body;
mod derive;
mod frontmatter;
mod lexer;
mod sourcing;

pub use body::{CanonBody, DirectiveOccurrence, parse_body, parse_body_from_tokens};
pub use derive::{
    CanonMetadata, DEFAULT_FALLBACK_ID, DEFAULT_GLOBAL_TRIGGER_PATTERN, DEFAULT_INSPECT_PLANES,
    InspectPlane, InspectToken, derive_exists, derive_id, derive_inspect, derive_metadata,
    derive_references, derive_tags, derive_title, derive_triggers, extract_first_heading,
    extract_first_heading_from_tokens, id_to_title_case, to_kebab_case,
};
pub use frontmatter::{
    ExtractedFrontmatter, FrontmatterKeySpan, RawFrontmatter, extract_frontmatter,
    extract_frontmatter_from_tokens,
};
pub use lexer::{
    CanonParseError, CanonToken, CanonTokenKind, DirectiveName, SourceSpan,
    normalize_canon_content, tokenize_canon,
};
pub use sourcing::{
    CanonScope, CanonSourceInfo, CanonSourceKind, derive_diagnostic_path, derive_scope,
    has_directory_traversal, resolve_source_info, scope_pattern,
};

use serde::{Deserialize, Serialize};

/// Normalized, flat in-memory domain entity representing a parsed Canon rule (`SPEC.md` §3–§5).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Canon {
    /// Canonical machine identifier slug (`SPEC.md` §4.1, §4.2.1).
    pub id: String,
    /// Human-readable title displayed in check runs and reports (`SPEC.md` §4.1, §4.2.2).
    pub title: String,
    /// Path globs defining file modifications that activate this canon (`SPEC.md` §4.1, §4.2.3).
    pub triggers: Vec<String>,
    /// Path globs that must exist in the Target File Tree for this canon to run (`SPEC.md` §4.1, §4.2.6).
    pub exists: Vec<String>,
    /// Context exhibit planes supplied during rule evaluation (`SPEC.md` §4.1, §4.2.4).
    pub inspect: Vec<InspectPlane>,
    /// Categorical labels normalized to lowercase kebab-case (`SPEC.md` §4.1, §4.2.5).
    pub tags: Vec<String>,
    /// Path globs of repository files injected as grounding context (`SPEC.md` §4.1, §4.2.7).
    pub references: Vec<String>,
    /// Primary normative invariant statement — What (`SPEC.md` §4.2.8, §5).
    pub invariant: String,
    /// 1-based source coordinates of the invariant statement in the document, if present in body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invariant_span: Option<SourceSpan>,
    /// Discrete permissible deviation clauses evaluated as logical ORs — When (`SPEC.md` §5).
    pub exceptions: Vec<String>,
    /// Precedent engineering rationale — Why (`SPEC.md` §5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    /// Actionable contributor remediation instructions — How (`SPEC.md` §5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    /// Source-qualified reference (`<workspace-path>`, `builtin:<path>`, or `<pack>:<path>`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    /// Sourcing realm classification (`Workspace`, `Builtin`, or `Pack`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<CanonSourceKind>,
    /// Workspace-relative POSIX file path when sourced from the workspace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// Path relative to the nearest enclosing `.canons/` directory or pack root (`SPEC.md` §4.2.1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_path: Option<String>,
    /// Monorepo package scope prefix (e.g. `"packages/ui"`), if scoped (`SPEC.md` §3.2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Full raw UTF-8 content of the canon file as provided to the parser.
    pub raw_content: String,
    /// Markdown body text with the opening YAML frontmatter removed.
    pub raw_body: String,
    /// Raw unvalidated YAML frontmatter mapping, if a frontmatter block was present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_frontmatter: Option<RawFrontmatter>,
    /// 1-based source coordinates of the opening YAML frontmatter block, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontmatter_span: Option<SourceSpan>,
    /// 1-based source coordinates and raw values for each top-level frontmatter key occurrence.
    pub frontmatter_key_spans: Vec<FrontmatterKeySpan>,
    /// 1-based source coordinates and raw labels for each body directive occurrence.
    pub directive_spans: Vec<DirectiveOccurrence>,
}

/// Configuration options for [`parse_canon`] and [`assemble_canon_from_tokens`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParseCanonOptions {
    /// Optional source-qualified reference or workspace file path (e.g. `".canons/auth/rule.md"`, `"builtin:core/rule"`).
    pub source_ref: Option<String>,
    /// Optional explicit monorepo scope override (e.g. `"packages/ui"`).
    pub scope: Option<String>,
}

impl ParseCanonOptions {
    /// Creates options with a source-qualified reference or workspace-relative file path.
    pub fn with_source_ref(source_ref: impl Into<String>) -> Self {
        Self {
            source_ref: Some(source_ref.into()),
            scope: None,
        }
    }

    /// Attaches an explicit monorepo scope prefix override.
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }
}

/// Stage 2 Semantic AST Builder: Constructs a normalized [`Canon`] entity from Stage 1 [`CanonToken`]s.
pub fn assemble_canon_from_tokens(
    tokens: &[CanonToken],
    raw_content: &str,
    options: &ParseCanonOptions,
) -> Result<Canon, CanonParseError> {
    let source_info = options.source_ref.as_deref().and_then(resolve_source_info);
    let file_path_ctx = source_info
        .as_ref()
        .map(|s| s.source_ref.as_str())
        .or(options.source_ref.as_deref());

    let extracted =
        extract_frontmatter_from_tokens(tokens).map_err(|e| e.with_file_path(file_path_ctx))?;

    let explicit_scope = options
        .scope
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| CanonScope(s.replace('\\', "/").trim_matches('/').to_string()));

    let effective_scope =
        explicit_scope.or_else(|| source_info.as_ref().and_then(|s| s.scope.clone()));

    let metadata = derive_metadata(
        &extracted.frontmatter,
        tokens,
        source_info.as_ref(),
        effective_scope.as_ref(),
    )?;

    let body = parse_body_from_tokens(tokens);

    // SPEC.md §4.2.8: Invariant falls back to derived title for empty or heading-only canons
    let invariant = if body.invariant.trim().is_empty() {
        metadata.title.clone()
    } else {
        body.invariant
    };

    Ok(Canon {
        id: metadata.id,
        title: metadata.title,
        triggers: metadata.triggers,
        exists: metadata.exists,
        inspect: metadata.inspect,
        tags: metadata.tags,
        references: metadata.references,
        invariant,
        invariant_span: body.invariant_span,
        exceptions: body.exceptions,
        rationale: body.rationale,
        remediation: body.remediation,
        source_ref: source_info.as_ref().map(|s| s.source_ref.clone()),
        source_kind: source_info.as_ref().map(|s| s.source_kind.clone()),
        file_path: source_info.as_ref().and_then(|s| s.file_path.clone()),
        diagnostic_path: source_info
            .as_ref()
            .map(|s| s.diagnostic_path.clone())
            .filter(|p| !p.is_empty()),
        scope: effective_scope.map(|s| s.0),
        raw_content: raw_content.to_string(),
        raw_body: body.raw_body,
        raw_frontmatter: extracted.raw_frontmatter,
        frontmatter_span: extracted.frontmatter_span,
        frontmatter_key_spans: extracted.key_spans,
        directive_spans: body.directive_spans,
    })
}

/// Pure two-stage parser that converts raw Canon Markdown and YAML frontmatter into a normalized [`Canon`] entity.
///
/// Strictly pure with zero filesystem or network I/O side effects.
pub fn parse_canon(
    raw_content: &str,
    options: &ParseCanonOptions,
) -> Result<Canon, CanonParseError> {
    let tokens =
        tokenize_canon(raw_content).map_err(|e| e.with_file_path(options.source_ref.as_deref()))?;
    assemble_canon_from_tokens(&tokens, raw_content, options)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod stage1_lexer {
        use super::*;

        #[test]
        fn test_tokenizes_frontmatter_and_headings_with_spans() {
            let raw = "\
---
id: test-canon
triggers:
  - \"**/*.rs\"
---
# Test Canon Title

Primary invariant statement.";

            let tokens = tokenize_canon(raw).expect("tokenize valid canon");

            assert_eq!(
                tokens[0].span,
                SourceSpan {
                    start_line: 1,
                    start_column: 1,
                    end_line: 5,
                    end_column: 3,
                }
            );
            assert!(matches!(
                &tokens[0].kind,
                CanonTokenKind::Frontmatter {
                    yaml,
                    yaml_start_line: 2
                } if yaml.contains("id: test-canon")
            ));

            let heading = tokens
                .iter()
                .find(|t| matches!(t.kind, CanonTokenKind::Heading { .. }))
                .expect("heading token present");
            assert_eq!(heading.span.start_line, 6);
            assert_eq!(
                heading.kind,
                CanonTokenKind::Heading {
                    level: 1,
                    text: "Test Canon Title".to_string(),
                }
            );
        }

        #[test]
        fn test_tokenizes_directives_and_code_blocks_with_spans() {
            let raw = "\
Primary invariant statement.

Exception: Legacy modules MAY omit this.

Rationale: Invariant preserves type safety.

**Remediation:** Follow these steps:
```bash
cargo check
```";

            let tokens = tokenize_canon(raw).expect("tokenize directives and code");
            let directives: Vec<&CanonToken> = tokens
                .iter()
                .filter(|t| matches!(t.kind, CanonTokenKind::Directive { .. }))
                .collect();
            assert_eq!(directives.len(), 3);
            assert_eq!(directives[0].span.start_line, 3);
            assert_eq!(
                directives[0].kind,
                CanonTokenKind::Directive {
                    name: DirectiveName::Exception,
                    label: "Exception:".to_string(),
                    value: "Legacy modules MAY omit this.".to_string(),
                }
            );
            assert_eq!(directives[1].span.start_line, 5);
            assert_eq!(
                directives[1].kind,
                CanonTokenKind::Directive {
                    name: DirectiveName::Rationale,
                    label: "Rationale:".to_string(),
                    value: "Invariant preserves type safety.".to_string(),
                }
            );
            assert_eq!(directives[2].span.start_line, 7);
            assert_eq!(
                directives[2].kind,
                CanonTokenKind::Directive {
                    name: DirectiveName::Remediation,
                    label: "**Remediation:**".to_string(),
                    value: "Follow these steps:".to_string(),
                }
            );

            let code_block = tokens
                .iter()
                .find(|t| matches!(t.kind, CanonTokenKind::CodeBlock { .. }))
                .expect("code block present");
            assert_eq!(
                code_block.span,
                SourceSpan {
                    start_line: 8,
                    start_column: 1,
                    end_line: 10,
                    end_column: 3,
                }
            );
            assert_eq!(
                code_block.kind,
                CanonTokenKind::CodeBlock {
                    lang: Some("bash".to_string()),
                    content: "cargo check".to_string(),
                    closed: true,
                }
            );
        }

        #[test]
        fn test_ignores_directives_and_headings_inside_fenced_code_blocks() {
            let raw = "\
# Fenced Example

```markdown
Exception: Inside code fence
Rationale: Also inside code fence
# Heading inside fence
```

Rationale: Real rationale outside code.";

            let tokens = tokenize_canon(raw).expect("tokenize");
            let directives: Vec<&CanonToken> = tokens
                .iter()
                .filter(|t| matches!(t.kind, CanonTokenKind::Directive { .. }))
                .collect();
            assert_eq!(directives.len(), 1);
            assert_eq!(directives[0].span.start_line, 9);
        }

        #[test]
        fn test_recognizes_singular_and_plural_section_header_directives() {
            let raw = "\
## Exceptions
- First exception

### Rationale
Why it exists.

### Remediation
How to fix.";

            let tokens = tokenize_canon(raw).expect("tokenize");
            let directives: Vec<&CanonToken> = tokens
                .iter()
                .filter(|t| matches!(t.kind, CanonTokenKind::Directive { .. }))
                .collect();
            assert_eq!(directives.len(), 3);
            assert!(matches!(
                directives[0].kind,
                CanonTokenKind::Directive {
                    name: DirectiveName::Exception,
                    ..
                }
            ));
            assert!(matches!(
                directives[1].kind,
                CanonTokenKind::Directive {
                    name: DirectiveName::Rationale,
                    ..
                }
            ));
            assert!(matches!(
                directives[2].kind,
                CanonTokenKind::Directive {
                    name: DirectiveName::Remediation,
                    ..
                }
            ));
        }

        #[test]
        fn test_unclosed_frontmatter_returns_error_with_line_1_span() {
            let raw = "---\nid: unclosed\ntitle: Missing delimiter";
            let err = tokenize_canon(raw).expect_err("unclosed frontmatter must fail");
            assert!(err.message.contains("Unclosed frontmatter block"));
            assert_eq!(err.span.unwrap().start_line, 1);
        }

        #[test]
        fn test_empty_and_bom_crlf_inputs() {
            assert!(tokenize_canon("").expect("empty").is_empty());
            let tokens =
                tokenize_canon("\u{feff}---\r\nid: bom\r\n---\r\nBody line\r\n").expect("bom crlf");
            assert!(matches!(tokens[0].kind, CanonTokenKind::Frontmatter { .. }));
        }
    }

    mod frontmatter_extraction {
        use super::*;

        #[test]
        fn test_extracts_frontmatter_and_tracks_1_based_key_and_item_spans() {
            let raw = "\
---
id: custom-id
triggers:
  - \"src/**/*.rs\"
  - \"tests/**/*.rs\"
---
# Heading

Body text.";

            let extracted = extract_frontmatter(raw).expect("extract frontmatter");
            assert!(extracted.raw_frontmatter.is_some());
            assert_eq!(extracted.body, "# Heading\n\nBody text.");
            assert_eq!(extracted.key_spans.len(), 2);

            let id_span = &extracted.key_spans[0];
            assert_eq!(id_span.key, "id");
            assert_eq!(id_span.key_span.start_line, 2);
            assert_eq!(id_span.value_span.start_line, 2);
            assert_eq!(id_span.value_span.start_column, 5);

            let triggers_span = &extracted.key_spans[1];
            assert_eq!(triggers_span.key, "triggers");
            assert_eq!(triggers_span.key_span.start_line, 3);
            assert_eq!(triggers_span.item_spans.len(), 2);
            assert_eq!(triggers_span.item_spans[0].start_line, 4);
            assert_eq!(triggers_span.item_spans[0].start_column, 5);
            assert_eq!(triggers_span.item_spans[1].start_line, 5);
        }

        #[test]
        fn test_handles_omitted_and_empty_frontmatter_blocks() {
            let no_fm = extract_frontmatter("# Only Heading\n\nRule.").expect("no fm");
            assert!(no_fm.raw_frontmatter.is_none());
            assert!(no_fm.frontmatter.is_empty());
            assert_eq!(no_fm.body, "# Only Heading\n\nRule.");

            let empty_fm = extract_frontmatter("---\n---\n# Only Heading").expect("empty fm");
            assert!(empty_fm.raw_frontmatter.is_none());
            assert!(empty_fm.frontmatter.is_empty());
            assert_eq!(empty_fm.body, "# Only Heading");
        }

        #[test]
        fn test_rejects_invalid_yaml_and_non_mapping_frontmatter_with_spans() {
            let bad_yaml = "---\nid: [unclosed\n---\n# Body";
            let err = extract_frontmatter(bad_yaml).expect_err("invalid yaml must fail");
            assert!(err.message.contains("Invalid YAML in frontmatter block"));
            assert!(err.span.unwrap().start_line >= 2);

            let seq_yaml = "---\n- item 1\n- item 2\n---\n# Body";
            let seq_err = extract_frontmatter(seq_yaml).expect_err("non-mapping must fail");
            assert!(
                seq_err
                    .message
                    .contains("must be a valid YAML mapping/object")
            );
        }
    }

    mod derivation_and_sourcing {
        use super::*;

        #[test]
        fn test_root_and_nested_workspace_sourcing() {
            let root = resolve_source_info(".canons/prs-must-include-tests.md").unwrap();
            assert_eq!(root.source_kind, CanonSourceKind::Workspace);
            assert_eq!(
                root.file_path.as_deref(),
                Some(".canons/prs-must-include-tests.md")
            );
            assert_eq!(root.diagnostic_path, "prs-must-include-tests");
            assert_eq!(root.file_stem, "prs-must-include-tests");
            assert_eq!(root.scope, None);
            assert_eq!(derive_id(None, Some(&root)), "prs-must-include-tests");

            let nested = resolve_source_info(".canons/auth/tokens-must-expire.md").unwrap();
            assert_eq!(nested.source_kind, CanonSourceKind::Workspace);
            assert_eq!(nested.diagnostic_path, "auth/tokens-must-expire");
            assert_eq!(nested.file_stem, "tokens-must-expire");
            assert_eq!(nested.scope, None);
            assert_eq!(derive_id(None, Some(&nested)), "tokens-must-expire");
        }

        #[test]
        fn test_scoped_workspace_sourcing() {
            let scoped =
                resolve_source_info("packages/ui/.canons/auth/tokens-must-expire.md").unwrap();
            assert_eq!(scoped.source_kind, CanonSourceKind::Workspace);
            assert_eq!(
                scoped.scope.as_ref().map(CanonScope::as_str),
                Some("packages/ui")
            );
            assert_eq!(scoped.diagnostic_path, "auth/tokens-must-expire");
            assert_eq!(derive_id(None, Some(&scoped)), "tokens-must-expire");
        }

        #[test]
        fn test_builtin_and_pack_realm_sourcing() {
            let builtin =
                resolve_source_info("builtin:canon-authoring/canons-must-be-atomic.md").unwrap();
            assert_eq!(builtin.source_kind, CanonSourceKind::Builtin);
            assert_eq!(builtin.file_path, None);
            assert_eq!(
                builtin.diagnostic_path,
                "canon-authoring/canons-must-be-atomic"
            );
            assert_eq!(derive_id(None, Some(&builtin)), "canons-must-be-atomic");

            let pack = resolve_source_info("security:auth/tokens-must-expire.md").unwrap();
            assert_eq!(
                pack.source_kind,
                CanonSourceKind::Pack("security".to_string())
            );
            assert_eq!(pack.file_path, None);
            assert_eq!(pack.diagnostic_path, "auth/tokens-must-expire");
            assert_eq!(derive_id(None, Some(&pack)), "tokens-must-expire");
        }

        #[test]
        fn test_explicit_id_slugification() {
            let explicit = serde_yaml::Value::String("Auth: Token Expiry (v2)".to_string());
            assert_eq!(derive_id(Some(&explicit), None), "auth-token-expiry-v2");
            assert_eq!(
                to_kebab_case("  Spaces_And/CAPS!  "),
                "spaces-and-caps".to_string()
            );
        }

        #[test]
        fn test_title_derivation_hierarchy() {
            let tokens_with_h1 = tokenize_canon("# Heading Title\n\nBody.").unwrap();
            let explicit = serde_yaml::Value::String("Explicit Title".to_string());
            assert_eq!(
                derive_title(Some(&explicit), &tokens_with_h1, "some-id"),
                "Explicit Title"
            );
            assert_eq!(
                derive_title(None, &tokens_with_h1, "some-id"),
                "Heading Title"
            );

            let tokens_no_heading = tokenize_canon("Plain body paragraph.").unwrap();
            assert_eq!(
                derive_title(
                    None,
                    &tokens_no_heading,
                    "canon-0001-prs-must-include-tests"
                ),
                "Prs Must Include Tests"
            );
        }

        #[test]
        fn test_scoped_pattern_prefixing_and_traversal_rejection() {
            let ui_scope = CanonScope("packages/ui".to_string());
            assert_eq!(
                derive_triggers(None, Some(&ui_scope)).unwrap(),
                vec!["packages/ui/**"]
            );
            assert_eq!(derive_triggers(None, None).unwrap(), vec!["**/*"]);

            let raw_triggers: serde_yaml::Value =
                serde_yaml::from_str("[\"**/*.tsx\", \"/src/Button.tsx\"]").unwrap();
            assert_eq!(
                derive_triggers(Some(&raw_triggers), Some(&ui_scope)).unwrap(),
                vec!["packages/ui/**/*.tsx", "packages/ui/src/Button.tsx"]
            );

            let traversal: serde_yaml::Value = serde_yaml::from_str("[\"../outside.ts\"]").unwrap();
            assert!(derive_triggers(Some(&traversal), Some(&ui_scope)).is_err());
            assert!(derive_exists(Some(&traversal), Some(&ui_scope)).is_err());
            assert!(derive_references(Some(&traversal), Some(&ui_scope)).is_err());
        }

        #[test]
        fn test_inspect_planes_required_and_optional_riders() {
            assert_eq!(derive_inspect(None), DEFAULT_INSPECT_PLANES);

            let custom: serde_yaml::Value =
                serde_yaml::from_str("[\"diff?\", \"pr_body\", \"linked_issues?\"]").unwrap();
            assert_eq!(
                derive_inspect(Some(&custom)),
                vec![
                    InspectPlane {
                        token: InspectToken::Diff,
                        optional: true,
                    },
                    InspectPlane {
                        token: InspectToken::PrBody,
                        optional: false,
                    },
                    InspectPlane {
                        token: InspectToken::LinkedIssues,
                        optional: true,
                    },
                ]
            );
        }
    }

    mod body_cognitive_tetrad {
        use super::*;

        #[test]
        fn test_parses_full_tetrad_and_plural_exceptions_section() {
            let raw = "\
## Rule
Standard platform functions MUST be used rather than introducing bespoke implementations.

## Exceptions
- A bespoke implementation MAY be introduced IFF accompanied by a benchmark.
- A bespoke implementation MAY be introduced IFF isolating a documented CVE.

## Rationale
Custom utility functions increase maintenance drag.

## Remediation
Replace custom utilities with platform equivalents.";

            let body = parse_body(raw).expect("parse tetrad");
            assert_eq!(
                body.invariant,
                "Standard platform functions MUST be used rather than introducing bespoke implementations."
            );
            assert_eq!(body.invariant_span.unwrap().start_line, 2);
            assert_eq!(
                body.exceptions,
                vec![
                    "A bespoke implementation MAY be introduced IFF accompanied by a benchmark.",
                    "A bespoke implementation MAY be introduced IFF isolating a documented CVE.",
                ]
            );
            assert_eq!(
                body.rationale.as_deref(),
                Some("Custom utility functions increase maintenance drag.")
            );
            assert_eq!(
                body.remediation.as_deref(),
                Some("Replace custom utilities with platform equivalents.")
            );
            assert_eq!(body.directive_spans.len(), 3);
        }
    }

    mod fault_tolerant_states {
        use super::*;

        #[test]
        fn test_missing_and_empty_canon_files_fallback_invariant_to_title() {
            // 0-byte canon file
            let zero_byte = parse_canon(
                "",
                &ParseCanonOptions::with_source_ref(
                    ".canons/all-caps-spec-must-refer-to-spec-md.md",
                ),
            )
            .expect("0-byte canon must be valid");
            assert_eq!(zero_byte.id, "all-caps-spec-must-refer-to-spec-md");
            assert_eq!(zero_byte.title, "All Caps Spec Must Refer To Spec Md");
            assert_eq!(zero_byte.invariant, "All Caps Spec Must Refer To Spec Md");
            assert!(zero_byte.exceptions.is_empty());
            assert!(zero_byte.rationale.is_none());
            assert!(zero_byte.remediation.is_none());

            // Heading-only canon file
            let heading_only =
                parse_canon("# PRs Must Include Tests\n", &ParseCanonOptions::default())
                    .expect("heading-only canon must be valid");
            assert_eq!(heading_only.title, "PRs Must Include Tests");
            assert_eq!(heading_only.invariant, "PRs Must Include Tests");
        }

        #[test]
        fn test_blank_and_whitespace_only_frontmatter_and_directives() {
            let raw = "\
---
id: \"   \"
title: \"   \"
triggers:
  - \"   \"
exists: \"   \"
inspect:
  - \"   \"
tags:
  - \"   \"
  - \"!!!\"
references: \"   \"
---
Invariant text.

Exception:    

Exception: Valid exception clause.

Rationale:   \t  

**Remediation:**    ";

            let canon = parse_canon(
                raw,
                &ParseCanonOptions::with_source_ref(".canons/auth/tokens-must-expire.md"),
            )
            .expect("blank fields must degrade gracefully");

            assert_eq!(canon.id, "tokens-must-expire");
            assert_eq!(canon.title, "Tokens Must Expire");
            assert_eq!(canon.triggers, vec!["**/*"]);
            assert!(canon.exists.is_empty());
            assert_eq!(canon.inspect, DEFAULT_INSPECT_PLANES);
            assert!(canon.tags.is_empty());
            assert!(canon.references.is_empty());
            assert_eq!(canon.invariant, "Invariant text.");
            assert_eq!(canon.exceptions, vec!["Valid exception clause."]);
            assert_eq!(canon.rationale, None);
            assert_eq!(canon.remediation, None);
            // All 4 directive occurrences (including blank ones) are preserved with spans for `validate`
            assert_eq!(canon.directive_spans.len(), 4);
        }

        #[test]
        fn test_repeated_frontmatter_keys_and_repeated_body_directives() {
            let raw = "\
---
id: first-id
id: second-id
tags:
  - alpha
tags:
  - beta
---
Invariant text.

Exception: First exception.

Exception: Second exception.

Rationale: First rationale paragraph.

Rationale: Second rationale paragraph.

**Remediation:** Step 1.

**Remediation:** Step 2.";

            let canon = parse_canon(raw, &ParseCanonOptions::default())
                .expect("repeated keys and directives must be handled fault-tolerantly");

            assert_eq!(canon.id, "second-id");
            assert_eq!(canon.tags, vec!["beta"]);
            // All 4 top-level frontmatter key occurrences preserved with 1-based line numbers
            assert_eq!(canon.frontmatter_key_spans.len(), 4);
            assert_eq!(canon.frontmatter_key_spans[0].key_span.start_line, 2);
            assert_eq!(canon.frontmatter_key_spans[1].key_span.start_line, 3);

            assert_eq!(
                canon.exceptions,
                vec!["First exception.", "Second exception."]
            );
            assert_eq!(
                canon.rationale.as_deref(),
                Some("First rationale paragraph.\n\nSecond rationale paragraph.")
            );
            assert_eq!(canon.remediation.as_deref(), Some("Step 1.\n\nStep 2."));
            assert_eq!(canon.directive_spans.len(), 6);
        }

        #[test]
        fn test_mistyped_frontmatter_fields_degrade_gracefully_while_preserving_raw_inputs() {
            let raw = "\
---
id: 12345
title: true
triggers:
  - 999
  - \"src/**/*.rs\"
exists: 42
inspect:
  - \"unknown_plane\"
  - \"diff?\"
  - 100
tags:
  - false
  - \"Core_Module\"
references:
  nested: object
---
Normative invariant MUST hold.";

            let canon = parse_canon(
                raw,
                &ParseCanonOptions::with_source_ref(".canons/fallback-stem.md"),
            )
            .expect("mistyped frontmatter fields must degrade gracefully");

            assert_eq!(canon.id, "fallback-stem");
            assert_eq!(canon.title, "Fallback Stem");
            assert_eq!(canon.triggers, vec!["src/**/*.rs"]);
            assert!(canon.exists.is_empty());
            assert_eq!(
                canon.inspect,
                vec![InspectPlane {
                    token: InspectToken::Diff,
                    optional: true,
                }]
            );
            assert_eq!(canon.tags, vec!["core-module"]);
            assert!(canon.references.is_empty());

            // Verify raw mistyped YAML values and spans remain intact for downstream `validate` linting
            let raw_fm = canon.raw_frontmatter.as_ref().expect("raw frontmatter");
            assert!(raw_fm["id"].is_number());
            assert!(raw_fm["title"].is_bool());
            assert_eq!(canon.frontmatter_key_spans.len(), 7);
        }
    }

    mod repository_corpus_smoke {
        use super::*;
        use std::fs;
        use std::path::Path;

        fn collect_canon_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        collect_canon_files(&path, out);
                    } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        out.push(path);
                    }
                }
            }
        }

        #[test]
        fn test_all_repository_canons_parse_cleanly() {
            let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
            let canons_dir = manifest_dir.join(".canons");
            let mut files = Vec::new();
            collect_canon_files(&canons_dir, &mut files);
            assert!(!files.is_empty(), "repository must contain dogfood canons");

            for path in files {
                let rel = path
                    .strip_prefix(manifest_dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let raw = fs::read_to_string(&path).expect("read canon file");
                let canon = parse_canon(&raw, &ParseCanonOptions::with_source_ref(&rel))
                    .unwrap_or_else(|e| panic!("failed to parse {rel}: {e}"));
                assert!(!canon.id.is_empty());
                assert!(!canon.title.is_empty());
                assert!(!canon.invariant.is_empty());
                assert!(canon.diagnostic_path.is_some());
            }
        }
    }
}
