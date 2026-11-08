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

//! Stage 2 semantic AST builder for unified diff token streams.

use super::lexer::{DiffParseError, DiffToken, DiffTokenKind};
use crate::caseload::{ContentOmissionReason, FileArtifact, FileChangeStatus, PatchOmissionReason};
use std::collections::BTreeMap;

#[derive(Default)]
struct FileDraft {
    old_path: Option<String>,
    new_path: Option<String>,
    is_new_file: bool,
    is_deleted_file: bool,
    is_rename: bool,
    is_copy: bool,
    is_binary: bool,
    lines_added: usize,
    lines_deleted: usize,
    patch_lines: Vec<String>,
}

impl FileDraft {
    fn into_artifact(self) -> Option<FileArtifact> {
        let status = if self.is_new_file {
            FileChangeStatus::Added
        } else if self.is_deleted_file {
            FileChangeStatus::Deleted
        } else if self.is_copy {
            FileChangeStatus::Copied
        } else if self.is_rename {
            FileChangeStatus::Renamed
        } else {
            FileChangeStatus::Modified
        };

        let path = match status {
            FileChangeStatus::Deleted => self.old_path.clone().or(self.new_path.clone())?,
            _ => self.new_path.clone().or(self.old_path.clone())?,
        };

        if path.is_empty() || path == "/dev/null" {
            return None;
        }

        let previous_path = match status {
            FileChangeStatus::Renamed | FileChangeStatus::Copied => self
                .old_path
                .filter(|old| old != &path && old != "/dev/null"),
            _ => None,
        };

        let (patch, patch_omission_reason) = if self.is_binary {
            (None, Some(PatchOmissionReason::Binary))
        } else if self.patch_lines.is_empty() {
            (None, Some(PatchOmissionReason::Unchanged))
        } else {
            (Some(self.patch_lines.join("\n")), None)
        };

        let content_omission_reason = if self.is_binary {
            Some(ContentOmissionReason::Binary)
        } else if status == FileChangeStatus::Deleted {
            Some(ContentOmissionReason::Deleted)
        } else {
            Some(ContentOmissionReason::NotRequested)
        };

        Some(FileArtifact {
            path,
            status,
            previous_path,
            lines_added: self.lines_added,
            lines_deleted: self.lines_deleted,
            patch,
            patch_omission_reason,
            content: None,
            content_omission_reason,
        })
    }
}

fn commit_draft(draft: Option<FileDraft>, artifacts: &mut BTreeMap<String, FileArtifact>) {
    if let Some(d) = draft
        && let Some(artifact) = d.into_artifact()
    {
        if let Some(existing) = artifacts.get_mut(&artifact.path) {
            // Codify multi-occurrence semantics: merge subsequent patches for the same file path
            existing.lines_added += artifact.lines_added;
            existing.lines_deleted += artifact.lines_deleted;
            if let Some(new_patch) = artifact.patch {
                match &mut existing.patch {
                    Some(prev_patch) => {
                        prev_patch.push('\n');
                        prev_patch.push_str(&new_patch);
                    }
                    None => {
                        existing.patch = Some(new_patch);
                        existing.patch_omission_reason = None;
                    }
                }
            }
            if artifact.status == FileChangeStatus::Deleted {
                existing.status = FileChangeStatus::Deleted;
                existing.content_omission_reason = Some(ContentOmissionReason::Deleted);
            }
        } else {
            artifacts.insert(artifact.path.clone(), artifact);
        }
    }
}

fn apply_header_metadata(kind: &DiffTokenKind, current: &mut Option<FileDraft>) {
    let Some(draft) = current.as_mut() else {
        return;
    };
    match kind {
        DiffTokenKind::NewFileMode(_) => {
            draft.is_new_file = true;
            draft.is_rename = false;
        }
        DiffTokenKind::DeletedFileMode(_) => {
            draft.is_deleted_file = true;
            draft.is_rename = false;
        }
        DiffTokenKind::RenameFrom(old_path) => {
            draft.is_rename = true;
            draft.old_path = Some(old_path.clone());
        }
        DiffTokenKind::RenameTo(new_path) => {
            draft.is_rename = true;
            draft.new_path = Some(new_path.clone());
        }
        DiffTokenKind::CopyFrom(old_path) => {
            draft.is_copy = true;
            draft.is_rename = false;
            draft.old_path = Some(old_path.clone());
        }
        DiffTokenKind::CopyTo(new_path) => {
            draft.is_copy = true;
            draft.is_rename = false;
            draft.new_path = Some(new_path.clone());
        }
        DiffTokenKind::BinaryMarker(_) => {
            draft.is_binary = true;
        }
        DiffTokenKind::NoNewlineAtEof(raw) => {
            draft.patch_lines.push(raw.clone());
        }
        _ => {}
    }
}

