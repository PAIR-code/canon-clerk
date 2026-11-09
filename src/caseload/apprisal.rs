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

//! Data plane models for the `.apprisal` namespace (`docs/architecture/nodes/apprise.md` §3).

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

/// Deterministically derived statutory apprisal status (`docs/architecture/nodes/apprise.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprisalStatus {
    /// Canon claims prospective subject-matter jurisdiction (`apprisal_score >= threshold`).
    Applicable,
    /// Canon does not govern the prospective design intent and target scope (`apprisal_score < threshold`).
    Dismissed,
}

impl ApprisalStatus {
    /// Returns the canonical wire representation (`"applicable"` or `"dismissed"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Applicable => "applicable",
            Self::Dismissed => "dismissed",
        }
    }

    /// Derives [`ApprisalStatus`] deterministically from `score` against `threshold`.
    pub fn from_score(score: f64, threshold: f64) -> Self {
        if score >= threshold {
            Self::Applicable
        } else {
            Self::Dismissed
        }
    }
}

/// Validates that a heuristic score is finite and within `[0.0, 1.0]`, normalizing `-0.0` to `0.0`.
pub fn normalize_unit_score(score: f64) -> Result<f64, String> {
    if !score.is_finite() {
        return Err("apprisal_score must be a finite number in [0.0, 1.0]".to_string());
    }
    if !(0.0..=1.0).contains(&score) {
        return Err(format!(
            "apprisal_score must be within [0.0, 1.0], got {score}"
        ));
    }
    Ok(if score == 0.0 { 0.0 } else { score })
}

fn serialize_unit_score<S>(score: &f64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let normalized = normalize_unit_score(*score).map_err(serde::ser::Error::custom)?;
    serializer.serialize_f64(normalized)
}

fn deserialize_unit_score<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = f64::deserialize(deserializer)?;
    normalize_unit_score(raw).map_err(serde::de::Error::custom)
}

fn canonical_score_bits(score: f64) -> u64 {
    if score == 0.0 {
        0.0_f64.to_bits()
    } else {
        score.to_bits()
    }
}

/// Prospective statutory jurisdiction assessment for a single candidate canon (`docs/architecture/nodes/apprise.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CanonApprisalAssessment {
    /// Rationale explaining why the canon does or does not govern the prospective design intent and target scope.
    pub apprisal_summary: String,
    /// Continuous prospective applicability score in `[0.0, 1.0]`.
    #[serde(
        serialize_with = "serialize_unit_score",
        deserialize_with = "deserialize_unit_score"
    )]
    pub apprisal_score: f64,
    /// Deterministically derived statutory apprisal status (`"applicable"` or `"dismissed"`).
    pub apprisal_status: ApprisalStatus,
}

impl PartialEq for CanonApprisalAssessment {
    fn eq(&self, other: &Self) -> bool {
        self.apprisal_summary == other.apprisal_summary
            && canonical_score_bits(self.apprisal_score)
                == canonical_score_bits(other.apprisal_score)
            && self.apprisal_status == other.apprisal_status
    }
}

impl Eq for CanonApprisalAssessment {}

/// Prospective statutory notice assessments captured during the `apprise` stage (`docs/architecture/nodes/apprise.md` §3).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct CaseloadApprisal {
    /// Deterministic per-canon apprisal assessments keyed by candidate canon identifier.
    #[serde(default)]
    pub assessments: BTreeMap<String, CanonApprisalAssessment>,
}

impl CaseloadApprisal {
    /// Returns the count of candidate canons marked [`ApprisalStatus::Applicable`].
    pub fn applicable_count(&self) -> usize {
        self.assessments
            .values()
            .filter(|a| a.apprisal_status == ApprisalStatus::Applicable)
            .count()
    }

