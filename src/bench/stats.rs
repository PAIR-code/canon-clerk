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

//! Statistical aggregation, sample standard deviation, latency summaries, and flipper analysis.

use super::extract::{ExtractedAssessment, SubjectKey};
use super::{
    BenchmarkStage, ConfigConsistencyReport, ConsistencyConfigSpec, SubjectConsistency,
    SubjectObservation, TrialSummary,
};
use crate::provider::TokenUsage;
use std::collections::{BTreeMap, BTreeSet};

const OMITTED_BELOW_THRESHOLD_SUMMARY: &str = "(omitted below threshold)";

/// Raw outcome and operational telemetry captured from a single trial execution.
#[derive(Debug, Clone)]
pub struct TrialRecord {
    /// 1-indexed trial number (`1..=N`).
    pub trial: usize,
    /// Stage wall-clock latency in milliseconds.
    pub duration_ms: u64,
    /// Aggregated token accounting across all model calls in the trial.
    pub usage: TokenUsage,
    /// Deterministic per-subject assessments extracted from the trial's enriched `Caseload`.
    pub assessments: BTreeMap<SubjectKey, ExtractedAssessment>,
}

/// Computes arithmetic mean of `values` (`0.0` when empty).
pub(crate) fn mean_f64(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let sum: f64 = values.iter().copied().sum();
    normalize_zero(sum / (values.len() as f64))
}

/// Computes sample standard deviation ($s = \sqrt{\frac{1}{N - 1}\sum (x_i - \bar{x})^2}$), returning `0.0` when `N <= 1`.
pub(crate) fn sample_stdev(values: &[f64]) -> f64 {
    if values.len() <= 1 {
        return 0.0;
    }
    let mean = mean_f64(values);
    let variance_sum: f64 = values
        .iter()
        .map(|v| {
            let diff = *v - mean;
            diff * diff
        })
        .sum();
    let stdev = (variance_sum / ((values.len() - 1) as f64)).sqrt();
    normalize_zero(stdev)
}

/// Computes median of a `u64` slice (`0` when empty; rounded average of middle pair when even).
pub(crate) fn median_u64(values: &[u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        let low = u128::from(sorted[mid - 1]);
        let high = u128::from(sorted[mid]);
        ((low + high) / 2) as u64
    }
}

fn normalize_zero(val: f64) -> f64 {
    if val == 0.0 || !val.is_finite() {
        0.0
    } else {
        val
    }
}

/// Aggregates `records` across `N` trials into a deterministic [`ConfigConsistencyReport`].
pub fn analyze_config_trials(
    spec: &ConsistencyConfigSpec,
    stage: BenchmarkStage,
    model: &str,
    records: &[TrialRecord],
) -> ConfigConsistencyReport {
    let mut all_keys = BTreeSet::new();
    for rec in records {
        for key in rec.assessments.keys() {
            all_keys.insert(key.clone());
        }
    }

    let subject_total = all_keys.len();
    let trial_summaries: Vec<TrialSummary> = records
        .iter()
        .map(|rec| build_trial_summary(rec, subject_total))
        .collect();

    let subjects: Vec<SubjectConsistency> = all_keys
        .into_iter()
        .map(|key| build_subject_consistency(key, stage, records))
        .collect();

    let flipper_count = subjects.iter().filter(|s| s.is_flipper).count();
    let unanimous_agreement_rate = if subject_total == 0 {
        1.0
    } else {
        normalize_zero(((subject_total - flipper_count) as f64) / (subject_total as f64))
    };

    let positive_counts: Vec<usize> = trial_summaries.iter().map(|t| t.positive_count).collect();
    let positive_count_min = positive_counts.iter().copied().min().unwrap_or(0);
    let positive_count_max = positive_counts.iter().copied().max().unwrap_or(0);

    let stdevs: Vec<f64> = subjects.iter().map(|s| s.score_stdev).collect();
    let mean_score_stdev = mean_f64(&stdevs);
    let max_score_stdev = stdevs.iter().copied().fold(0.0_f64, f64::max);

    let durations: Vec<u64> = trial_summaries.iter().map(|t| t.duration_ms).collect();
    let duration_f64: Vec<f64> = durations.iter().map(|d| *d as f64).collect();
    let latency_min_ms = durations.iter().copied().min().unwrap_or(0);
    let latency_median_ms = median_u64(&durations);
    let latency_mean_ms = mean_f64(&duration_f64);
    let latency_max_ms = durations.iter().copied().max().unwrap_or(0);
    let total_tokens = trial_summaries.iter().map(|t| t.total_tokens).sum();

    ConfigConsistencyReport {
        label: spec.label.clone(),
        stage,
        model: model.to_string(),
        trials: records.len(),
        threshold: spec.threshold,
        max_canons_per_call: spec.max_canons_per_call,
        max_concurrent_calls: spec.max_concurrent_calls,
        subject_count: subject_total,
        flipper_count,
        unanimous_agreement_rate,
        positive_counts,
        positive_count_min,
        positive_count_max,
        mean_score_stdev,
        max_score_stdev: normalize_zero(max_score_stdev),
        latency_min_ms,
        latency_median_ms,
        latency_mean_ms,
        latency_max_ms,
        total_tokens,
        trial_summaries,
        subjects,
    }
}

