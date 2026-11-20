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

//! Pure in-memory unified diff overlay for resolving `references:` against a pre-patch
//! (`--workspace-state base`) workspace snapshot (`docs/architecture/nodes/vet.md`, `audit.md`).

use crate::canon::Canon;
use crate::caseload::{
    CaseloadIntake, FileArtifact, FileChangeStatus, PatchOmissionReason, WorkspaceState,
};
use crate::discover::{GlobPattern, WorkspaceSnapshot, normalize_workspace_path};
use std::collections::BTreeSet;
use std::fmt;

/// Deterministic fail-closed error raised when a `references:` file cannot be reconstructed
/// from a `--workspace-state base` checkout and `.intake.diffs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReferenceOverlayError {
    /// A modified, renamed, or copied reference file's base copy is missing from the workspace.
    MissingBaseFile {
        /// Target workspace-relative POSIX path in the filing.
        path: String,
        /// Base workspace-relative POSIX path expected on the base branch.
        base_path: String,
        /// Governing canon whose `references:` matched the file.
        canon_ref: String,
    },
    /// A modified or added reference file omitted its unified diff patch (e.g. binary or oversized).
    OmittedPatch {
        /// Target workspace-relative POSIX path in the filing.
        path: String,
        /// Governing canon whose `references:` matched the file.
        canon_ref: String,
        /// Recorded omission rationale from `intake`, if any.
        reason: Option<PatchOmissionReason>,
    },
    /// Unified diff patch failed to parse or apply cleanly against the base-branch file.
    PatchFailed {
        /// Target workspace-relative POSIX path in the filing.
        path: String,
        /// Governing canon whose `references:` matched the file.
        canon_ref: String,
        /// Underlying parser or hunk-matching diagnostic.
        detail: String,
    },
}

fn format_patch_omission_reason(reason: Option<PatchOmissionReason>) -> &'static str {
    match reason {
        Some(PatchOmissionReason::Binary) => "binary file",
        Some(PatchOmissionReason::Oversized) => "oversized diff",
        Some(PatchOmissionReason::NotRequested) => "patch not requested",
        Some(PatchOmissionReason::Unchanged) => "unchanged file",
        None => "unspecified reason",
    }
}

impl fmt::Display for ReferenceOverlayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBaseFile {
                path,
                base_path,
                canon_ref,
            } => write!(
                f,
                "Cannot reconstruct referenced file `{path}` for canon `{canon_ref}` under `--workspace-state base`: \
                 base file `{base_path}` is missing or unreadable in the base workspace.\n  \
                 Hint: Verify the base-branch checkout contains `{base_path}`, or check out the PR head and re-run with `--workspace-state head`."
            ),
            Self::OmittedPatch {
                path,
                canon_ref,
                reason,
            } => write!(
                f,
                "Cannot reconstruct referenced file `{path}` for canon `{canon_ref}` under `--workspace-state base`: \
                 unified diff patch was omitted ({}).\n  \
                 Hint: Supply a complete text unified diff for `{path}`, or check out the PR head and re-run with `--workspace-state head`.",
                format_patch_omission_reason(*reason)
            ),
            Self::PatchFailed {
                path,
                canon_ref,
                detail,
            } => write!(
                f,
                "Cannot apply filing diff to referenced file `{path}` for canon `{canon_ref}` under `--workspace-state base`: \
                 {detail}.\n  \
                 Hint: Rebase or merge the base branch so the diff applies cleanly, or check out the PR head and re-run with `--workspace-state head`."
            ),
        }
    }
}

impl std::error::Error for ReferenceOverlayError {}

struct CanonRefMatchers<'a> {
    canon_ref: &'a str,
    globs: Vec<GlobPattern>,
}

fn compile_canon_reference_matchers<'a>(
    canon_refs: &'a [String],
    workspace: &WorkspaceSnapshot,
) -> Vec<CanonRefMatchers<'a>> {
    let mut compiled = Vec::new();
    for canon_ref in canon_refs {
        let Some(canon) = workspace.canons.get(canon_ref) else {
            continue;
        };
        let globs = compile_single_canon_globs(canon);
        if !globs.is_empty() {
            compiled.push(CanonRefMatchers {
                canon_ref: canon_ref.as_str(),
                globs,
            });
        }
    }
    compiled
}

