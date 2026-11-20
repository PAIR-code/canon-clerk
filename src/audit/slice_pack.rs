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

//! Context-budgeted bin-packing and pinned-reference formatting shared by `audit` and `vet` slices.

use std::fmt::Write as _;

/// Pre-rendered single file exhibit paired with its repository-relative POSIX path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderedFileExhibit {
    pub(crate) file_path: String,
    pub(crate) rendered: String,
}

pub(crate) fn format_pinned_reference_exhibits(pinned_refs: &[&RenderedFileExhibit]) -> String {
    let total_chars: usize = pinned_refs.iter().map(|r| r.rendered.len()).sum();
    let mut out = String::with_capacity(total_chars + 128);
    let _ = writeln!(
        out,
        "## Pinned Companion Reference Exhibits ({})\n",
        pinned_refs.len()
    );
    for item in pinned_refs {
        out.push_str(&item.rendered);
    }
    out
}

/// Packs rendered file exhibits into contiguous slices bounded by `body_budget`.
/// When `allow_oversized_singleton` is `true`, an exhibit exceeding `body_budget` is emitted in its
/// own singleton slice; when `false`, returns `None` so callers can escalate oversized exhibits.
pub(crate) fn pack_rendered_exhibit_slices<'a>(
    exhibits: &'a [RenderedFileExhibit],
    body_budget: usize,
    allow_oversized_singleton: bool,
) -> Option<Vec<Vec<&'a RenderedFileExhibit>>> {
    let mut groups: Vec<Vec<&'a RenderedFileExhibit>> = Vec::new();
    let mut current: Vec<&'a RenderedFileExhibit> = Vec::new();
    let mut current_len = 0usize;

    for item in exhibits {
        if item.rendered.len() > body_budget {
            if !allow_oversized_singleton {
                return None;
            }
            if !current.is_empty() {
                groups.push(current);
                current = Vec::new();
                current_len = 0;
            }
            groups.push(vec![item]);
            continue;
        }
        if !current.is_empty() && current_len + item.rendered.len() > body_budget {
            groups.push(current);
            current = Vec::new();
            current_len = 0;
        }
        current_len += item.rendered.len();
        current.push(item);
    }
    if !current.is_empty() {
        groups.push(current);
    }
    Some(groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod slice_bin_packing {
        use super::*;

        #[test]
        fn test_oversized_exhibit_rejects_in_strict_mode_and_emits_singleton_slice_in_audit_mode() {
            let items = vec![
                RenderedFileExhibit {
                    file_path: "src/small_a.rs".to_string(),
                    rendered: "a".repeat(200),
                },
                RenderedFileExhibit {
                    file_path: "src/huge_b.rs".to_string(),
                    rendered: "b".repeat(1200),
                },
                RenderedFileExhibit {
                    file_path: "src/small_c.rs".to_string(),
                    rendered: "c".repeat(200),
                },
            ];

            assert!(
                pack_rendered_exhibit_slices(&items, 500, false).is_none(),
                "strict mode (`vet`) must escalate oversized exhibit"
            );

            let audit_groups = pack_rendered_exhibit_slices(&items, 500, true)
                .expect("lossless mode (`audit`) must retain oversized exhibit as singleton");
            assert_eq!(audit_groups.len(), 3);
            assert_eq!(audit_groups[0][0].file_path, "src/small_a.rs");
            assert_eq!(audit_groups[1][0].file_path, "src/huge_b.rs");
            assert_eq!(audit_groups[2][0].file_path, "src/small_c.rs");
        }
    }
}
