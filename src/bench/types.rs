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

//! Core configuration and report data structures for the consistency benchmark harness (`docs/architecture/benchmarking.md`).

use super::{BenchmarkStage, DEFAULT_BENCHMARK_TRIALS};
use crate::apprise::DEFAULT_APPRISE_THRESHOLD;
use crate::provider::DEFAULT_GENERATION_TIMEOUT_MS;
use serde::{Deserialize, Serialize};

/// Execution parameters for a single benchmark configuration (e.g. unbounded vs. `batch=15,conc=4`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ConsistencyConfigSpec {
    /// Human-readable configuration label (`"unbounded"`, `"batch=15,conc=4"`, etc.).
    pub label: String,
    /// Classification score threshold in `[0.0, 1.0]`.
    pub threshold: f64,
    /// Optional maximum candidate canons per screener model call (`None` = single-turn unbounded).
    pub max_canons_per_call: Option<usize>,
    /// Optional maximum concurrent in-flight model calls when batched.
    pub max_concurrent_calls: Option<usize>,
    /// Per-request generation timeout ceiling in milliseconds.
    pub timeout_ms: u64,
}

impl Default for ConsistencyConfigSpec {
    fn default() -> Self {
        Self {
            label: "unbounded".to_string(),
            threshold: DEFAULT_APPRISE_THRESHOLD,
            max_canons_per_call: None,
            max_concurrent_calls: None,
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
        }
    }
}

/// Input options configuring [`super::execute_consistency_bench`].
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ConsistencyBenchOptions {
    /// Target Caseload heuristic stage.
    pub stage: BenchmarkStage,
    /// Number of repeated trials per configuration (`>= 1`; defaults to [`DEFAULT_BENCHMARK_TRIALS`]).
    pub trials: usize,
    /// One or more configurations to evaluate across `trials`.
    pub configs: Vec<ConsistencyConfigSpec>,
}

impl Default for ConsistencyBenchOptions {
    fn default() -> Self {
        Self {
            stage: BenchmarkStage::Apprise,
            trials: DEFAULT_BENCHMARK_TRIALS,
            configs: vec![ConsistencyConfigSpec::default()],
        }
    }
}

/// Per-trial outcome recorded for a single canon or `(canon_path, exhibit_path)` subject.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SubjectObservation {
    /// 1-indexed trial number (`1..=N`).
    pub trial: usize,
    /// Stage status string (`"applicable"`, `"docketed"`, `"admitted"`, `"vetted"`, `"pass"`, etc.).
    pub status: String,
    /// Whether this trial produced an affirmative classification (`score >= threshold`).
    pub is_positive: bool,
    /// Continuous score in `[0.0, 1.0]`.
    pub score: f64,
    /// Rationale summary emitted by the model in this trial.
    pub summary: String,
}

/// Aggregate operational and count metrics for a single trial run (`1..=N`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TrialSummary {
    /// 1-indexed trial number (`1..=N`).
    pub trial: usize,
    /// Total evaluated subjects in this trial.
    pub subject_count: usize,
    /// Count of subjects classified affirmatively (`applicable`, `docketed`, `admitted`, `vetted`, `pass`).
    pub positive_count: usize,
    /// Count of subjects classified negatively (`dismissed`, `excluded`, `contested`, `fail`).
    pub negative_count: usize,
    /// Stage wall-clock latency in milliseconds.
    pub duration_ms: u64,
    /// Prompt tokens consumed across the trial.
    pub prompt_tokens: u64,
    /// Completion tokens consumed across the trial.
    pub completion_tokens: u64,
    /// Extended reasoning tokens consumed across the trial.
    pub reasoning_tokens: u64,
    /// Total tokens consumed across the trial.
    pub total_tokens: u64,
}

