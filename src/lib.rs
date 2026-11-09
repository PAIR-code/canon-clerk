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

//! Canon Clerk: Automated review gate enforcing declarative engineering rule packs.

pub mod canon;
pub mod caseload;
pub mod cli;
pub mod discover;
pub mod intake;
pub mod validate;

pub use canon::{
    Canon, CanonBody, CanonMetadata, CanonParseError, CanonScope, CanonSourceInfo, CanonSourceKind,
    CanonToken, CanonTokenKind, DEFAULT_FALLBACK_ID, DEFAULT_GLOBAL_TRIGGER_PATTERN,
    DEFAULT_INSPECT_PLANES, DirectiveName, DirectiveOccurrence, ExtractedFrontmatter,
    FrontmatterKeySpan, InspectPlane, InspectToken, ParseCanonOptions, RawFrontmatter, SourceSpan,
    assemble_canon_from_tokens, derive_diagnostic_path, derive_exists, derive_id, derive_inspect,
    derive_metadata, derive_references, derive_scope, derive_tags, derive_title, derive_triggers,
    extract_first_heading, extract_first_heading_from_tokens, extract_frontmatter,
    extract_frontmatter_from_tokens, has_directory_traversal, id_to_title_case,
    normalize_canon_content, parse_body, parse_body_from_tokens, parse_canon, resolve_source_info,
    scope_pattern, to_kebab_case, tokenize_canon,
};
pub use caseload::{
    ActiveExhibits, AnnotationLevel, CASELOAD_SCHEMA_VERSION, Caseload, CaseloadDiscovery,
    CaseloadIntake, CaseloadValidation, CodeAnnotation, ContentOmissionReason, DiscoveryMode,
    FileArtifact, FileChangeStatus, IntakeScope, LinkedIssueContext, PatchOmissionReason,
    ValidatedCanonMetadata, ValidationOutcome,
};
pub use cli::{
    Cli, CliError, Commands, DiscoverArgs, DiscoverFormat, IntakeArgs, ValidateArgs,
    ValidateFormat, init_logging, run, run_discover, run_discover_in_workspace, run_intake,
    run_validate, run_validate_in_workspace,
};
pub use discover::{
    DEFAULT_CANON_GLOB, DEFAULT_IGNORES, DiscoverError, DiscoverOptions, GlobPattern,
    GlobSyntaxError, WorkspaceSnapshot, execute_discover, execute_discover_on_disk,
    is_ignored_path, is_wildcard_pattern, normalize_workspace_path, parse_canon_or_fallback,
    scope_relative_path,
};
pub use intake::{IntakeError, IntakeOptions, execute_intake, parse_unified_diff};
pub use validate::{
    ValidateError, ValidateOptions, execute_validate, execute_validate_on_disk, lint_canon_source,
};

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct BaselineManifest {
        name: String,
        version: String,
    }

    #[test]
    fn test_foundational_serde_serialization() {
        let manifest = BaselineManifest {
            name: "canon-clerk".to_string(),
            version: "0.1.0".to_string(),
        };

        let json = serde_json::to_string(&manifest).expect("serialization must succeed");
        let decoded: BaselineManifest =
            serde_json::from_str(&json).expect("deserialization must succeed");

        assert_eq!(manifest, decoded);
    }
}