    /// Returns the count of candidate canons marked [`ApprisalStatus::Dismissed`].
    pub fn dismissed_count(&self) -> usize {
        self.assessments
            .values()
            .filter(|a| a.apprisal_status == ApprisalStatus::Dismissed)
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
        fn test_caseload_apprisal_round_trip_and_lexicographical_key_order() {
            let mut assessments = BTreeMap::new();
            assessments.insert(
                ".canons/z-rule.md".to_string(),
                CanonApprisalAssessment {
                    apprisal_summary: "Unrelated to database migrations.".to_string(),
                    apprisal_score: 0.05,
                    apprisal_status: ApprisalStatus::Dismissed,
                },
            );
            assessments.insert(
                ".canons/a-rule.md".to_string(),
                CanonApprisalAssessment {
                    apprisal_summary: "Governs provider cache expiration.".to_string(),
                    apprisal_score: 0.92,
                    apprisal_status: ApprisalStatus::Applicable,
                },
            );

            let apprisal = CaseloadApprisal { assessments };
            assert_eq!(apprisal.applicable_count(), 1);
            assert_eq!(apprisal.dismissed_count(), 1);
            assert_eq!(ApprisalStatus::Applicable.as_str(), "applicable");
            assert_eq!(ApprisalStatus::Dismissed.as_str(), "dismissed");

            let caseload = Caseload::new().with_apprisal(apprisal);
            let encoded = serde_json::to_string(&caseload).expect("serialize apprisal caseload");

            let a_pos = encoded.find(".canons/a-rule.md").expect("a-rule present");
            let z_pos = encoded.find(".canons/z-rule.md").expect("z-rule present");
            assert!(a_pos < z_pos, "assessments must serialize in sorted order");

            let summary_pos = encoded
                .find("\"apprisal_summary\"")
                .expect("summary field present");
            let score_pos = encoded
                .find("\"apprisal_score\"")
                .expect("score field present");
            let status_pos = encoded
                .find("\"apprisal_status\"")
                .expect("status field present");
            assert!(
                summary_pos < score_pos && score_pos < status_pos,
                "reason-first field order preserved in serialized struct"
            );

            let decoded: Caseload =
                serde_json::from_str(&encoded).expect("deserialize apprisal caseload");
            assert_eq!(decoded, caseload);

            let reencoded = serde_json::to_string(&decoded).expect("re-serialize caseload");
            assert_eq!(encoded, reencoded);
        }

        #[test]
        fn test_negative_zero_normalizes_deterministically() {
            let neg_zero = CanonApprisalAssessment {
                apprisal_summary: "Zero applicability.".to_string(),
                apprisal_score: -0.0,
                apprisal_status: ApprisalStatus::Dismissed,
            };
            let pos_zero = CanonApprisalAssessment {
                apprisal_summary: "Zero applicability.".to_string(),
                apprisal_score: 0.0,
                apprisal_status: ApprisalStatus::Dismissed,
            };
            assert_eq!(neg_zero, pos_zero);

            let json = serde_json::to_string(&neg_zero).expect("serialize -0.0");
            assert!(json.contains("\"apprisal_score\":0.0"));
            assert!(!json.contains("-0.0"));
        }
    }

    mod score_validation {
        use super::*;

        #[test]
        fn test_rejects_non_finite_and_out_of_bounds_scores() {
            assert!(normalize_unit_score(f64::NAN).is_err());
            assert!(normalize_unit_score(f64::INFINITY).is_err());
            assert!(normalize_unit_score(f64::NEG_INFINITY).is_err());
            assert!(normalize_unit_score(-0.01).is_err());
            assert!(normalize_unit_score(1.01).is_err());
            assert_eq!(normalize_unit_score(0.0).unwrap(), 0.0);
            assert_eq!(normalize_unit_score(0.5).unwrap(), 0.5);
            assert_eq!(normalize_unit_score(1.0).unwrap(), 1.0);

            let out_of_bounds_json = r#"{
                "apprisal_summary": "Invalid score.",
                "apprisal_score": 1.5,
                "apprisal_status": "applicable"
            }"#;
            let err = serde_json::from_str::<CanonApprisalAssessment>(out_of_bounds_json)
                .expect_err("score > 1.0 must fail deserialization");
            assert!(err.to_string().contains("[0.0, 1.0]"));

            let non_finite = CanonApprisalAssessment {
                apprisal_summary: "NaN score".to_string(),
                apprisal_score: f64::NAN,
                apprisal_status: ApprisalStatus::Dismissed,
            };
            assert!(serde_json::to_string(&non_finite).is_err());
        }
    }
}
