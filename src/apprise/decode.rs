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

//! Structured JSON response decoding and canon path matching for `apprise`.

use crate::caseload::{ApprisalStatus, CanonApprisalAssessment, normalize_unit_score};
use crate::docket::resolve_canon_path;
use crate::provider::ProviderError;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn schema_decode_error(model_specifier: &str, message: impl Into<String>) -> ProviderError {
    ProviderError::SchemaDecode {
        model_specifier: model_specifier.to_string(),
        message: message.into(),
    }
}

pub(super) fn decode_apprise_assessments(
    payload: &Value,
    candidate_canons: &[String],
    threshold: f64,
    model_specifier: &str,
) -> Result<BTreeMap<String, CanonApprisalAssessment>, ProviderError> {
    let assessments_arr = payload
        .get("assessments")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            schema_decode_error(
                model_specifier,
                "missing array field `assessments` in structured apprise response",
            )
        })?;

    let expected: BTreeSet<&str> = candidate_canons.iter().map(String::as_str).collect();
    let same_len = assessments_arr.len() == candidate_canons.len();
    let mut out = BTreeMap::new();

    for (index, raw_item) in assessments_arr.iter().enumerate() {
        let pos_expected = same_len
            .then(|| candidate_canons.get(index).map(String::as_str))
            .flatten();
        let (canon_path, assessment) = decode_single_assessment(
            raw_item,
            index,
            &expected,
            pos_expected,
            threshold,
            model_specifier,
        )?;
        if out.insert(canon_path.clone(), assessment).is_some() {
            return Err(schema_decode_error(
                model_specifier,
                format!("duplicate `canon_path` `{canon_path}` in structured apprise response"),
            ));
        }
    }

    let missing: Vec<&str> = candidate_canons
        .iter()
        .map(String::as_str)
        .filter(|canon_ref| !out.contains_key(*canon_ref))
        .collect();
    if !missing.is_empty() {
        return Err(schema_decode_error(
            model_specifier,
            format!(
                "missing assessment entry for candidate canon(s): {}",
                missing.join(", ")
            ),
        ));
    }

    Ok(out)
}

fn decode_single_assessment(
    raw_item: &Value,
    index: usize,
    expected: &BTreeSet<&str>,
    positional_expected: Option<&str>,
    threshold: f64,
    model_specifier: &str,
) -> Result<(String, CanonApprisalAssessment), ProviderError> {
    let entry = raw_item.as_object().ok_or_else(|| {
        schema_decode_error(
            model_specifier,
            format!("expected object at `assessments[{index}]` in structured apprise response"),
        )
    })?;

    let raw_canon_path = entry
        .get("canon_path")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            schema_decode_error(
                model_specifier,
                format!("missing string `canon_path` at `assessments[{index}]`"),
            )
        })?;

    let canon_path =
        resolve_canon_path(raw_canon_path, expected, positional_expected).ok_or_else(|| {
            schema_decode_error(
                model_specifier,
                format!("unknown `canon_path` `{raw_canon_path}` in structured apprise response"),
            )
        })?;

    let raw_summary = entry
        .get("apprisal_summary")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            schema_decode_error(
                model_specifier,
                format!("missing string `apprisal_summary` for `{canon_path}`"),
            )
        })?;

    let raw_score = entry
        .get("apprisal_score")
        .and_then(Value::as_f64)
        .ok_or_else(|| {
            schema_decode_error(
                model_specifier,
                format!("missing numeric `apprisal_score` for `{canon_path}`"),
            )
        })?;

    let apprisal_score = normalize_unit_score(raw_score).map_err(|detail| {
        schema_decode_error(
            model_specifier,
            format!("invalid `apprisal_score` for `{canon_path}`: {detail}"),
        )
    })?;

    Ok((
        canon_path.to_string(),
        CanonApprisalAssessment {
            apprisal_summary: raw_summary.trim().to_string(),
            apprisal_score,
            apprisal_status: ApprisalStatus::from_score(apprisal_score, threshold),
        },
    ))
}
