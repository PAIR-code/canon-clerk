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

//! Data plane models for the `.vetting` namespace (`docs/architecture/nodes/vet.md` §3).

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

/// Deterministically derived per-case prima facie compliance status (`docs/architecture/nodes/vet.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum VettingStatus {
    /// Active case passed high-confidence prima facie compliance vetting (`vetting_score >= threshold`).
    Vetted,
    /// Active case requires full judicial adjudication in `audit` (`vetting_score < threshold`).
    Contested,
}

impl VettingStatus {
    /// Returns the canonical wire representation (`"vetted"` or `"contested"`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vetted => "vetted",
            Self::Contested => "contested",
        }
    }

    /// Derives [`VettingStatus`] deterministically from `score` against `threshold`.
    pub fn from_score(score: f64, threshold: f64) -> Self {
        if score >= threshold {
            Self::Vetted
        } else {
            Self::Contested
        }
    }
}

/// Validates that a vetting score or threshold is finite and within `[0.0, 1.0]`, normalizing `-0.0` to `0.0`.
pub fn normalize_vetting_score(score: f64) -> Result<f64, String> {
    if !score.is_finite() {
        return Err("vetting_score must be a finite number in [0.0, 1.0]".to_string());
    }
    if !(0.0..=1.0).contains(&score) {
        return Err(format!(
            "vetting_score must be within [0.0, 1.0], got {score}"
        ));
    }
    Ok(if score == 0.0 { 0.0 } else { score })
}

fn serialize_vetting_score<S>(score: &f64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let normalized = normalize_vetting_score(*score).map_err(serde::ser::Error::custom)?;
    serializer.serialize_f64(normalized)
}

fn deserialize_vetting_score<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = f64::deserialize(deserializer)?;
    normalize_vetting_score(raw).map_err(serde::de::Error::custom)
}

fn deserialize_sorted_string_list<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut items = Vec::<String>::deserialize(deserializer)?;
    items.sort();
    items.dedup();
    Ok(items)
}

fn serialize_sorted_string_list<S>(items: &[String], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut sorted = items.to_vec();
    sorted.sort();
    sorted.dedup();
    sorted.serialize(serializer)
}

fn canonical_score_bits(score: f64) -> u64 {
    if score == 0.0 {
        0.0_f64.to_bits()
    } else {
        score.to_bits()
    }
}

/// Localized contested line span within an admitted file exhibit (`docs/architecture/nodes/vet.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ContestedSpan {
    /// Repository-relative POSIX path of the admitted file exhibit.
    pub path: String,
    /// Inclusive 1-based post-image start line in the target file (`>= 1`).
    pub start_line: usize,
    /// Inclusive 1-based post-image end line in the target file (`>= start_line`).
    pub end_line: usize,
    /// Continuous prima facie compliance score in `[0.0, 1.0]` for this span.
    #[serde(
        serialize_with = "serialize_vetting_score",
        deserialize_with = "deserialize_vetting_score"
    )]
    pub score: f64,
}

impl ContestedSpan {
    /// Normalizes line bounds (`start_line >= 1`, `end_line >= start_line`) and `-0.0` score.
    pub fn canonicalized(mut self) -> Self {
        self.start_line = self.start_line.max(1);
        self.end_line = self.end_line.max(self.start_line);
        if self.score == 0.0 {
            self.score = 0.0;
        }
        self
    }
}

impl PartialEq for ContestedSpan {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
            && self.start_line == other.start_line
            && self.end_line == other.end_line
            && canonical_score_bits(self.score) == canonical_score_bits(other.score)
    }
}

impl Eq for ContestedSpan {}

/// Deterministically normalizes, sorts by `(path, start_line, end_line)`, and deduplicates
/// contested spans (retaining the lowest score when two spans share identical coordinates).
pub fn canonicalize_contested_spans(spans: &mut Vec<ContestedSpan>) {
    for span in spans.iter_mut() {
        span.start_line = span.start_line.max(1);
        span.end_line = span.end_line.max(span.start_line);
        if span.score == 0.0 {
            span.score = 0.0;
        }
    }
    spans.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.start_line.cmp(&b.start_line))
            .then_with(|| a.end_line.cmp(&b.end_line))
            .then_with(|| a.score.total_cmp(&b.score))
    });
    spans.dedup_by(|later, earlier| {
        later.path == earlier.path
            && later.start_line == earlier.start_line
            && later.end_line == earlier.end_line
    });
}

