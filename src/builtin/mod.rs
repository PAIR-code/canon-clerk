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

//! Compile-time embedded `builtin:<domain>/<stem>` standard canon registry (`SPEC.md` §3.1).

/// A single curated standard canon embedded into the binary at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinCanon {
    /// Canonical source-qualified reference (`builtin:<domain>/<stem>`, without `.canon.md`).
    pub source_ref: &'static str,
    /// Domain pack directory name (e.g. `"cli"`, `"canon-authoring"`).
    pub domain: &'static str,
    /// Canon file stem without `.canon.md` extension.
    pub stem: &'static str,
    /// Raw UTF-8 Markdown source embedded via `include_str!`.
    pub raw_markdown: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/builtins_generated.rs"));

/// Returns all embedded `builtin:*` canons in deterministic lexicographical order by `source_ref`.
pub fn builtin_canons() -> &'static [BuiltinCanon] {
    BUILTIN_CANONS
}

/// Looks up an embedded built-in canon by its canonical `builtin:<domain>/<stem>` reference.
pub fn find_builtin_canon(source_ref: &str) -> Option<&'static BuiltinCanon> {
    BUILTIN_CANONS
        .binary_search_by_key(&source_ref, |entry| entry.source_ref)
        .ok()
        .map(|idx| &BUILTIN_CANONS[idx])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::{CanonSourceKind, ParseCanonOptions, parse_canon};
    use crate::caseload::{Caseload, CaseloadDiscovery, DiscoveryMode};
    use crate::discover::WorkspaceSnapshot;
    use crate::validate::{ValidateOptions, execute_validate};
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_builtin_registry_is_non_empty_sorted_and_lookupable() {
        let builtins = builtin_canons();
        assert!(!builtins.is_empty(), "embedded builtin corpus must exist");

        for pair in builtins.windows(2) {
            assert!(
                pair[0].source_ref < pair[1].source_ref,
                "builtin registry must be strictly sorted: {} >= {}",
                pair[0].source_ref,
                pair[1].source_ref
            );
        }

        for entry in builtins {
            assert_eq!(
                entry.source_ref,
                format!("builtin:{}/{}", entry.domain, entry.stem)
            );
            assert!(!entry.source_ref.ends_with(".canon.md"));
            assert!(!entry.source_ref.ends_with(".md"));
            assert!(!entry.raw_markdown.trim().is_empty());
            assert_eq!(find_builtin_canon(entry.source_ref), Some(entry));
        }

        assert_eq!(find_builtin_canon("builtin:nonexistent/rule"), None);
    }

    #[test]
    fn test_builtin_registry_matches_builtins_directory_on_disk() {
        let builtins_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("builtins");
        let mut disk_refs = BTreeSet::new();

        for domain_entry in fs::read_dir(&builtins_root).expect("read builtins/") {
            let domain_entry = domain_entry.expect("domain dir entry");
            let domain_path = domain_entry.path();
            if !domain_path.is_dir() {
                continue;
            }
            let domain = domain_entry.file_name().to_string_lossy().into_owned();
            for file_entry in fs::read_dir(&domain_path).expect("read domain dir") {
                let file_entry = file_entry.expect("file entry");
                let file_path = file_entry.path();
                if !file_path.is_file() {
                    continue;
                }
                let name = file_entry.file_name().to_string_lossy().into_owned();
                if let Some(stem) = name.strip_suffix(".canon.md") {
                    disk_refs.insert(format!("builtin:{domain}/{stem}"));
                }
            }
        }

        let embedded_refs: BTreeSet<String> = builtin_canons()
            .iter()
            .map(|entry| entry.source_ref.to_string())
            .collect();
        assert_eq!(embedded_refs, disk_refs);
    }

    #[test]
    fn test_all_embedded_builtins_parse_and_validate_with_zero_errors_and_zero_warnings() {
        let mut snapshot = WorkspaceSnapshot::new().with_builtins();
        let mut candidate_canons = Vec::new();

        for entry in builtin_canons() {
            let opts = ParseCanonOptions::with_source_ref(entry.source_ref);
            let parsed = parse_canon(entry.raw_markdown, &opts)
                .unwrap_or_else(|err| panic!("failed to parse {}: {err}", entry.source_ref));
            assert_eq!(parsed.source_kind, Some(CanonSourceKind::Builtin));
            assert_eq!(parsed.source_ref.as_deref(), Some(entry.source_ref));
            assert_eq!(parsed.file_path, None);
            candidate_canons.push(entry.source_ref.to_string());
        }

        assert!(
            snapshot.existing_files.is_empty(),
            "with_builtins() must not populate existing_files"
        );
        // Ensure every builtin is present in snapshot.canons
        assert_eq!(snapshot.canons.len(), builtin_canons().len());

        let caseload = Caseload::new().with_discovery(CaseloadDiscovery {
            mode: DiscoveryMode::AllCanons,
            candidate_canons,
            ..CaseloadDiscovery::default()
        });

        let validated = execute_validate(
            &ValidateOptions {
                max_warnings: Some(0),
            },
            caseload,
            &snapshot,
        )
        .expect("validate embedded builtins");

        let validation = validated.validation.expect("validation populated");
        let mut failures = Vec::new();
        for (canon_ref, meta) in &validation.results {
            if meta.error_count > 0 || meta.warning_count > 0 {
                failures.push(format!(
                    "{canon_ref}: {} errors, {} warnings: {:?}",
                    meta.error_count, meta.warning_count, meta.diagnostics
                ));
            }
        }
        assert!(
            failures.is_empty() && !validation.has_errors,
            "embedded builtin canons must pass validate with 0 errors and 0 warnings:\n{}",
            failures.join("\n")
        );
        // Keep snapshot mutable reference used cleanly
        snapshot.existing_files.clear();
    }
}