fn build_trial_summary(rec: &TrialRecord, subject_total: usize) -> TrialSummary {
    let positive_count = rec.assessments.values().filter(|a| a.is_positive).count();
    let negative_count = subject_total.saturating_sub(positive_count);
    TrialSummary {
        trial: rec.trial,
        subject_count: subject_total,
        positive_count,
        negative_count,
        duration_ms: rec.duration_ms,
        prompt_tokens: rec.usage.prompt_tokens,
        completion_tokens: rec.usage.completion_tokens,
        reasoning_tokens: rec.usage.reasoning_tokens,
        total_tokens: rec.usage.total_tokens,
    }
}

fn build_subject_consistency(
    key: SubjectKey,
    stage: BenchmarkStage,
    records: &[TrialRecord],
) -> SubjectConsistency {
    let mut observations = Vec::with_capacity(records.len());
    let mut status_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut positive_count = 0usize;
    let mut scores = Vec::with_capacity(records.len());

    for rec in records {
        let obs = match rec.assessments.get(&key) {
            Some(found) => SubjectObservation {
                trial: rec.trial,
                status: found.status.clone(),
                is_positive: found.is_positive,
                score: normalize_zero(found.score),
                summary: found.summary.clone(),
            },
            None => SubjectObservation {
                trial: rec.trial,
                status: stage.negative_status_label().to_string(),
                is_positive: false,
                score: 0.0,
                summary: OMITTED_BELOW_THRESHOLD_SUMMARY.to_string(),
            },
        };
        if obs.is_positive {
            positive_count += 1;
        }
        *status_counts.entry(obs.status.clone()).or_insert(0) += 1;
        scores.push(obs.score);
        observations.push(obs);
    }

    let total_obs = observations.len();
    let negative_count = total_obs.saturating_sub(positive_count);
    let is_flipper = status_counts.len() > 1 || (positive_count > 0 && negative_count > 0);
    let max_freq = status_counts.values().copied().max().unwrap_or(0);
    let agreement_rate = if total_obs == 0 {
        1.0
    } else {
        normalize_zero((max_freq as f64) / (total_obs as f64))
    };

    let score_mean = mean_f64(&scores);
    let score_stdev = sample_stdev(&scores);
    let score_min = scores
        .iter()
        .copied()
        .reduce(f64::min)
        .map_or(0.0, normalize_zero);
    let score_max = scores
        .iter()
        .copied()
        .reduce(f64::max)
        .map_or(0.0, normalize_zero);

    SubjectConsistency {
        canon_path: key.canon_path,
        exhibit_path: key.exhibit_path,
        is_flipper,
        positive_count,
        negative_count,
        agreement_rate,
        score_mean,
        score_stdev,
        score_min,
        score_max,
        observations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apprise::DEFAULT_APPRISE_THRESHOLD;
    use crate::provider::DEFAULT_GENERATION_TIMEOUT_MS;

    mod statistical_primitives {
        use super::*;

        #[test]
        fn test_mean_stdev_and_median_calculations() {
            assert_eq!(mean_f64(&[]), 0.0);
            assert_eq!(sample_stdev(&[]), 0.0);
            assert_eq!(sample_stdev(&[0.85]), 0.0);
            assert_eq!(sample_stdev(&[0.9, 0.9, 0.9, 0.9, 0.9]), 0.0);

            let flipped = [0.9, 0.9, 0.1, 0.9, 0.1];
            assert!((mean_f64(&flipped) - 0.58).abs() < 1e-9);
            // Variance = (3*(0.32^2) + 2*(-0.48^2)) / 4 = (0.3072 + 0.4608) / 4 = 0.192 -> sqrt = 0.438178...
            assert!((sample_stdev(&flipped) - 0.438_178_046).abs() < 1e-6);

            assert_eq!(median_u64(&[]), 0);
            assert_eq!(median_u64(&[500, 100, 300]), 300);
            assert_eq!(median_u64(&[100, 200, 300, 400]), 250);
        }
    }

    mod flipper_classification_and_agreement {
        use super::*;

        fn two_trial_records() -> Vec<TrialRecord> {
            let mut t1 = BTreeMap::new();
            t1.insert(
                SubjectKey::canon(".canons/z-flipper.md"),
                ExtractedAssessment {
                    status: "applicable".to_string(),
                    is_positive: true,
                    score: 0.85,
                    summary: "Trial 1 applicable.".to_string(),
                },
            );
            t1.insert(
                SubjectKey::canon(".canons/a-stable.md"),
                ExtractedAssessment {
                    status: "dismissed".to_string(),
                    is_positive: false,
                    score: 0.10,
                    summary: "Always dismissed.".to_string(),
                },
            );

            let mut t2 = BTreeMap::new();
            t2.insert(
                SubjectKey::canon(".canons/z-flipper.md"),
                ExtractedAssessment {
                    status: "dismissed".to_string(),
                    is_positive: false,
                    score: 0.15,
                    summary: "Trial 2 dismissed.".to_string(),
                },
            );
            t2.insert(
                SubjectKey::canon(".canons/a-stable.md"),
                ExtractedAssessment {
                    status: "dismissed".to_string(),
                    is_positive: false,
                    score: 0.12,
                    summary: "Always dismissed.".to_string(),
                },
            );

            vec![
                TrialRecord {
                    trial: 1,
                    duration_ms: 1200,
                    usage: TokenUsage {
                        prompt_tokens: 100,
                        completion_tokens: 50,
                        reasoning_tokens: 0,
                        total_tokens: 150,
                    },
                    assessments: t1,
                },
                TrialRecord {
                    trial: 2,
                    duration_ms: 1400,
                    usage: TokenUsage {
                        prompt_tokens: 100,
                        completion_tokens: 60,
                        reasoning_tokens: 0,
                        total_tokens: 160,
                    },
                    assessments: t2,
                },
            ]
        }

        #[test]
        fn test_identifies_flipper_vs_stable_subjects_and_sorts_deterministically() {
            let spec = ConsistencyConfigSpec {
                label: "batch=15,conc=4".to_string(),
                threshold: DEFAULT_APPRISE_THRESHOLD,
                max_canons_per_call: Some(15),
                max_concurrent_calls: Some(4),
                timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
            };
            let records = two_trial_records();
            let report = analyze_config_trials(
                &spec,
                BenchmarkStage::Apprise,
                "google:gemini-flash-lite-latest",
                &records,
            );
            assert_eq!(report.subject_count, 2);
            assert_eq!(report.flipper_count, 1);
            assert_eq!(report.unanimous_agreement_rate, 0.5);
            assert_eq!(report.positive_counts, vec![1, 0]);
            assert_eq!(report.latency_min_ms, 1200);
            assert_eq!(report.latency_median_ms, 1300);
            assert_eq!(report.latency_max_ms, 1400);
            assert_eq!(report.total_tokens, 310);

            assert!(!report.subjects[0].is_flipper);
            assert_eq!(report.subjects[0].canon_path, ".canons/a-stable.md");
            assert!(report.subjects[1].is_flipper);
            assert_eq!(report.subjects[1].canon_path, ".canons/z-flipper.md");
            assert_eq!(report.subjects[1].agreement_rate, 0.5);
        }
    }

    mod sparse_exhibit_union {
        use super::*;

        #[test]
        fn test_sparse_admit_exhibits_backfill_excluded_trials_as_flippers() {
            let spec = ConsistencyConfigSpec {
                label: "admit-default".to_string(),
                threshold: DEFAULT_APPRISE_THRESHOLD,
                max_canons_per_call: None,
                max_concurrent_calls: None,
                timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
            };

            let key = SubjectKey::exhibit(".canons/cli/flags.md", "src/cli/args.rs");
            let mut t1 = BTreeMap::new();
            t1.insert(
                key,
                ExtractedAssessment {
                    status: "admitted".to_string(),
                    is_positive: true,
                    score: 0.88,
                    summary: "Admitted in trial 1.".to_string(),
                },
            );

            let records = vec![
                TrialRecord {
                    trial: 1,
                    duration_ms: 400,
                    usage: TokenUsage::default(),
                    assessments: t1,
                },
                TrialRecord {
                    trial: 2,
                    duration_ms: 380,
                    usage: TokenUsage::default(),
                    assessments: BTreeMap::new(),
                },
            ];

            let report = analyze_config_trials(
                &spec,
                BenchmarkStage::Admit,
                "google:gemini-flash-lite-latest",
                &records,
            );
            assert_eq!(report.subject_count, 1);
            assert_eq!(report.flipper_count, 1);
            let subj = &report.subjects[0];
            assert_eq!(subj.exhibit_path.as_deref(), Some("src/cli/args.rs"));
            assert_eq!(subj.positive_count, 1);
            assert_eq!(subj.negative_count, 1);
            assert_eq!(subj.observations[1].status, "excluded");
            assert_eq!(
                subj.observations[1].summary,
                OMITTED_BELOW_THRESHOLD_SUMMARY
            );
        }
    }
}