fn deserialize_sorted_contested_spans<'de, D>(
    deserializer: D,
) -> Result<Vec<ContestedSpan>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut spans = Vec::<ContestedSpan>::deserialize(deserializer)?;
    canonicalize_contested_spans(&mut spans);
    Ok(spans)
}

fn serialize_sorted_contested_spans<S>(
    spans: &[ContestedSpan],
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut sorted = spans.to_vec();
    canonicalize_contested_spans(&mut sorted);
    sorted.serialize(serializer)
}

/// Prima facie compliance vetting assessment for a single active case (`docs/architecture/nodes/vet.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CanonVettingAssessment {
    /// Rationale or discriminative model score summary justifying the prima facie compliance posture.
    pub vetting_summary: String,
    /// Continuous prima facie compliance score in `[0.0, 1.0]`.
    #[serde(
        serialize_with = "serialize_vetting_score",
        deserialize_with = "deserialize_vetting_score"
    )]
    pub vetting_score: f64,
    /// Deterministically derived prima facie compliance status (`"vetted"` or `"contested"`).
    pub vetting_status: VettingStatus,
    /// Stably sorted contested file exhibit paths when discriminative vetting cleared a proper subset of
    /// exhibits (`vetting_status == Contested`); omitted when `"vetted"` or when all sliceable file exhibits are contested.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "serialize_sorted_string_list",
        deserialize_with = "deserialize_sorted_string_list"
    )]
    pub contested_exhibits: Vec<String>,
    /// Deterministically sorted localized contested spans (`vetting_status == Contested`),
    /// ordered by `(path, start_line, end_line)` and omitted when `"vetted"` or empty.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "serialize_sorted_contested_spans",
        deserialize_with = "deserialize_sorted_contested_spans"
    )]
    pub contested_spans: Vec<ContestedSpan>,
}

impl PartialEq for CanonVettingAssessment {
    fn eq(&self, other: &Self) -> bool {
        self.vetting_summary == other.vetting_summary
            && canonical_score_bits(self.vetting_score) == canonical_score_bits(other.vetting_score)
            && self.vetting_status == other.vetting_status
            && self.contested_exhibits == other.contested_exhibits
            && self.contested_spans == other.contested_spans
    }
}

impl Eq for CanonVettingAssessment {}

/// Prima facie compliance vetting assessments and case partition captured during `vet` (`docs/architecture/nodes/vet.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct CaseloadVetting {
    /// Deterministic per-case vetting assessments keyed by active canon identifier (`canon_path`).
    #[serde(default)]
    pub cases: BTreeMap<String, CanonVettingAssessment>,
    /// Stably sorted lexicographical list of canon identifiers whose `vetting_status` is `"vetted"`.
    #[serde(default, deserialize_with = "deserialize_sorted_string_list")]
    pub vetted_cases: Vec<String>,
    /// Stably sorted lexicographical list of canon identifiers whose `vetting_status` is `"contested"`.
    #[serde(default, deserialize_with = "deserialize_sorted_string_list")]
    pub contested_cases: Vec<String>,
}

impl CaseloadVetting {
    /// Constructs a [`CaseloadVetting`] from per-case `cases`, canonicalizing any slice-scoped
    /// `contested_exhibits` and deriving `vetted_cases` and `contested_cases` in deterministic lexicographical order.
    pub fn from_cases(mut cases: BTreeMap<String, CanonVettingAssessment>) -> Self {
        let mut vetted_cases = Vec::new();
        let mut contested_cases = Vec::new();
        for (canon_ref, assessment) in &mut cases {
            match assessment.vetting_status {
                VettingStatus::Vetted => {
                    assessment.contested_exhibits.clear();
                    assessment.contested_spans.clear();
                    vetted_cases.push(canon_ref.clone());
                }
                VettingStatus::Contested => {
                    assessment.contested_exhibits.sort();
                    assessment.contested_exhibits.dedup();
                    canonicalize_contested_spans(&mut assessment.contested_spans);
                    contested_cases.push(canon_ref.clone());
                }
            }
        }
        Self {
            cases,
            vetted_cases,
            contested_cases,
        }
    }

    /// Returns `true` when zero active cases are contested (`contested_cases.is_empty()`).
    pub fn is_all_vetted(&self) -> bool {
        self.contested_cases.is_empty()
    }

    /// Returns the count of active cases marked [`VettingStatus::Vetted`].
    pub fn vetted_count(&self) -> usize {
        self.cases
            .values()
            .filter(|a| a.vetting_status == VettingStatus::Vetted)
            .count()
    }

