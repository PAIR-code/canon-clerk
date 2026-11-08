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

//! Pure two-stage unified diff parser (Stage 1 lexical tokenizer -> Stage 2 semantic AST builder).

mod ast;
mod lexer;

pub use ast::assemble_diff_artifacts;
pub use lexer::{
    DiffParseError, DiffToken, DiffTokenKind, normalize_diff_path, tokenize_diff, unquote_git_path,
};

use crate::caseload::FileArtifact;
use std::collections::BTreeMap;

/// Parses a unified diff string in-memory into a deterministically ordered map of [`FileArtifact`]s.
pub fn parse_unified_diff(input: &str) -> Result<BTreeMap<String, FileArtifact>, DiffParseError> {
    let tokens = tokenize_diff(input)?;
    assemble_diff_artifacts(&tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{ContentOmissionReason, FileChangeStatus, PatchOmissionReason};

    mod tokenizer {
        use super::*;

        #[test]
        fn test_tokenize_empty_and_whitespace_returns_empty_vec() {
            assert!(tokenize_diff("").expect("empty").is_empty());
            assert!(tokenize_diff("   \n\t\n  ").expect("whitespace").is_empty());
        }

        #[test]
        fn test_tokenize_git_headers_and_hunk_lines() {
            let patch = "\
diff --git a/src/lib.rs b/src/lib.rs
index 1234567..89abcdef 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,2 +1,3 @@
 pub mod cli;
+pub mod caseload;
 // end
";
            let tokens = tokenize_diff(patch).expect("tokenize valid patch");
            assert_eq!(
                tokens[0],
                DiffToken {
                    line_number: 1,
                    kind: DiffTokenKind::DiffGitHeader {
                        old_path: "src/lib.rs".to_string(),
                        new_path: "src/lib.rs".to_string(),
                    }
                }
            );
            assert!(matches!(
                tokens[4].kind,
                DiffTokenKind::HunkHeader {
                    old_start: 1,
                    old_count: 2,
                    new_start: 1,
                    new_count: 3,
                    ..
                }
            ));
            assert_eq!(
                tokens[6].kind,
                DiffTokenKind::HunkAdd("pub mod caseload;".to_string())
            );
        }

        #[test]
        fn test_unquote_git_path_decodes_escaped_and_octal_sequences() {
            assert_eq!(
                normalize_diff_path(r#""a/dir with spaces/file\tname.rs""#),
                "dir with spaces/file\tname.rs"
            );
            // \303\251 is UTF-8 for 'é'
            assert_eq!(normalize_diff_path(r#""b/caf\303\251.rs""#), "café.rs");
        }
    }

    mod semantic_assembly {
        use super::*;

        #[test]
        fn test_parse_multi_file_added_modified_deleted() {
            let patch = "\
diff --git a/src/added.rs b/src/added.rs
new file mode 100644
index 0000000..1111111
--- /dev/null
+++ b/src/added.rs
@@ -0,0 +1,2 @@
+pub fn hello() {}
+pub fn world() {}
diff --git a/src/modified.rs b/src/modified.rs
index 2222222..3333333 100644
--- a/src/modified.rs
+++ b/src/modified.rs
@@ -5,3 +5,3 @@
 fn keep() {}
-fn old() {}
+fn new() {}
diff --git a/src/deleted.rs b/src/deleted.rs
deleted file mode 100644
index 4444444..0000000
--- a/src/deleted.rs
+++ /dev/null
@@ -1,2 +0,0 @@
-line 1
-line 2
";
            let artifacts = parse_unified_diff(patch).expect("parse multi-file diff");
            assert_eq!(artifacts.len(), 3);

            let added = &artifacts["src/added.rs"];
            assert_eq!(added.status, FileChangeStatus::Added);
            assert_eq!(added.lines_added, 2);
            assert_eq!(added.lines_deleted, 0);
            assert!(
                added
                    .patch
                    .as_deref()
                    .unwrap()
                    .contains("+pub fn hello() {}")
            );
            assert_eq!(added.patch_omission_reason, None);
            assert_eq!(
                added.content_omission_reason,
                Some(ContentOmissionReason::NotRequested)
            );

            let modified = &artifacts["src/modified.rs"];
            assert_eq!(modified.status, FileChangeStatus::Modified);
            assert_eq!(modified.lines_added, 1);
            assert_eq!(modified.lines_deleted, 1);

            let deleted = &artifacts["src/deleted.rs"];
            assert_eq!(deleted.status, FileChangeStatus::Deleted);
            assert_eq!(deleted.lines_added, 0);
            assert_eq!(deleted.lines_deleted, 2);
            assert_eq!(
                deleted.content_omission_reason,
                Some(ContentOmissionReason::Deleted)
            );
        }

        #[test]
        fn test_parse_pure_rename_and_copied_files() {
            let patch = "\
diff --git a/src/old_name.rs b/src/new_name.rs
similarity index 100%
rename from src/old_name.rs
rename to src/new_name.rs
diff --git a/src/template.rs b/src/copied.rs
similarity index 90%
copy from src/template.rs
copy to src/copied.rs
--- a/src/template.rs
+++ b/src/copied.rs
@@ -1 +1,2 @@
 fn base() {}
+fn extra() {}
";
            let artifacts = parse_unified_diff(patch).expect("parse rename and copy");
            assert_eq!(artifacts.len(), 2);

            let renamed = &artifacts["src/new_name.rs"];
            assert_eq!(renamed.status, FileChangeStatus::Renamed);
            assert_eq!(renamed.previous_path.as_deref(), Some("src/old_name.rs"));
            assert_eq!(renamed.patch, None);
            assert_eq!(
                renamed.patch_omission_reason,
                Some(PatchOmissionReason::Unchanged)
            );

            let copied = &artifacts["src/copied.rs"];
            assert_eq!(copied.status, FileChangeStatus::Copied);
            assert_eq!(copied.previous_path.as_deref(), Some("src/template.rs"));
            assert_eq!(copied.lines_added, 1);
            assert!(copied.patch.is_some());
        }

        #[test]
        fn test_parse_binary_file_sets_binary_omission_reasons() {
            let patch = "\
diff --git a/assets/icon.png b/assets/icon.png
new file mode 100644
index 0000000..abcdef1
Binary files /dev/null and b/assets/icon.png differ
";
            let artifacts = parse_unified_diff(patch).expect("parse binary diff");
            let icon = &artifacts["assets/icon.png"];
            assert_eq!(icon.status, FileChangeStatus::Added);
            assert_eq!(icon.patch, None);
            assert_eq!(
                icon.patch_omission_reason,
                Some(PatchOmissionReason::Binary)
            );
            assert_eq!(
                icon.content_omission_reason,
                Some(ContentOmissionReason::Binary)
            );
        }

        #[test]
        fn test_repeated_file_diffs_merge_hunks_and_line_counts() {
            let patch = "\
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,1 +1,2 @@
 pub mod a;
+pub mod b;
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,1 +11,2 @@
 pub mod x;
+pub mod y;
";
            let artifacts = parse_unified_diff(patch).expect("parse repeated file diff");
            assert_eq!(artifacts.len(), 1);
            let lib = &artifacts["src/lib.rs"];
            assert_eq!(lib.lines_added, 2);
            assert!(lib.patch.as_deref().unwrap().contains("+pub mod b;"));
            assert!(lib.patch.as_deref().unwrap().contains("+pub mod y;"));
        }

        #[test]
        fn test_corrupted_diff_rejects_with_parse_error() {
            let err = parse_unified_diff("this is not a unified diff at all\njust random text")
                .expect_err("non-diff text must fail");
            assert_eq!(err.line_number, 1);
            assert!(err.to_string().contains("Invalid unified diff"));
        }
    }
}
