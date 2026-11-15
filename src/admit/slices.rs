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

//! Context- and count-budgeted candidate exhibit slicing for generative `admit` (`docs/architecture/nodes/admit.md` §4).

use super::exhibits::render_candidate_exhibit;
use super::prompt::{
    append_filing_overview, build_admit_case_user_prompt, build_admit_slice_user_prompt,
    build_sliced_filing_overview,
};
use crate::canon::Canon;
use crate::caseload::{CaseloadDiscovery, CaseloadIntake};
use crate::discover::WorkspaceSnapshot;
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Default per-slice prompt character budget (`~15k` tokens) keeping generative micro-triage latency bounded.
pub const MAX_ADMIT_SLICE_CHARS: usize = 60_000;

/// Default maximum candidate exhibits evaluated in a single generative `admit` slice turn.
pub const MAX_ADMIT_EXHIBITS_PER_SLICE: usize = 15;

const ADMIT_CANON_FRAME_RESERVE_CHARS: usize = 4_096;
const SLICE_HEADING_RESERVE_CHARS: usize = 128;

/// Canon-invariant slice plan entry capturing one disjoint `candidate_exhibits` subset and its
/// optional pre-assembled leading prompt prefix (`## Filing Overview` + `## Candidate Exhibits`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PreparedExhibitSlice {
    pub(super) candidate_exhibits: Vec<String>,
    pub(super) shared_prefix: Option<String>,
}

/// Concrete isolated prompt turn for one slice of an active generative `admit` case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AdmitCaseSlice {
    pub(super) user_prompt: String,
    pub(super) candidate_exhibits: Vec<String>,
}

/// Memoizing planner that partitions `candidate_exhibits` independently of governing canons and
/// reuses pre-rendered exhibit blocks and shared slice prefixes across sibling active cases.
pub(super) struct ExhibitSlicePlanner<'a> {
    intake: &'a CaseloadIntake,
    discovery: &'a CaseloadDiscovery,
    workspace: &'a WorkspaceSnapshot,
    unsliced_overview_len: usize,
    sliced_overview: String,
    max_slice_chars: usize,
    max_exhibits_per_slice: usize,
    rendered_cache: BTreeMap<String, String>,
    plan_cache: BTreeMap<Vec<String>, Vec<PreparedExhibitSlice>>,
}

impl<'a> ExhibitSlicePlanner<'a> {
    pub(super) fn new(
        intake: &'a CaseloadIntake,
        discovery: &'a CaseloadDiscovery,
        workspace: &'a WorkspaceSnapshot,
        max_slice_chars: usize,
        max_exhibits_per_slice: usize,
    ) -> Self {
        let mut unsliced_overview = String::with_capacity(512);
        append_filing_overview(&mut unsliced_overview, intake, discovery);
        let sliced_overview = build_sliced_filing_overview(intake, discovery, max_slice_chars / 3);
        Self {
            intake,
            discovery,
            workspace,
            unsliced_overview_len: unsliced_overview.len(),
            sliced_overview,
            max_slice_chars,
            max_exhibits_per_slice,
            rendered_cache: BTreeMap::new(),
            plan_cache: BTreeMap::new(),
        }
    }

    pub(super) fn build_case_slices(
        &mut self,
        canon_ref: &str,
        canon: &Canon,
        candidate_exhibits: Vec<String>,
    ) -> Vec<AdmitCaseSlice> {
        if !self.plan_cache.contains_key(&candidate_exhibits) {
            let plan = self.partition_candidate_exhibits(&candidate_exhibits);
            self.plan_cache.insert(candidate_exhibits.clone(), plan);
        }
        let plan = &self.plan_cache[&candidate_exhibits];
        materialize_case_slices(
            plan,
            canon_ref,
            canon,
            self.intake,
            self.discovery,
            self.workspace,
        )
    }