/// Multi-trial consistency and flipper analysis for a single canon or `(canon_path, exhibit_path)` subject.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SubjectConsistency {
    /// Repository-relative canon path or identifier.
    pub canon_path: String,
    /// Optional repository-relative exhibit path for micro evidentiary triage (`admit`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exhibit_path: Option<String>,
    /// `true` if classification status varied across the `N` trials.
    pub is_flipper: bool,
    /// Number of trials with an affirmative status (`applicable`, `docketed`, `admitted`, `vetted`, `pass`).
    pub positive_count: usize,
    /// Number of trials with a negative status (`dismissed`, `excluded`, `contested`, `fail`).
    pub negative_count: usize,
    /// Modal status agreement fraction in `(0.0, 1.0]` (`1.0` when unanimous).
    pub agreement_rate: f64,
    /// Arithmetic mean of continuous scores across trials.
    pub score_mean: f64,
    /// Sample standard deviation (`N - 1` denominator; `0.0` when `N <= 1`) of scores across trials.
    pub score_stdev: f64,
    /// Minimum score observed across trials.
    pub score_min: f64,
    /// Maximum score observed across trials.
    pub score_max: f64,
    /// Chronological per-trial observations (`1..=N`).
    pub observations: Vec<SubjectObservation>,
}

impl SubjectConsistency {
    /// Formats `"<canon_path>"` or `"<canon_path> :: <exhibit_path>"` for human-readable tables.
    pub fn display_id(&self) -> String {
        match &self.exhibit_path {
            Some(exhibit) => format!("{} :: {exhibit}", self.canon_path),
            None => self.canon_path.clone(),
        }
    }
}

/// Multi-trial consistency summary for a single [`ConsistencyConfigSpec`].
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ConfigConsistencyReport {
    /// Configuration label (`"unbounded"`, `"batch=15,conc=4"`, etc.).
    pub label: String,
    /// Evaluated Caseload stage.
    pub stage: BenchmarkStage,
    /// Resolved model specifier used for the stage.
    pub model: String,
    /// Number of completed trials (`N`).
    pub trials: usize,
    /// Applied classification score threshold.
    pub threshold: f64,
    /// Optional batch ceiling (`max_canons_per_call`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_canons_per_call: Option<usize>,
    /// Optional concurrency ceiling (`max_concurrent_calls`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent_calls: Option<usize>,
    /// Total distinct subjects evaluated across trials.
    pub subject_count: usize,
    /// Count of subjects whose classification status flipped at least once.
    pub flipper_count: usize,
    /// Fraction of subjects with 100% unanimous status agreement across all `N` trials.
    pub unanimous_agreement_rate: f64,
    /// Per-trial affirmative counts (`[c_1, c_2, ..., c_N]`).
    pub positive_counts: Vec<usize>,
    /// Minimum affirmative count across trials.
    pub positive_count_min: usize,
    /// Maximum affirmative count across trials.
    pub positive_count_max: usize,
    /// Mean per-subject score sample standard deviation.
    pub mean_score_stdev: f64,
    /// Maximum per-subject score sample standard deviation.
    pub max_score_stdev: f64,
    /// Minimum trial wall-clock latency in milliseconds.
    pub latency_min_ms: u64,
    /// Median trial wall-clock latency in milliseconds.
    pub latency_median_ms: u64,
    /// Mean trial wall-clock latency in milliseconds.
    pub latency_mean_ms: f64,
    /// Maximum trial wall-clock latency in milliseconds.
    pub latency_max_ms: u64,
    /// Cumulative tokens expended across all `N` trials for this configuration.
    pub total_tokens: u64,
    /// Per-trial operational and count summaries.
    pub trial_summaries: Vec<TrialSummary>,
    /// Stably sorted per-subject consistency records.
    pub subjects: Vec<SubjectConsistency>,
}

impl ConfigConsistencyReport {
    /// Returns all subjects whose classification status flipped across trials (`is_flipper == true`).
    pub fn flippers(&self) -> Vec<&SubjectConsistency> {
        self.subjects.iter().filter(|s| s.is_flipper).collect()
    }
}

/// Top-level structured report produced by [`super::execute_consistency_bench`].
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ConsistencyBenchReport {
    /// Target Caseload heuristic stage.
    pub stage: BenchmarkStage,
    /// Number of trials executed per configuration.
    pub trials: usize,
    /// Number of candidate canons in `.discovery.candidate_canons`.
    pub candidate_canon_count: usize,
    /// Consistency results for each evaluated configuration.
    pub configs: Vec<ConfigConsistencyReport>,
}