fn compile_single_canon_globs(canon: &Canon) -> Vec<GlobPattern> {
    let mut matchers = Vec::new();
    for raw_pattern in &canon.references {
        let trimmed = raw_pattern.trim();
        if trimmed.is_empty() {
            continue;
        }
        let candidate = normalize_workspace_path(trimmed).unwrap_or_else(|_| trimmed.to_string());
        if let Ok(glob) = GlobPattern::parse(&candidate) {
            matchers.push(glob);
        }
    }
    matchers
}

fn matching_canon_ref<'a>(path: &str, matchers: &'a [CanonRefMatchers<'a>]) -> Option<&'a str> {
    matchers
        .iter()
        .find(|entry| entry.globs.iter().any(|g| g.is_match(path)))
        .map(|entry| entry.canon_ref)
}

/// Computes the base-branch file paths that must be hydrated from disk before calling
/// [`apply_base_diff_to_references`] when running against a live `--workspace-state base` checkout.
pub(crate) fn collect_base_reference_disk_paths(
    canon_refs: &[String],
    intake: &CaseloadIntake,
    workspace: &WorkspaceSnapshot,
) -> Vec<String> {
    if intake.workspace_state != WorkspaceState::Base || intake.diffs.is_empty() {
        return Vec::new();
    }
    let matchers = compile_canon_reference_matchers(canon_refs, workspace);
    if matchers.is_empty() {
        return Vec::new();
    }

    let mut needed = BTreeSet::new();
    for (diff_path, artifact) in &intake.diffs {
        if workspace.applied_base_diffs.contains(diff_path) {
            continue;
        }
        if matching_canon_ref(diff_path, &matchers).is_none() {
            continue;
        }
        match artifact.status {
            FileChangeStatus::Added | FileChangeStatus::Deleted => {}
            FileChangeStatus::Renamed | FileChangeStatus::Copied => {
                let base_path = artifact
                    .previous_path
                    .as_deref()
                    .unwrap_or(diff_path.as_str());
                if !workspace.file_contents.contains_key(base_path) {
                    needed.insert(base_path.to_string());
                }
            }
            FileChangeStatus::Modified | FileChangeStatus::Unchanged => {
                if !workspace.file_contents.contains_key(diff_path) {
                    needed.insert(diff_path.clone());
                }
            }
        }
    }
    needed.into_iter().collect()
}

/// Applies `.intake.diffs` in memory onto `workspace` for all files matching `references:` of
/// `canon_refs` when `intake.workspace_state == WorkspaceState::Base`.
///
/// Idempotent: paths already recorded in `workspace.applied_base_diffs` are skipped.
pub(crate) fn apply_base_diff_to_references(
    canon_refs: &[String],
    intake: &CaseloadIntake,
    workspace: &mut WorkspaceSnapshot,
) -> Result<(), ReferenceOverlayError> {
    if intake.workspace_state != WorkspaceState::Base || intake.diffs.is_empty() {
        return Ok(());
    }
    let matchers = compile_canon_reference_matchers(canon_refs, workspace);
    if matchers.is_empty() {
        return Ok(());
    }

    let mut overlay_updates: Vec<(String, Option<String>)> = Vec::new();
    let mut removals: Vec<String> = Vec::new();

    for (diff_path, artifact) in &intake.diffs {
        if workspace.applied_base_diffs.contains(diff_path) {
            continue;
        }
        let target_canon = matching_canon_ref(diff_path, &matchers);
        let prev_canon = artifact
            .previous_path
            .as_deref()
            .and_then(|p| matching_canon_ref(p, &matchers));

        if target_canon.is_none() && prev_canon.is_none() {
            continue;
        }

        if artifact.status == FileChangeStatus::Deleted {
            removals.push(diff_path.clone());
            continue;
        }

        if artifact.status == FileChangeStatus::Renamed
            && let Some(prev) = artifact.previous_path.as_ref()
            && prev != diff_path
            && !intake.diffs.contains_key(prev)
        {
            removals.push(prev.clone());
        }

        if let Some(canon_ref) = target_canon {
            let post_content =
                reconstruct_post_patch_content(diff_path, artifact, canon_ref, workspace)?;
            overlay_updates.push((diff_path.clone(), Some(post_content)));
        }
    }

    for removed in removals {
        workspace.existing_files.remove(&removed);
        workspace.file_contents.remove(&removed);
        workspace.applied_base_diffs.insert(removed);
    }
    for (path, maybe_content) in overlay_updates {
        workspace.existing_files.insert(path.clone());
        if let Some(content) = maybe_content {
            workspace.file_contents.insert(path.clone(), content);
        }
        workspace.applied_base_diffs.insert(path);
    }

    Ok(())
}

fn reconstruct_post_patch_content(
    diff_path: &str,
    artifact: &FileArtifact,
    canon_ref: &str,
    workspace: &WorkspaceSnapshot,
) -> Result<String, ReferenceOverlayError> {
    if let Some(content) = artifact.content.as_ref() {
        return Ok(content.clone());
    }

    if artifact.status == FileChangeStatus::Added {
        return reconstruct_added_file(diff_path, artifact, canon_ref);
    }

    let base_path = match artifact.status {
        FileChangeStatus::Renamed | FileChangeStatus::Copied => {
            artifact.previous_path.as_deref().unwrap_or(diff_path)
        }
        _ => diff_path,
    };
    let Some(base_content) = workspace.file_contents.get(base_path) else {
        return Err(ReferenceOverlayError::MissingBaseFile {
            path: diff_path.to_string(),
            base_path: base_path.to_string(),
            canon_ref: canon_ref.to_string(),
        });
    };

    if let Some(patch) = artifact.patch.as_deref() {
        return apply_unified_patch(base_content, Some(base_path), diff_path, patch, canon_ref);
    }

    if artifact.patch_omission_reason == Some(PatchOmissionReason::Unchanged) {
        return Ok(base_content.clone());
    }

    Err(ReferenceOverlayError::OmittedPatch {
        path: diff_path.to_string(),
        canon_ref: canon_ref.to_string(),
        reason: artifact.patch_omission_reason,
    })
}

fn reconstruct_added_file(
    diff_path: &str,
    artifact: &FileArtifact,
    canon_ref: &str,
) -> Result<String, ReferenceOverlayError> {
    if let Some(patch) = artifact.patch.as_deref() {
        return apply_unified_patch("", None, diff_path, patch, canon_ref);
    }
    if artifact.patch_omission_reason == Some(PatchOmissionReason::Unchanged) {
        return Ok(String::new());
    }
    Err(ReferenceOverlayError::OmittedPatch {
        path: diff_path.to_string(),
        canon_ref: canon_ref.to_string(),
        reason: artifact.patch_omission_reason,
    })
}

fn apply_unified_patch(
    base_content: &str,
    old_path: Option<&str>,
    new_path: &str,
    raw_hunks: &str,
    canon_ref: &str,
) -> Result<String, ReferenceOverlayError> {
    let old_header = old_path.map_or_else(|| "/dev/null".to_string(), |p| format!("a/{p}"));
    let mut full_patch =
        String::with_capacity(raw_hunks.len() + old_header.len() + new_path.len() + 24);
    full_patch.push_str("--- ");
    full_patch.push_str(&old_header);
    full_patch.push_str("\n+++ b/");
    full_patch.push_str(new_path);
    full_patch.push('\n');
    full_patch.push_str(raw_hunks);
    if !raw_hunks.ends_with('\n') {
        full_patch.push('\n');
    }

    let parsed =
        diffy::Patch::from_str(&full_patch).map_err(|err| ReferenceOverlayError::PatchFailed {
            path: new_path.to_string(),
            canon_ref: canon_ref.to_string(),
            detail: format!("failed to parse unified patch: {err}"),
        })?;

    diffy::apply(base_content, &parsed).map_err(|err| ReferenceOverlayError::PatchFailed {
        path: new_path.to_string(),
        canon_ref: canon_ref.to_string(),
        detail: format!("failed to apply hunk(s) to base file: {err}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::intake::parse_unified_diff;

    fn sample_workspace_with_canon(canon_md: &str) -> WorkspaceSnapshot {
        WorkspaceSnapshot::new().with_canon_markdown(
            canon_md,
            &ParseCanonOptions::with_source_ref(".canons/ref-check.md"),
        )
    }

    fn base_intake_from_diff(diff: &str) -> CaseloadIntake {
        let diffs = parse_unified_diff(diff).unwrap_or_default();
        CaseloadIntake {
            workspace_state: WorkspaceState::Base,
            diffs,
            ..CaseloadIntake::default()
        }
    }

    #[test]
    fn test_head_workspace_state_is_noop() {
        let mut ws = sample_workspace_with_canon(
            "---\ntriggers:\n  - \"src/**\"\nreferences:\n  - \"README.md\"\n---\nKeep README updated.\n",
        )
        .with_file_content("README.md", "base line\n");
        let intake = CaseloadIntake {
            workspace_state: WorkspaceState::Head,
            diffs: parse_unified_diff(
                "diff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-base line\n+head line\n",
            )
            .unwrap_or_default(),
            ..CaseloadIntake::default()
        };

        let result =
            apply_base_diff_to_references(&[".canons/ref-check.md".to_string()], &intake, &mut ws);
        assert!(result.is_ok());
        assert_eq!(
            ws.file_contents.get("README.md").map(String::as_str),
            Some("base line\n")
        );
    }

    #[test]
    fn test_modified_reference_applies_cleanly_and_is_idempotent() {
        let mut ws = sample_workspace_with_canon(
            "---\ntriggers:\n  - \"README.md\"\nreferences:\n  - \".github/settings.yml\"\n---\nSync settings.\n",
        )
        .with_file_content(
            ".github/settings.yml",
            "repository:\n  description: Old description\n  homepage: https://example.com\n",
        );
        let diff = "\
diff --git a/.github/settings.yml b/.github/settings.yml
--- a/.github/settings.yml
+++ b/.github/settings.yml
@@ -1,3 +1,3 @@
 repository:
-  description: Old description
+  description: New description
   homepage: https://example.com
";
        let intake = base_intake_from_diff(diff);
        let canons = vec![".canons/ref-check.md".to_string()];

        assert!(apply_base_diff_to_references(&canons, &intake, &mut ws).is_ok());
        assert_eq!(
            ws.file_contents
                .get(".github/settings.yml")
                .map(String::as_str),
            Some("repository:\n  description: New description\n  homepage: https://example.com\n")
        );

        // Second call must be a clean idempotent no-op (must NOT re-apply patch).
        assert!(apply_base_diff_to_references(&canons, &intake, &mut ws).is_ok());
    }

    #[test]
    fn test_offset_shifted_hunk_applies_cleanly() {
        let mut ws = sample_workspace_with_canon(
            "---\ntriggers:\n  - \"src/**\"\nreferences:\n  - \"README.md\"\n---\nKeep README updated.\n",
        )
        .with_file_content(
            "README.md",
            "inserted on main 1\ninserted on main 2\ninserted on main 3\n# Title\n\nAlpha\nBeta\nGamma\n",
        );
        // Diff was generated before `main` inserted 3 lines at the top.
        let diff = "\
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1,5 +1,6 @@
 # Title
 
 Alpha
+New Feature
 Beta
 Gamma
";
        let intake = base_intake_from_diff(diff);
        let canons = vec![".canons/ref-check.md".to_string()];

        assert!(apply_base_diff_to_references(&canons, &intake, &mut ws).is_ok());
        let updated = ws.file_contents.get("README.md").map(String::as_str);
        assert_eq!(
            updated,
            Some(
                "inserted on main 1\ninserted on main 2\ninserted on main 3\n# Title\n\nAlpha\nNew Feature\nBeta\nGamma\n"
            )
        );
    }

    #[test]
    fn test_conflicting_hunk_pulls_andon_cord() {
        let mut ws = sample_workspace_with_canon(
            "---\ntriggers:\n  - \"src/**\"\nreferences:\n  - \"README.md\"\n---\nKeep README updated.\n",
        )
        .with_file_content("README.md", "completely different main content\n");
        let diff = "\
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1,2 +1,2 @@
-original line
+pr updated line
 trailing line
";
        let intake = base_intake_from_diff(diff);
        let err =
            apply_base_diff_to_references(&[".canons/ref-check.md".to_string()], &intake, &mut ws);
        assert!(matches!(
            err,
            Err(ReferenceOverlayError::PatchFailed { .. })
        ));
    }

    #[test]
    fn test_added_deleted_and_renamed_references() {
        let mut ws = sample_workspace_with_canon(
            "---\ntriggers:\n  - \"src/**\"\nreferences:\n  - \"docs/*.md\"\n---\nKeep docs aligned.\n",
        )
        .with_file_content("docs/obsolete.md", "old doc\n")
        .with_file_content("docs/old-name.md", "header\nbody\n");

        let diff = "\
diff --git a/docs/new-guide.md b/docs/new-guide.md
new file mode 100644
--- /dev/null
+++ b/docs/new-guide.md
@@ -0,0 +1,2 @@
+# New Guide
+Details here
diff --git a/docs/obsolete.md b/docs/obsolete.md
deleted file mode 100644
--- a/docs/obsolete.md
+++ /dev/null
@@ -1 +0,0 @@
-old doc
diff --git a/docs/old-name.md b/docs/renamed-guide.md
similarity index 80%
rename from docs/old-name.md
rename to docs/renamed-guide.md
--- a/docs/old-name.md
+++ b/docs/renamed-guide.md
@@ -1,2 +1,2 @@
 header
-body
+updated body
";
        let intake = base_intake_from_diff(diff);
        let canons = vec![".canons/ref-check.md".to_string()];

        assert!(apply_base_diff_to_references(&canons, &intake, &mut ws).is_ok());

        assert!(ws.existing_files.contains("docs/new-guide.md"));
        assert_eq!(
            ws.file_contents
                .get("docs/new-guide.md")
                .map(String::as_str),
            Some("# New Guide\nDetails here\n")
        );

        assert!(!ws.existing_files.contains("docs/obsolete.md"));
        assert!(!ws.file_contents.contains_key("docs/obsolete.md"));

        assert!(!ws.existing_files.contains("docs/old-name.md"));
        assert!(ws.existing_files.contains("docs/renamed-guide.md"));
        assert_eq!(
            ws.file_contents
                .get("docs/renamed-guide.md")
                .map(String::as_str),
            Some("header\nupdated body\n")
        );
    }

    #[test]
    fn test_non_referenced_conflicting_file_is_ignored() {
        let mut ws = sample_workspace_with_canon(
            "---\ntriggers:\n  - \"src/**\"\nreferences:\n  - \"README.md\"\n---\nKeep README updated.\n",
        )
        .with_file_content("README.md", "unchanged readme\n");
        let diff = "\
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1 +1 @@
-stale base line
+new line
";
        let intake = base_intake_from_diff(diff);
        let result =
            apply_base_diff_to_references(&[".canons/ref-check.md".to_string()], &intake, &mut ws);
        assert!(result.is_ok());
    }

    #[test]
    fn test_omitted_patch_and_missing_base_file_fail_closed() {
        let mut ws = sample_workspace_with_canon(
            "---\ntriggers:\n  - \"src/**\"\nreferences:\n  - \"assets/**\"\n  - \"README.md\"\n---\nKeep aligned.\n",
        );
        let binary_diff = "\
diff --git a/assets/diagram.png b/assets/diagram.png
Binary files a/assets/diagram.png and b/assets/diagram.png differ
";
        let binary_intake = base_intake_from_diff(binary_diff);
        ws = ws.with_file_content("assets/diagram.png", "stub");
        let err = apply_base_diff_to_references(
            &[".canons/ref-check.md".to_string()],
            &binary_intake,
            &mut ws,
        );
        assert!(matches!(
            err,
            Err(ReferenceOverlayError::OmittedPatch {
                reason: Some(PatchOmissionReason::Binary),
                ..
            })
        ));
        let omitted_msg = err.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(omitted_msg.contains("binary file"));
        assert!(omitted_msg.contains("Hint:"));
        assert!(omitted_msg.contains("--workspace-state head"));

        let missing_base_diff = "\
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1 +1 @@
-old
+new
";
        let missing_intake = base_intake_from_diff(missing_base_diff);
        let err2 = apply_base_diff_to_references(
            &[".canons/ref-check.md".to_string()],
            &missing_intake,
            &mut ws,
        );
        assert!(matches!(
            err2,
            Err(ReferenceOverlayError::MissingBaseFile { .. })
        ));
        let missing_msg = err2.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(missing_msg.contains("Hint:"));
        assert!(missing_msg.contains("--workspace-state head"));
    }

    #[test]
    fn test_pure_rename_and_collect_base_reference_disk_paths() {
        let mut ws = sample_workspace_with_canon(
            "---\ntriggers:\n  - \"src/**\"\nreferences:\n  - \"docs/*.md\"\n---\nKeep docs aligned.\n",
        )
        .with_file("docs/old.md");
        let diff = "\
diff --git a/docs/old.md b/docs/new.md
similarity index 100%
rename from docs/old.md
rename to docs/new.md
";
        let intake = base_intake_from_diff(diff);
        let canons = vec![".canons/ref-check.md".to_string()];

        let disk_paths = collect_base_reference_disk_paths(&canons, &intake, &ws);
        assert_eq!(disk_paths, vec!["docs/old.md".to_string()]);

        ws = ws.with_file_content("docs/old.md", "pure rename content\n");
        assert!(apply_base_diff_to_references(&canons, &intake, &mut ws).is_ok());
        assert!(!ws.existing_files.contains("docs/old.md"));
        assert!(ws.existing_files.contains("docs/new.md"));
        assert_eq!(
            ws.file_contents.get("docs/new.md").map(String::as_str),
            Some("pure rename content\n")
        );
    }
}