    fn partition_candidate_exhibits(
        &mut self,
        candidate_exhibits: &[String],
    ) -> Vec<PreparedExhibitSlice> {
        if candidate_exhibits.is_empty() {
            return Vec::new();
        }

        for id in candidate_exhibits {
            if !self.rendered_cache.contains_key(id) {
                self.rendered_cache.insert(
                    id.clone(),
                    render_candidate_exhibit(id, self.intake, self.workspace),
                );
            }
        }

        let effective_max_items = self.max_exhibits_per_slice.max(1);
        let canon_reserve = ADMIT_CANON_FRAME_RESERVE_CHARS.min(self.max_slice_chars / 4);

        let total_item_cost: usize = candidate_exhibits
            .iter()
            .map(|id| item_prompt_cost(id, &self.rendered_cache))
            .sum();
        let single_slice_estimate = self.unsliced_overview_len
            + SLICE_HEADING_RESERVE_CHARS
            + total_item_cost
            + canon_reserve;

        if candidate_exhibits.len() <= effective_max_items
            && single_slice_estimate <= self.max_slice_chars
        {
            return vec![PreparedExhibitSlice {
                candidate_exhibits: candidate_exhibits.to_vec(),
                shared_prefix: None,
            }];
        }

        let body_budget = self
            .max_slice_chars
            .saturating_sub(
                self.sliced_overview.len() + canon_reserve + SLICE_HEADING_RESERVE_CHARS,
            )
            .max(1);
        let groups = pack_exhibit_groups(
            candidate_exhibits,
            &self.rendered_cache,
            body_budget,
            effective_max_items,
        );

        if groups.len() <= 1 {
            return vec![PreparedExhibitSlice {
                candidate_exhibits: candidate_exhibits.to_vec(),
                shared_prefix: None,
            }];
        }

        let total_slices = groups.len();
        groups
            .into_iter()
            .enumerate()
            .map(|(idx, group)| {
                let rendered_bytes: usize = group
                    .iter()
                    .map(|id| self.rendered_cache.get(id).map_or(0, String::len))
                    .sum();
                let mut prefix = String::with_capacity(
                    self.sliced_overview.len() + rendered_bytes + SLICE_HEADING_RESERVE_CHARS,
                );
                prefix.push_str(&self.sliced_overview);
                let _ = writeln!(
                    prefix,
                    "## Candidate Exhibits (Slice {} of {}, {} exhibit(s))\n",
                    idx + 1,
                    total_slices,
                    group.len()
                );
                for id in &group {
                    if let Some(rendered) = self.rendered_cache.get(id) {
                        prefix.push_str(rendered);
                    }
                }
                PreparedExhibitSlice {
                    candidate_exhibits: group,
                    shared_prefix: Some(prefix),
                }
            })
            .collect()
    }
}

fn materialize_case_slices(
    prepared_slices: &[PreparedExhibitSlice],
    canon_ref: &str,
    canon: &Canon,
    intake: &CaseloadIntake,
    discovery: &CaseloadDiscovery,
    workspace: &WorkspaceSnapshot,
) -> Vec<AdmitCaseSlice> {
    prepared_slices
        .iter()
        .map(|slice| {
            let user_prompt = match &slice.shared_prefix {
                Some(prefix) => build_admit_slice_user_prompt(
                    prefix,
                    canon_ref,
                    canon,
                    &slice.candidate_exhibits,
                ),
                None => build_admit_case_user_prompt(
                    canon_ref,
                    canon,
                    &slice.candidate_exhibits,
                    intake,
                    discovery,
                    workspace,
                ),
            };
            AdmitCaseSlice {
                user_prompt,
                candidate_exhibits: slice.candidate_exhibits.clone(),
            }
        })
        .collect()
}

fn item_prompt_cost(exhibit_id: &str, rendered_cache: &BTreeMap<String, String>) -> usize {
    let rendered_len = rendered_cache.get(exhibit_id).map_or(0, String::len);
    rendered_len + exhibit_id.len() + 8
}

