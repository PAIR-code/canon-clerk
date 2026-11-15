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

//! Deterministic Map-Reduce decree folding across multi-slice `audit` trials (`docs/architecture/nodes/audit.md` §3–§4).

use crate::caseload::{CanonAdjudication, VerdictStatus, sort_annotations};

/// Folds one or more canonically ordered slice adjudications into a single [`CanonAdjudication`]:
/// - **Single slice (`1`):** Returns the decoded adjudication verbatim.
/// - **Multi-slice (`>= 2`):**
///   - `compliance_score`: Conjunctive minimum (`min`) across slices.
///   - `status`: Derived deterministically from `compliance_score >= threshold`.
///   - `annotations`: `[]` when `Pass`; canonical sorted and deduplicated union of failing slice annotations when `Fail`.
///   - `compliance_summary`: Deterministic slice-indexed synthesis of failing decrees when `Fail`, or passing/excused slice summaries when `Pass`.
pub(super) fn fold_case_slice_adjudications(
    canon_ref: &str,
    mut slice_decrees: Vec<CanonAdjudication>,
    threshold: f64,
) -> CanonAdjudication {
    debug_assert!(
        !slice_decrees.is_empty(),
        "fold_case_slice_adjudications requires at least one slice decree"
    );

    if slice_decrees.len() == 1 {
        return slice_decrees.remove(0);
    }

    let total_slices = slice_decrees.len();
    let raw_min = slice_decrees
        .iter()
        .map(|decree| decree.compliance_score)
        .fold(1.0_f64, f64::min);
    let compliance_score = if raw_min == 0.0 { 0.0 } else { raw_min };
    let status = VerdictStatus::from_score(compliance_score, threshold);

    match status {
        VerdictStatus::Pass => {
            let summaries: Vec<String> = slice_decrees
                .iter()
                .enumerate()
                .map(|(idx, decree)| {
                    format!(
                        "[Slice {}/{}] {}",
                        idx + 1,
                        total_slices,
                        decree.compliance_summary.trim()
                    )
                })
                .collect();
            CanonAdjudication {
                canon_path: canon_ref.to_string(),
                compliance_summary: format!(
                    "Compliant across {total_slices} trial slices (min score: {compliance_score:.2}): {}",
                    summaries.join(" | ")
                ),
                compliance_score,
                status: VerdictStatus::Pass,
                annotations: Vec::new(),
            }
        }
        VerdictStatus::Fail => {
            let mut annotations = Vec::new();
            let mut failing_summaries = Vec::new();
            for (idx, decree) in slice_decrees.into_iter().enumerate() {
                if decree.status == VerdictStatus::Fail || decree.compliance_score < threshold {
                    failing_summaries.push(format!(
                        "[Slice {}/{}] {}",
                        idx + 1,
                        total_slices,
                        decree.compliance_summary.trim()
                    ));
                    annotations.extend(decree.annotations);
                }
            }
            sort_annotations(&mut annotations);
            CanonAdjudication {
                canon_path: canon_ref.to_string(),
                compliance_summary: failing_summaries.join(" | "),
                compliance_score,
                status: VerdictStatus::Fail,
                annotations,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::{AnnotationLevel, CodeAnnotation};

    mod single_and_multi_slice_folding {
        use super::*;

        #[test]
        fn test_single_slice_passes_through_verbatim() {
            let original = CanonAdjudication {
                canon_path: ".canons/cli/rule.md".to_string(),
                compliance_summary: "All flags follow kebab-case.".to_string(),
                compliance_score: 0.94,
                status: VerdictStatus::Pass,
                annotations: Vec::new(),
            };
            let folded =
                fold_case_slice_adjudications(".canons/cli/rule.md", vec![original.clone()], 0.5);
            assert_eq!(folded, original);
        }

        #[test]
        fn test_multi_slice_pass_preserves_min_score_exception_notes_and_empty_annotations() {
            let folded = fold_case_slice_adjudications(
                ".canons/cli/rule.md",
                vec![
                    CanonAdjudication {
                        canon_path: ".canons/cli/rule.md".to_string(),
                        compliance_summary: "Flags in slice 1 comply.".to_string(),
                        compliance_score: 0.96,
                        status: VerdictStatus::Pass,
                        annotations: Vec::new(),
                    },
                    CanonAdjudication {
                        canon_path: ".canons/cli/rule.md".to_string(),
                        compliance_summary: "Excused under Legacy Alias Exception in slice 2."
                            .to_string(),
                        compliance_score: 0.84,
                        status: VerdictStatus::Pass,
                        annotations: Vec::new(),
                    },
                ],
                0.5,
            );
            assert_eq!(folded.status, VerdictStatus::Pass);
            assert!((folded.compliance_score - 0.84).abs() < 1e-9);
            assert!(folded.annotations.is_empty());
            assert!(folded.compliance_summary.contains("[Slice 1/2]"));
            assert!(folded.compliance_summary.contains("Legacy Alias Exception"));
        }

        #[test]
        fn test_multi_slice_fail_takes_min_score_unions_deduplicates_annotations_and_joins_failing_summaries()
         {
            let dup_ann = CodeAnnotation {
                path: "README.md".to_string(),
                start_line: 1,
                end_line: 1,
                start_column: None,
                end_column: None,
                annotation_level: AnnotationLevel::Failure,
                message: "Document newly added flag.".to_string(),
                title: Some("rule".to_string()),
            };
            let slice3_ann = CodeAnnotation {
                path: "src/a.rs".to_string(),
                start_line: 14,
                end_line: 14,
                start_column: Some(5),
                end_column: Some(12),
                annotation_level: AnnotationLevel::Failure,
                message: "Rename `--badFlag` to `--bad-flag`.".to_string(),
                title: Some("rule".to_string()),
            };
            let slice1_ann = CodeAnnotation {
                path: "src/z.rs".to_string(),
                start_line: 8,
                end_line: 9,
                start_column: None,
                end_column: None,
                annotation_level: AnnotationLevel::Failure,
                message: "Add remediation hint.".to_string(),
                title: Some("rule".to_string()),
            };

            let folded = fold_case_slice_adjudications(
                ".canons/cli/rule.md",
                vec![
                    CanonAdjudication {
                        canon_path: ".canons/cli/rule.md".to_string(),
                        compliance_summary: "Violation in z.rs; add Hint line.".to_string(),
                        compliance_score: 0.15,
                        status: VerdictStatus::Fail,
                        annotations: vec![slice1_ann.clone(), dup_ann.clone()],
                    },
                    CanonAdjudication {
                        canon_path: ".canons/cli/rule.md".to_string(),
                        compliance_summary: "Slice 2 clean.".to_string(),
                        compliance_score: 0.95,
                        status: VerdictStatus::Pass,
                        annotations: Vec::new(),
                    },
                    CanonAdjudication {
                        canon_path: ".canons/cli/rule.md".to_string(),
                        compliance_summary: "Violation in a.rs; use kebab-case.".to_string(),
                        compliance_score: 0.08,
                        status: VerdictStatus::Fail,
                        annotations: vec![slice3_ann.clone(), dup_ann.clone()],
                    },
                ],
                0.5,
            );

            assert_eq!(folded.status, VerdictStatus::Fail);
            assert!((folded.compliance_score - 0.08).abs() < 1e-9);
            assert_eq!(
                folded.compliance_summary,
                "[Slice 1/3] Violation in z.rs; add Hint line. | [Slice 3/3] Violation in a.rs; use kebab-case."
            );
            assert_eq!(folded.annotations, vec![dup_ann, slice3_ann, slice1_ann]);
        }
    }
}