    /// Returns the count of active cases marked [`VettingStatus::Contested`].
    pub fn contested_count(&self) -> usize {
        self.cases
            .values()
            .filter(|a| a.vetting_status == VettingStatus::Contested)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caseload::Caseload;

    mod serialization_and_determinism {
        use super::*;

        #[test]
        fn test_caseload_vetting_round_trip_and_lexicographical_partitioning() {
            let mut cases = BTreeMap::new();
            cases.insert(
                ".canons/z-rule.md".to_string(),
                CanonVettingAssessment {
                    vetting_summary: "Compliant via discriminative vetting (`jev`: 0.940 >= 0.80)."
                        .to_string(),
                    vetting_score: 0.94,
                    vetting_status: VettingStatus::Vetted,
                    contested_exhibits: Vec::new(),
                    contested_spans: Vec::new(),
                },
            );
            cases.insert(
                ".canons/m-rule.md".to_string(),
                CanonVettingAssessment {
                    vetting_summary:
                        "Contested via discriminative vetting (`jev`: 0.420 < 0.80); requires audit."
                            .to_string(),
                    vetting_score: 0.42,
                    vetting_status: VettingStatus::Contested,
                    contested_exhibits: Vec::new(),
                    contested_spans: Vec::new(),
                },
            );
            cases.insert(
                ".canons/a-rule.md".to_string(),
                CanonVettingAssessment {
                    vetting_summary: "All flags follow kebab-case.".to_string(),
                    vetting_score: 0.98,
                    vetting_status: VettingStatus::Vetted,
                    contested_exhibits: Vec::new(),
                    contested_spans: Vec::new(),
                },
            );

            let vetting = CaseloadVetting::from_cases(cases);
            assert!(!vetting.is_all_vetted());
            assert_eq!(vetting.vetted_count(), 2);
            assert_eq!(vetting.contested_count(), 1);
            assert_eq!(
                vetting.vetted_cases,
                vec![
                    ".canons/a-rule.md".to_string(),
                    ".canons/z-rule.md".to_string()
                ]
            );
            assert_eq!(
                vetting.contested_cases,
                vec![".canons/m-rule.md".to_string()]
            );
            assert_eq!(VettingStatus::Vetted.as_str(), "vetted");
            assert_eq!(VettingStatus::Contested.as_str(), "contested");

            let caseload = Caseload::new().with_vetting(vetting);
            let encoded = serde_json::to_string(&caseload).expect("serialize vetting caseload");

            let a_pos = encoded.find(".canons/a-rule.md").expect("a-rule");
            let m_pos = encoded.find(".canons/m-rule.md").expect("m-rule");
            let z_pos = encoded.find(".canons/z-rule.md").expect("z-rule");
            assert!(a_pos < m_pos && m_pos < z_pos);

            let summary_pos = encoded.find("\"vetting_summary\"").expect("summary");
            let score_pos = encoded.find("\"vetting_score\"").expect("score");
            let status_pos = encoded.find("\"vetting_status\"").expect("status");
            assert!(summary_pos < score_pos && score_pos < status_pos);

            let decoded: Caseload =
                serde_json::from_str(&encoded).expect("deserialize vetting caseload");
            assert_eq!(decoded, caseload);
            assert_eq!(
                serde_json::to_string(&decoded).expect("re-serialize"),
                encoded
            );
        }

        #[test]
        fn test_unsorted_lists_and_negative_zero_normalize_deterministically() {
            let raw = r#"{
                "cases": {},
                "vetted_cases": [".canons/z.md", ".canons/a.md", ".canons/z.md"],
                "contested_cases": [".canons/y.md", ".canons/b.md"]
            }"#;
            let decoded: CaseloadVetting = serde_json::from_str(raw).expect("decode vetting");
            assert_eq!(
                decoded.vetted_cases,
                vec![".canons/a.md".to_string(), ".canons/z.md".to_string()]
            );
            assert_eq!(
                decoded.contested_cases,
                vec![".canons/b.md".to_string(), ".canons/y.md".to_string()]
            );

            let neg_zero = CanonVettingAssessment {
                vetting_summary: "Contested.".to_string(),
                vetting_score: -0.0,
                vetting_status: VettingStatus::Contested,
                contested_exhibits: Vec::new(),
                contested_spans: Vec::new(),
            };
            let pos_zero = CanonVettingAssessment {
                vetting_summary: "Contested.".to_string(),
                vetting_score: 0.0,
                vetting_status: VettingStatus::Contested,
                contested_exhibits: Vec::new(),
                contested_spans: Vec::new(),
            };
            assert_eq!(neg_zero, pos_zero);
            let json = serde_json::to_string(&neg_zero).expect("serialize -0.0");
            assert!(!json.contains("-0.0"));
            assert!(json.contains("\"vetting_score\":0.0"));
            assert!(!json.contains("contested_exhibits"));
            assert!(!json.contains("contested_spans"));
        }

        #[test]
        fn test_contested_exhibits_omits_when_vetted_and_canonicalizes_partial_slice_paths() {
            let mut cases = BTreeMap::new();
            cases.insert(
                ".canons/vetted.md".to_string(),
                CanonVettingAssessment {
                    vetting_summary: "All slices cleared.".to_string(),
                    vetting_score: 0.91,
                    vetting_status: VettingStatus::Vetted,
                    contested_exhibits: vec!["src/should_be_cleared.rs".to_string()],
                    contested_spans: vec![ContestedSpan {
                        path: "src/should_be_cleared.rs".to_string(),
                        start_line: 10,
                        end_line: 55,
                        score: 0.2,
                    }],
                },
            );
            cases.insert(
                ".canons/partial.md".to_string(),
                CanonVettingAssessment {
                    vetting_summary: "1 of 3 slices contested.".to_string(),
                    vetting_score: 0.34,
                    vetting_status: VettingStatus::Contested,
                    contested_exhibits: vec![
                        "src/z_mod.rs".to_string(),
                        "src/a_mod.rs".to_string(),
                        "src/z_mod.rs".to_string(),
                    ],
                    contested_spans: vec![
                        ContestedSpan {
                            path: "src/z_mod.rs".to_string(),
                            start_line: 90,
                            end_line: 140,
                            score: 0.34,
                        },
                        ContestedSpan {
                            path: "src/a_mod.rs".to_string(),
                            start_line: 0,
                            end_line: 0,
                            score: -0.0,
                        },
                        ContestedSpan {
                            path: "src/z_mod.rs".to_string(),
                            start_line: 90,
                            end_line: 140,
                            score: 0.18,
                        },
                    ],
                },
            );

            let vetting = CaseloadVetting::from_cases(cases);
            assert!(
                vetting.cases[".canons/vetted.md"]
                    .contested_exhibits
                    .is_empty()
            );
            assert!(
                vetting.cases[".canons/vetted.md"]
                    .contested_spans
                    .is_empty()
            );
            assert_eq!(
                vetting.cases[".canons/partial.md"].contested_exhibits,
                vec!["src/a_mod.rs".to_string(), "src/z_mod.rs".to_string()]
            );
            assert_eq!(
                vetting.cases[".canons/partial.md"].contested_spans,
                vec![
                    ContestedSpan {
                        path: "src/a_mod.rs".to_string(),
                        start_line: 1,
                        end_line: 1,
                        score: 0.0,
                    },
                    ContestedSpan {
                        path: "src/z_mod.rs".to_string(),
                        start_line: 90,
                        end_line: 140,
                        score: 0.18,
                    },
                ]
            );

            let encoded = serde_json::to_string(&vetting).expect("serialize partial vetting");
            assert_eq!(encoded.matches("\"contested_exhibits\"").count(), 1);
            assert_eq!(encoded.matches("\"contested_spans\"").count(), 1);
            assert!(encoded.contains(r#""contested_exhibits":["src/a_mod.rs","src/z_mod.rs"]"#));

            let decoded: CaseloadVetting =
                serde_json::from_str(&encoded).expect("round-trip partial vetting");
            assert_eq!(decoded, vetting);
        }
    }

    mod score_bounds_and_status {
        use super::*;

        #[test]
        fn test_rejects_non_finite_and_out_of_bounds_scores() {
            assert!(normalize_vetting_score(f64::NAN).is_err());
            assert!(normalize_vetting_score(f64::INFINITY).is_err());
            assert!(normalize_vetting_score(-0.01).is_err());
            assert!(normalize_vetting_score(1.01).is_err());
            assert_eq!(VettingStatus::from_score(0.80, 0.80), VettingStatus::Vetted);
            assert_eq!(
                VettingStatus::from_score(0.799, 0.80),
                VettingStatus::Contested
            );
        }
    }
}