fn apply_hunk_token(
    line_number: usize,
    kind: &DiffTokenKind,
    current: &mut Option<FileDraft>,
) -> Result<(), DiffParseError> {
    match kind {
        DiffTokenKind::HunkHeader { raw, .. } => {
            let draft = current.as_mut().ok_or_else(|| DiffParseError {
                line_number,
                message: "hunk header `@@` encountered outside of a file diff block".to_string(),
            })?;
            draft.patch_lines.push(raw.clone());
        }
        DiffTokenKind::HunkAdd(text) => {
            let draft = current.as_mut().ok_or_else(|| DiffParseError {
                line_number,
                message: "added hunk line encountered outside of a file diff block".to_string(),
            })?;
            draft.lines_added += 1;
            draft.patch_lines.push(format!("+{text}"));
        }
        DiffTokenKind::HunkDelete(text) => {
            let draft = current.as_mut().ok_or_else(|| DiffParseError {
                line_number,
                message: "deleted hunk line encountered outside of a file diff block".to_string(),
            })?;
            draft.lines_deleted += 1;
            draft.patch_lines.push(format!("-{text}"));
        }
        DiffTokenKind::HunkContext(text) => {
            let draft = current.as_mut().ok_or_else(|| DiffParseError {
                line_number,
                message: "context hunk line encountered outside of a file diff block".to_string(),
            })?;
            draft.patch_lines.push(format!(" {text}"));
        }
        _ => {}
    }
    Ok(())
}

/// Stage 2 Semantic AST Builder: Assembles [`FileArtifact`] records from a [`DiffToken`] slice.
pub fn assemble_diff_artifacts(
    tokens: &[DiffToken],
) -> Result<BTreeMap<String, FileArtifact>, DiffParseError> {
    if tokens.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut artifacts = BTreeMap::new();
    let mut current: Option<FileDraft> = None;
    let mut saw_file_demarcation = false;

    for token in tokens {
        match &token.kind {
            DiffTokenKind::DiffGitHeader { old_path, new_path } => {
                saw_file_demarcation = true;
                commit_draft(current.take(), &mut artifacts);
                current = Some(FileDraft {
                    old_path: Some(old_path.clone()),
                    new_path: Some(new_path.clone()),
                    is_rename: old_path != new_path,
                    ..FileDraft::default()
                });
            }
            DiffTokenKind::MinusFile(path) => {
                saw_file_demarcation = true;
                // Support standard unified diffs without a `diff --git` header
                if current
                    .as_ref()
                    .is_none_or(|draft| !draft.patch_lines.is_empty())
                {
                    commit_draft(current.take(), &mut artifacts);
                    current = Some(FileDraft::default());
                }
                if let Some(draft) = current.as_mut() {
                    if path == "/dev/null" {
                        draft.is_new_file = true;
                        draft.is_rename = false;
                    } else {
                        draft.old_path = Some(path.clone());
                    }
                }
            }
            DiffTokenKind::PlusFile(path) => {
                let draft = current.as_mut().ok_or_else(|| DiffParseError {
                    line_number: token.line_number,
                    message:
                        "`+++` header encountered without preceding `diff --git` or `---` header"
                            .to_string(),
                })?;
                if path == "/dev/null" {
                    draft.is_deleted_file = true;
                    draft.is_rename = false;
                } else {
                    draft.new_path = Some(path.clone());
                }
            }
            DiffTokenKind::HunkHeader { .. }
            | DiffTokenKind::HunkAdd(_)
            | DiffTokenKind::HunkDelete(_)
            | DiffTokenKind::HunkContext(_) => {
                apply_hunk_token(token.line_number, &token.kind, &mut current)?;
            }
            _ => apply_header_metadata(&token.kind, &mut current),
        }
    }

    if !saw_file_demarcation {
        return Err(DiffParseError {
            line_number: tokens[0].line_number,
            message: "input does not contain any recognizable unified diff headers (`diff --git` or `---`/`+++`)"
                .to_string(),
        });
    }

    commit_draft(current.take(), &mut artifacts);
    Ok(artifacts)
}
