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

//! Structured JSON response decoder and completeness/uniqueness validators for `admit`.

use super::AdmitError;
use crate::caseload::{AdmissibilityStatus, ExhibitAssessment, normalize_admissibility_score};
use crate::provider::ProviderError;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn schema_decode_error(model_specifier: &str, message: impl Into<String>) -> AdmitError {
    AdmitError::Provider(ProviderError::SchemaDecode {
        model_specifier: model_specifier.to_string(),
        message: message.into(),
    })
}

pub(super) fn decode_case_exhibits(
    payload: &Value,
    canon_ref: &str,
    candidate_exhibits: &[String],
    threshold: f64,
    model_specifier: &str,
) -> Result<Vec<ExhibitAssessment>, AdmitError> {
    let exhibits_arr = payload
        .get("exhibits")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            schema_decode_error(
                model_specifier,
                format!(
                    "missing array field `exhibits` in structured admit response for `{canon_ref}`"
                ),
            )
        })?;

    let expected: BTreeSet<&str> = candidate_exhibits.iter().map(String::as_str).collect();
    let same_len = exhibits_arr.len() == candidate_exhibits.len();
    let mut seen: BTreeMap<String, ExhibitAssessment> = BTreeMap::new();

    for (index, raw_item) in exhibits_arr.iter().enumerate() {
        let pos_expected = same_len
            .then(|| candidate_exhibits.get(index).map(String::as_str))
            .flatten();
        let assessment = decode_single_exhibit(
            raw_item,
            index,
            canon_ref,
            &expected,
            pos_expected,
            threshold,
            model_specifier,
        )?;
        if seen
            .insert(assessment.file_path.clone(), assessment)
            .is_some()
        {
            let dup = &exhibits_arr[index]["file_path"];
            return Err(schema_decode_error(
                model_specifier,
                format!(
                    "duplicate `file_path` {dup} in structured admit response for `{canon_ref}`"
                ),
            ));
        }
    }

    let missing: Vec<&str> = candidate_exhibits
        .iter()
        .map(String::as_str)
        .filter(|exhibit_id| !seen.contains_key(*exhibit_id))
        .collect();
    if !missing.is_empty() {
        return Err(schema_decode_error(
            model_specifier,
            format!(
                "missing exhibit assessment(s) for `{canon_ref}`: {}",
                missing.join(", ")
            ),
        ));
    }

    Ok(seen.into_values().collect())
}

fn resolve_exhibit_id<'a>(
    raw_id: &'a str,
    expected: &BTreeSet<&'a str>,
    pos: Option<&'a str>,
) -> Option<&'a str> {
    if let Some(&exact) = expected.get(raw_id) {
        return Some(exact);
    }
    let pos = pos?;
    if raw_id.rsplit('/').next() == pos.rsplit('/').next() {
        return Some(pos);
    }
    let pref = raw_id
        .bytes()
        .zip(pos.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let suff = raw_id
        .bytes()
        .rev()
        .zip(pos.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    ((pref + suff) * 5 >= raw_id.len().max(pos.len()).max(1) * 4).then_some(pos)
}

#[allow(
    clippy::too_many_arguments,
    reason = "Internal decoder binding raw JSON entry, index, canon_ref, expected set, positional fallback, threshold, and model"
)]
fn decode_single_exhibit(
    raw_item: &Value,
    index: usize,
    canon_ref: &str,
    expected: &BTreeSet<&str>,
    positional_expected: Option<&str>,
    threshold: f64,
    model: &str,
) -> Result<ExhibitAssessment, AdmitError> {
    let entry = raw_item.as_object().ok_or_else(|| {
        schema_decode_error(
            model,
            format!("expected object at `exhibits[{index}]` for `{canon_ref}`"),
        )
    })?;
    let raw_path = entry
        .get("file_path")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            schema_decode_error(
                model,
                format!("missing string `file_path` at `exhibits[{index}]` for `{canon_ref}`"),
            )
        })?;
    let resolved_path =
        resolve_exhibit_id(raw_path, expected, positional_expected).ok_or_else(|| {
            schema_decode_error(
                model,
                format!(
                    "unknown `file_path` `{raw_path}` in structured admit response for `{canon_ref}`"
                ),
            )
        })?;
    let raw_summary = entry
        .get("admissibility_summary")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            schema_decode_error(
                model,
                format!(
                    "missing string `admissibility_summary` for `{resolved_path}` in `{canon_ref}`"
                ),
            )
        })?;
    let raw_score = entry
        .get("admissibility_score")
        .and_then(Value::as_f64)
        .ok_or_else(|| {
            schema_decode_error(
                model,
                format!(
                    "missing numeric `admissibility_score` for `{resolved_path}` in `{canon_ref}`"
                ),
            )
        })?;
    let admissibility_score = normalize_admissibility_score(raw_score).map_err(|detail| {
        schema_decode_error(
            model,
            format!(
                "invalid `admissibility_score` for `{resolved_path}` in `{canon_ref}`: {detail}"
            ),
        )
    })?;

    Ok(ExhibitAssessment {
        file_path: resolved_path.to_string(),
        admissibility_summary: raw_summary.trim().to_string(),
        admissibility_score,
        admissibility_status: AdmissibilityStatus::from_score(admissibility_score, threshold),
    })
}