fn pack_exhibit_groups(
    candidate_exhibits: &[String],
    rendered_cache: &BTreeMap<String, String>,
    body_budget: usize,
    max_exhibits_per_slice: usize,
) -> Vec<Vec<String>> {
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut current_cost = 0usize;

    for id in candidate_exhibits {
        let cost = item_prompt_cost(id, rendered_cache);
        if cost > body_budget {
            if !current.is_empty() {
                groups.push(current);
                current = Vec::new();
                current_cost = 0;
            }
            groups.push(vec![id.clone()]);
            continue;
        }
        if !current.is_empty()
            && (current.len() >= max_exhibits_per_slice || current_cost + cost > body_budget)
        {
            groups.push(current);
            current = Vec::new();
            current_cost = 0;
        }
        current_cost += cost;
        current.push(id.clone());
    }

    if !current.is_empty() {
        groups.push(current);
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canon::ParseCanonOptions;
    use crate::caseload::{DiscoveryMode, FileArtifact, FileChangeStatus, LinkedIssueContext};

    fn parse_test_canon(source_ref: &str, markdown: &str) -> Canon {
        let ws = WorkspaceSnapshot::new()
            .with_canon_markdown(markdown, &ParseCanonOptions::with_source_ref(source_ref));
        ws.canons
            .get(source_ref)
            .expect("parsed test canon")
            .clone()
    }

    fn make_diff_artifact(path: &str, patch_body: &str) -> FileArtifact {
        FileArtifact {
            path: path.to_string(),
            status: FileChangeStatus::Modified,
            previous_path: None,
            lines_added: 10,
            lines_deleted: 2,
            patch: Some(format!("@@ -1,2 +1,10 @@\n{patch_body}")),
            patch_omission_reason: None,
            content: None,
            content_omission_reason: None,
        }
    }

    mod dual_bounded_partitioning {
        use super::*;

        #[test]
        fn test_small_case_fast_paths_with_zero_prompt_change() {
            let canon_ref = ".canons/cli/kebab.md";
            let canon = parse_test_canon(
                canon_ref,
                "# Kebab Flags\n\nCLI flags MUST use kebab-case.\n",
            );
            let mut diffs = BTreeMap::new();
            diffs.insert(
                "src/cli/app.rs".to_string(),
                make_diff_artifact("src/cli/app.rs", "+#[arg(long = \"my-flag\")]"),
            );
            let intake = CaseloadIntake {
                pr_title: Some("feat(cli): add my-flag".to_string()),
                diffs,
                ..CaseloadIntake::default()
            };
            let discovery = CaseloadDiscovery {
                mode: DiscoveryMode::Triggered,
                ..CaseloadDiscovery::default()
            };
            let ws = WorkspaceSnapshot::new();
            let exhibits = vec!["pr_title".to_string(), "src/cli/app.rs".to_string()];

            let mut planner = ExhibitSlicePlanner::new(
                &intake,
                &discovery,
                &ws,
                MAX_ADMIT_SLICE_CHARS,
                MAX_ADMIT_EXHIBITS_PER_SLICE,
            );
            let slices = planner.build_case_slices(canon_ref, &canon, exhibits.clone());
            assert_eq!(slices.len(), 1);
            assert_eq!(slices[0].candidate_exhibits, exhibits);

            let baseline = build_admit_case_user_prompt(
                canon_ref, &canon, &exhibits, &intake, &discovery, &ws,
            );
            assert_eq!(slices[0].user_prompt, baseline);
        }

        #[test]
        fn test_count_ceiling_partitions_many_small_exhibits_into_disjoint_slices() {
            let canon_ref = ".canons/cli/kebab.md";
            let canon = parse_test_canon(
                canon_ref,
                "# Kebab Flags\n\nCLI flags MUST use kebab-case.\n",
            );
            let mut diffs = BTreeMap::new();
            let mut exhibits = Vec::new();
            for idx in 0..18 {
                let path = format!("src/cli/cmd_{idx:02}.rs");
                diffs.insert(path.clone(), make_diff_artifact(&path, "+pub fn run() {}"));
                exhibits.push(path);
            }
            let intake = CaseloadIntake {
                diffs,
                ..CaseloadIntake::default()
            };
            let discovery = CaseloadDiscovery::default();
            let ws = WorkspaceSnapshot::new();

            let mut planner = ExhibitSlicePlanner::new(
                &intake,
                &discovery,
                &ws,
                MAX_ADMIT_SLICE_CHARS,
                MAX_ADMIT_EXHIBITS_PER_SLICE,
            );
            let slices = planner.build_case_slices(canon_ref, &canon, exhibits.clone());
            assert_eq!(slices.len(), 2);
            assert_eq!(slices[0].candidate_exhibits.len(), 15);
            assert_eq!(slices[1].candidate_exhibits.len(), 3);
            assert!(
                slices[0]
                    .user_prompt
                    .contains("## Candidate Exhibits (Slice 1 of 2, 15 exhibit(s))")
            );
            assert!(
                slices[1]
                    .user_prompt
                    .contains("## Candidate Exhibits (Slice 2 of 2, 3 exhibit(s))")
            );
            assert!(
                !slices[0]
                    .user_prompt
                    .contains("### Candidate Exhibit: `src/cli/cmd_15.rs`")
            );
            assert!(
                !slices[1]
                    .user_prompt
                    .contains("### Candidate Exhibit: `src/cli/cmd_00.rs`")
            );

            let reassembled: Vec<String> = slices
                .into_iter()
                .flat_map(|s| s.candidate_exhibits)
                .collect();
            assert_eq!(reassembled, exhibits);
        }

        #[test]
        fn test_char_budget_partitions_large_diffs_and_preserves_oversized_singleton() {
            let canon_ref = ".canons/core/pure.md";
            let canon =
                parse_test_canon(canon_ref, "# Pure Domain\n\nDomain code MUST stay pure.\n");
            let mut diffs = BTreeMap::new();
            diffs.insert(
                "src/a_small.rs".to_string(),
                make_diff_artifact("src/a_small.rs", &"+let a = 1;\n".repeat(12)),
            );
            diffs.insert(
                "src/b_huge.rs".to_string(),
                make_diff_artifact("src/b_huge.rs", &"+let huge = 2;\n".repeat(180)),
            );
            diffs.insert(
                "src/c_small.rs".to_string(),
                make_diff_artifact("src/c_small.rs", &"+let c = 3;\n".repeat(12)),
            );
            let exhibits = vec![
                "src/a_small.rs".to_string(),
                "src/b_huge.rs".to_string(),
                "src/c_small.rs".to_string(),
            ];
            let intake = CaseloadIntake {
                diffs,
                ..CaseloadIntake::default()
            };
            let discovery = CaseloadDiscovery::default();
            let ws = WorkspaceSnapshot::new();

            let mut planner = ExhibitSlicePlanner::new(
                &intake,
                &discovery,
                &ws,
                2_400,
                MAX_ADMIT_EXHIBITS_PER_SLICE,
            );
            let slices = planner.build_case_slices(canon_ref, &canon, exhibits);
            assert_eq!(slices.len(), 3);
            assert_eq!(slices[0].candidate_exhibits, vec!["src/a_small.rs"]);
            assert_eq!(slices[1].candidate_exhibits, vec!["src/b_huge.rs"]);
            assert_eq!(slices[2].candidate_exhibits, vec!["src/c_small.rs"]);
        }
    }

    mod global_context_and_prefix_caching {
        use super::*;

        fn shared_twenty_exhibit_filing() -> (CaseloadIntake, CaseloadDiscovery, Vec<String>) {
            let mut diffs = BTreeMap::new();
            diffs.insert(
                "README.md".to_string(),
                make_diff_artifact("README.md", "+Updated user-facing CLI docs."),
            );
            let mut shared_exhibits = vec![
                "README.md".to_string(),
                "linked_issues".to_string(),
                "pr_body".to_string(),
                "pr_title".to_string(),
            ];
            for idx in 0..16 {
                let path = format!("src/mod_{idx:02}.rs");
                diffs.insert(path.clone(), make_diff_artifact(&path, "+pub struct Mod;"));
                shared_exhibits.push(path);
            }
            shared_exhibits.sort();

            let intake = CaseloadIntake {
                pr_title: Some("feat(core): slice admit exhibits".to_string()),
                pr_body: Some("Closes #313 with dual-bounded exhibit slicing.".to_string()),
                diffs,
                linked_issues: Some(vec![LinkedIssueContext {
                    number: 313,
                    title: Some("Partition large generative admit cases".to_string()),
                    body: Some("Prevent 60s timeouts on 46-file PRs.".to_string()),
                    url: None,
                }]),
                ..CaseloadIntake::default()
            };
            let discovery = CaseloadDiscovery {
                mode: DiscoveryMode::Triggered,
                ..CaseloadDiscovery::default()
            };
            (intake, discovery, shared_exhibits)
        }

        #[test]
        fn test_every_slice_retains_global_pr_metadata_and_filing_manifest() {
            let canon_ref = ".canons/readme/feat-readme.md";
            let canon = parse_test_canon(
                canon_ref,
                "# Feat Updates README\n\nFeature PRs (`feat`) MUST update `README.md`.\n",
            );
            let (intake, discovery, shared_exhibits) = shared_twenty_exhibit_filing();
            let ws = WorkspaceSnapshot::new();

            let mut planner = ExhibitSlicePlanner::new(
                &intake,
                &discovery,
                &ws,
                MAX_ADMIT_SLICE_CHARS,
                MAX_ADMIT_EXHIBITS_PER_SLICE,
            );
            let slices = planner.build_case_slices(canon_ref, &canon, shared_exhibits);
            assert_eq!(slices.len(), 2);

            assert!(
                !slices[1]
                    .candidate_exhibits
                    .contains(&"README.md".to_string())
            );
            assert!(
                !slices[1]
                    .candidate_exhibits
                    .contains(&"pr_body".to_string())
            );
            for slice in &slices {
                assert!(
                    slice
                        .user_prompt
                        .contains("- PR Title: feat(core): slice admit exhibits")
                );
                assert!(
                    slice
                        .user_prompt
                        .contains("Closes #313 with dual-bounded exhibit slicing.")
                );
                assert!(
                    slice
                        .user_prompt
                        .contains("Issue #313: Partition large generative admit cases")
                );
                assert!(
                    slice
                        .user_prompt
                        .contains("### All Filing File Paths in Scope")
                );
                assert!(slice.user_prompt.contains("1. `README.md`"));
                assert!(!slice.user_prompt.contains("Modified Files in Filing ("));
            }
        }

        #[test]
        fn test_sibling_canons_share_byte_identical_slice_prefixes() {
            let short_ref = ".canons/readme/feat-readme.md";
            let long_ref = ".canons/commits/fix-only.md";
            let short_canon = parse_test_canon(
                short_ref,
                "# Feat Updates README\n\nFeature PRs (`feat`) MUST update `README.md`.\n",
            );
            let long_canon = parse_test_canon(
                long_ref,
                &format!(
                    "# Fix Defect Scope\n\nBug-fix PRs (`fix`) MUST include regression tests.\n\n{}\n",
                    "Detailed statutory rationale paragraph. ".repeat(40)
                ),
            );
            let (intake, discovery, shared_exhibits) = shared_twenty_exhibit_filing();
            let ws = WorkspaceSnapshot::new();

            let mut planner = ExhibitSlicePlanner::new(
                &intake,
                &discovery,
                &ws,
                MAX_ADMIT_SLICE_CHARS,
                MAX_ADMIT_EXHIBITS_PER_SLICE,
            );
            let short_slices =
                planner.build_case_slices(short_ref, &short_canon, shared_exhibits.clone());
            let long_slices = planner.build_case_slices(long_ref, &long_canon, shared_exhibits);

            assert_eq!(short_slices.len(), 2);
            assert_eq!(short_slices.len(), long_slices.len());
            for (s_short, s_long) in short_slices.iter().zip(long_slices.iter()) {
                assert_eq!(s_short.candidate_exhibits, s_long.candidate_exhibits);
                let split_short = s_short
                    .user_prompt
                    .find("## Active Case Governing Canon:")
                    .expect("short canon marker");
                let split_long = s_long
                    .user_prompt
                    .find("## Active Case Governing Canon:")
                    .expect("long canon marker");
                assert_eq!(
                    &s_short.user_prompt[..split_short],
                    &s_long.user_prompt[..split_long]
                );
            }
        }
    }
}
