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

//! Data plane models for the `.docket` namespace (`docs/architecture/nodes/docket.md` §3).

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

/// Deterministically derived macro subject-matter jurisdiction status (`docs/architecture/nodes/docket.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColorabilityStatus {
    /// Canon establishes a colorable claim of subject-matter jurisdiction (`colorability_score >= threshold`).
    Docketed,
    /// Canon does not claim subject-matter jurisdiction over the filing (`colorability_score < threshold`).
    Dismissed,
}

impl ColorabilityStatus {
    /// Returns the canonical wire representation (`"docketed"` or `"dismissed"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Docketed => "docketed",
            Self::Dismissed => "dismissed",
        }
    }

    /// Derives [`ColorabilityStatus`] deterministically from `score` against `threshold`.
    pub fn from_score(score: f64, threshold: f64) -> Self {
        if score >= threshold {
            Self::Docketed
        } else {
            Self::Dismissed
        }
    }
}

/// Validates that a colorability score or threshold is finite and within `[0.0, 1.0]`, normalizing `-0.0` to `0.0`.
pub fn normalize_colorability_score(score: f64) -> Result<f64, String> {
    if !score.is_finite() {
        return Err("colorability_score must be a finite number in [0.0, 1.0]".to_string());
    }
    if !(0.0..=1.0).contains(&score) {
        return Err(format!(
            "colorability_score must be within [0.0, 1.0], got {score}"
        ));
    }
    Ok(if score == 0.0 { 0.0 } else { score })
}

fn serialize_colorability_score<S>(score: &f64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let normalized = normalize_colorability_score(*score).map_err(serde::ser::Error::custom)?;
    serializer.serialize_f64(normalized)
}

fn deserialize_colorability_score<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = f64::deserialize(deserializer)?;
    normalize_colorability_score(raw).map_err(serde::de::Error::custom)
}

fn deserialize_sorted_active_docket<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut items = Vec::<String>::deserialize(deserializer)?;
    items.sort();
    items.dedup();
    Ok(items)
}

fn canonical_score_bits(score: f64) -> u64 {
    if score == 0.0 {
        0.0_f64.to_bits()
    } else {
        score.to_bits()
    }
}

/// Macro subject-matter jurisdiction assessment for a single candidate canon (`docs/architecture/nodes/docket.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CanonDocketAssessment {
    /// Rationale justifying whether subject-matter jurisdiction applies to the filing.
    pub colorability_summary: String,
    /// Continuous subject-matter jurisdiction score in `[0.0, 1.0]`.
    #[serde(
        serialize_with = "serialize_colorability_score",
        deserialize_with = "deserialize_colorability_score"
    )]
    pub colorability_score: f64,
    /// Deterministically derived jurisdiction status (`"docketed"` or `"dismissed"`).
    pub colorability_status: ColorabilityStatus,
}

impl PartialEq for CanonDocketAssessment {
    fn eq(&self, other: &Self) -> bool {
        self.colorability_summary == other.colorability_summary
            && canonical_score_bits(self.colorability_score)
                == canonical_score_bits(other.colorability_score)
            && self.colorability_status == other.colorability_status
    }
}

impl Eq for CanonDocketAssessment {}

/// Macro subject-matter jurisdiction assessments and active cases captured during the `docket` stage (`docs/architecture/nodes/docket.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct CaseloadDocket {
    /// Deterministic per-canon colorability assessments keyed by candidate canon identifier.
    #[serde(default)]
    pub cases: BTreeMap<String, CanonDocketAssessment>,
    /// Stably sorted lexicographical list of canon identifiers whose `colorability_status` is `"docketed"`.
    #[serde(default, deserialize_with = "deserialize_sorted_active_docket")]
    pub active_docket: Vec<String>,
    /// Optional diagnostic manifest recording non-fatal screening anomalies encountered during macro triage.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anomalies: Vec<String>,
}

impl CaseloadDocket {
    /// Constructs a [`CaseloadDocket`] from per-canon `cases`, deriving `active_docket` in deterministic lexicographical order.
    pub fn from_cases(cases: BTreeMap<String, CanonDocketAssessment>) -> Self {
        let active_docket = cases
            .iter()
            .filter(|(_, assessment)| {
                assessment.colorability_status == ColorabilityStatus::Docketed
            })
            .map(|(canon_ref, _)| canon_ref.clone())
            .collect();
        Self {
            cases,
            active_docket,
            anomalies: Vec::new(),
        }
    }

    /// Returns the count of candidate canons marked [`ColorabilityStatus::Docketed`].
    pub fn docketed_count(&self) -> usize {
        self.cases
            .values()
            .filter(|a| a.colorability_status == ColorabilityStatus::Docketed)
            .count()
    }

