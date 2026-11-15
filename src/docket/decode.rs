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

//! Structured JSON response decoding and canon path matching for `docket`.

use crate::caseload::{CanonDocketAssessment, ColorabilityStatus, normalize_colorability_score};
use crate::provider::ProviderError;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn schema_decode_error(model_specifier: &str, message: impl Into<String>) -> ProviderError {
    ProviderError::SchemaDecode {
        model_specifier: model_specifier.to_string(),
        message: message.into(),
    }
}

pub(super) fn decode_docket_cases(
    payload: &Value,
    candidate_canons: &[String],
    threshold: f64,
    model_specifier: &str,
) -> Result<BTreeMap<String, CanonDocketAssessment>, ProviderError> {
    let cases_arr = payload
        .get("cases")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            schema_decode_error(
                model_specifier,
                "missing array field `cases` in structured docket response",
            )
        })?;

    let expected: BTreeSet<&str> = candidate_canons.iter().map(String::as_str).collect();
    let same_len = cases_arr.len() == candidate_canons.len();
    let mut out = BTreeMap::new();

    for (index, raw_item) in cases_arr.iter().enumerate() {
        let pos_expected = same_len
            .then(|| candidate_canons.get(index).map(String::as_str))
            .flatten();
        let (canon_path, assessment) = decode_single_case(
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
                format!("duplicate `canon_path` `{canon_path}` in structured docket response"),
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
                "missing case entry for candidate canon(s): {}",
                missing.join(", ")
            ),
        ));
    }

    Ok(out)
}

pub(crate) fn resolve_canon_path<'a>(
    raw_path: &'a str,
    expected: &BTreeSet<&'a str>,
    pos: Option<&'a str>,
) -> Option<&'a str> {
    if let Some(&exact) = expected.get(raw_path) {
        return Some(exact);
    }
    let pos = pos?;
    if raw_path.rsplit('/').next() == pos.rsplit('/').next() {
        return Some(pos);
    }
    let pref = raw_path
        .bytes()
        .zip(pos.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let suff = raw_path
        .bytes()
        .rev()
        .zip(pos.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    ((pref + suff) * 5 >= raw_path.len().max(pos.len()).max(1) * 4).then_some(pos)
}

fn decode_single_case(
    raw_item: &Value,
    index: usize,
    expected: &BTreeSet<&str>,
    positional_expected: Option<&str>,
    threshold: f64,
    model: &str,
) -> Result<(String, CanonDocketAssessment), ProviderError> {
    let entry = raw_item.as_object().ok_or_else(|| {
        schema_decode_error(
            model,
            format!("expected object at `cases[{index}]` in structured docket response"),
        )
    })?;
    let raw_canon_path = entry["canon_path"].as_str().ok_or_else(|| {
        schema_decode_error(
            model,
            format!("missing string `canon_path` at `cases[{index}]`"),
        )
    })?;
    let canon_path =
        resolve_canon_path(raw_canon_path, expected, positional_expected).ok_or_else(|| {
            schema_decode_error(
                model,
                format!("unknown `canon_path` `{raw_canon_path}` in structured docket response"),
            )
        })?;
    let raw_summary = entry["colorability_summary"].as_str().ok_or_else(|| {
        schema_decode_error(
            model,
            format!("missing string `colorability_summary` for `{canon_path}`"),
        )
    })?;
    let raw_score = entry["colorability_score"].as_f64().ok_or_else(|| {
        schema_decode_error(
            model,
            format!("missing numeric `colorability_score` for `{canon_path}`"),
        )
    })?;
    let colorability_score = normalize_colorability_score(raw_score).map_err(|detail| {
        schema_decode_error(
            model,
            format!("invalid `colorability_score` for `{canon_path}`: {detail}"),
        )
    })?;

    Ok((
        canon_path.to_string(),
        CanonDocketAssessment {
            colorability_summary: raw_summary.trim().to_string(),
            colorability_score,
            colorability_status: ColorabilityStatus::from_score(colorability_score, threshold),
        },
    ))
}