    /// Returns the count of candidate canons marked [`ColorabilityStatus::Dismissed`].
    pub fn dismissed_count(&self) -> usize {
        self.cases
            .values()
            .filter(|a| a.colorability_status == ColorabilityStatus::Dismissed)
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
        fn test_caseload_docket_round_trip_sorted_active_docket_and_anomaly_omission() {
            let mut cases = BTreeMap::new();
            cases.insert(
                ".canons/z-rule.md".to_string(),
                CanonDocketAssessment {
                    colorability_summary: "PR modifies z-subsystem flags.".to_string(),
                    colorability_score: 0.85,
                    colorability_status: ColorabilityStatus::Docketed,
                },
            );
            cases.insert(
                ".canons/m-rule.md".to_string(),
                CanonDocketAssessment {
                    colorability_summary: "Unrelated to database schema migrations.".to_string(),
                    colorability_score: 0.08,
                    colorability_status: ColorabilityStatus::Dismissed,
                },
            );
            cases.insert(
                ".canons/a-rule.md".to_string(),
                CanonDocketAssessment {
                    colorability_summary: "PR introduces new CLI flags.".to_string(),
                    colorability_score: 0.96,
                    colorability_status: ColorabilityStatus::Docketed,
                },
            );

            let docket = CaseloadDocket::from_cases(cases);
            assert_eq!(docket.docketed_count(), 2);
            assert_eq!(docket.dismissed_count(), 1);
            assert_eq!(
                docket.active_docket,
                vec![
                    ".canons/a-rule.md".to_string(),
                    ".canons/z-rule.md".to_string()
                ]
            );
            assert_eq!(ColorabilityStatus::Docketed.as_str(), "docketed");
            assert_eq!(ColorabilityStatus::Dismissed.as_str(), "dismissed");

            let caseload = Caseload::new().with_docket(docket);
            let encoded = serde_json::to_string(&caseload).expect("serialize docket caseload");
            assert!(
                !encoded.contains("\"anomalies\""),
                "empty anomalies vec must be omitted on wire"
            );

            let a_pos = encoded.find(".canons/a-rule.md").expect("a-rule present");
            let m_pos = encoded.find(".canons/m-rule.md").expect("m-rule present");
            let z_pos = encoded.find(".canons/z-rule.md").expect("z-rule present");
            assert!(
                a_pos < m_pos && m_pos < z_pos,
                "cases must serialize in deterministic lexicographical order"
            );

            let summary_pos = encoded
                .find("\"colorability_summary\"")
                .expect("summary field present");
            let score_pos = encoded
                .find("\"colorability_score\"")
                .expect("score field present");
            let status_pos = encoded
                .find("\"colorability_status\"")
                .expect("status field present");
            assert!(
                summary_pos < score_pos && score_pos < status_pos,
                "reason-first field order preserved in serialized struct"
            );

            let decoded: Caseload =
                serde_json::from_str(&encoded).expect("deserialize docket caseload");
            assert_eq!(decoded, caseload);

            let reencoded = serde_json::to_string(&decoded).expect("re-serialize caseload");
            assert_eq!(encoded, reencoded);
        }

        #[test]
        fn test_anomalies_round_trip_and_unsorted_active_docket_normalization() {
            let raw_json = r#"{
                "cases": {},
                "active_docket": [".canons/z.md", ".canons/a.md", ".canons/z.md"],
                "anomalies": ["Non-fatal metadata truncation notice"]
            }"#;
            let decoded: CaseloadDocket =
                serde_json::from_str(raw_json).expect("deserialize docket with anomalies");
            assert_eq!(
                decoded.active_docket,
                vec![".canons/a.md".to_string(), ".canons/z.md".to_string()]
            );
            assert_eq!(
                decoded.anomalies,
                vec!["Non-fatal metadata truncation notice".to_string()]
            );

            let encoded = serde_json::to_string(&decoded).expect("serialize non-empty anomalies");
            assert!(encoded.contains("\"anomalies\":[\"Non-fatal metadata truncation notice\"]"));
        }

        #[test]
        fn test_negative_zero_normalizes_deterministically() {
            let neg_zero = CanonDocketAssessment {
                colorability_summary: "Zero colorability.".to_string(),
                colorability_score: -0.0,
                colorability_status: ColorabilityStatus::Dismissed,
            };
            let pos_zero = CanonDocketAssessment {
                colorability_summary: "Zero colorability.".to_string(),
                colorability_score: 0.0,
                colorability_status: ColorabilityStatus::Dismissed,
            };
            assert_eq!(neg_zero, pos_zero);

            let json = serde_json::to_string(&neg_zero).expect("serialize -0.0");
            assert!(json.contains("\"colorability_score\":0.0"));
            assert!(!json.contains("-0.0"));
        }
    }

    mod score_validation {
        use super::*;

        #[test]
        fn test_rejects_non_finite_and_out_of_bounds_scores() {
            assert!(normalize_colorability_score(f64::NAN).is_err());
            assert!(normalize_colorability_score(f64::INFINITY).is_err());
            assert!(normalize_colorability_score(f64::NEG_INFINITY).is_err());
            assert!(normalize_colorability_score(-0.01).is_err());
            assert!(normalize_colorability_score(1.01).is_err());
            assert_eq!(normalize_colorability_score(0.0).expect("0.0"), 0.0);
            assert_eq!(normalize_colorability_score(0.5).expect("0.5"), 0.5);
            assert_eq!(normalize_colorability_score(1.0).expect("1.0"), 1.0);
            assert_eq!(
                ColorabilityStatus::from_score(0.5, 0.5),
                ColorabilityStatus::Docketed
            );
            assert_eq!(
                ColorabilityStatus::from_score(0.49, 0.5),
                ColorabilityStatus::Dismissed
            );

            let out_of_bounds_json = r#"{
                "colorability_summary": "Invalid score.",
                "colorability_score": 1.25,
                "colorability_status": "docketed"
            }"#;
            let err = serde_json::from_str::<CanonDocketAssessment>(out_of_bounds_json)
                .expect_err("score > 1.0 must fail deserialization");
            assert!(err.to_string().contains("[0.0, 1.0]"));

            let non_finite = CanonDocketAssessment {
                colorability_summary: "NaN score".to_string(),
                colorability_score: f64::NAN,
                colorability_status: ColorabilityStatus::Dismissed,
            };
            assert!(serde_json::to_string(&non_finite).is_err());
        }
    }
}
