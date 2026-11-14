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

//! Human-readable Markdown diagnostic renderer for consistency and flipper benchmark reports.

use super::{ConfigConsistencyReport, ConsistencyBenchReport, SubjectConsistency};
use std::fmt::Write as _;

/// Renders a [`ConsistencyBenchReport`] into Markdown overview tables and per-trial flipper rationales.
pub fn render_consistency_markdown(
    report: &ConsistencyBenchReport,
    include_stable: bool,
) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "## Consistency Benchmark (`{}`, N = {}, {} candidate canons)\n",
        report.stage.as_str(),
        report.trials,
        report.candidate_canon_count
    );

    render_overview_table(&mut out, report);

    for cfg in &report.configs {
        out.push('\n');
        render_config_section(&mut out, cfg, include_stable);
    }

    out
}

fn render_overview_table(out: &mut String, report: &ConsistencyBenchReport) {
    let pos_heading = report.stage.positive_heading();
    let _ = writeln!(
        out,
        "| Config | Model | {pos_heading} Counts | Range | Flippers | Unanimous | Mean StdDev | Max StdDev | Latency min / med / mean / max (ms) | Total Tokens |"
    );
    let _ = writeln!(
        out,
        "| :--- | :--- | :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: |"
    );

    for cfg in &report.configs {
        let counts_str = format_counts(&cfg.positive_counts);
        let range_span = cfg
            .positive_count_max
            .saturating_sub(cfg.positive_count_min);
        let range_str = format!(
            "{range_span} ({}..{})",
            cfg.positive_count_min, cfg.positive_count_max
        );
        let unanimous_pct = cfg.unanimous_agreement_rate * 100.0;
        let _ = writeln!(
            out,
            "| `{}` | `{}` | `{}` | `{}` | `{} / {}` | `{:.1}%` | `{:.4}` | `{:.4}` | `{} / {} / {:.1} / {}` | `{}` |",
            sanitize_inline(&cfg.label),
            sanitize_inline(&cfg.model),
            counts_str,
            range_str,
            cfg.flipper_count,
            cfg.subject_count,
            unanimous_pct,
            cfg.mean_score_stdev,
            cfg.max_score_stdev,
            cfg.latency_min_ms,
            cfg.latency_median_ms,
            cfg.latency_mean_ms,
            cfg.latency_max_ms,
            cfg.total_tokens
        );
    }
}

fn render_config_section(out: &mut String, cfg: &ConfigConsistencyReport, include_stable: bool) {
    let _ = writeln!(
        out,
        "### Config: `{}` (threshold `{:.2}`)\n",
        sanitize_inline(&cfg.label),
        cfg.threshold
    );
    render_trial_table(out, cfg);
    out.push('\n');
    render_flippers_section(out, cfg);

    if include_stable {
        out.push('\n');
        render_stable_section(out, cfg);
    }
}

fn render_trial_table(out: &mut String, cfg: &ConfigConsistencyReport) {
    let pos_heading = cfg.stage.positive_heading();
    let neg_heading = cfg.stage.negative_heading();
    let _ = writeln!(
        out,
        "| Trial | {pos_heading} | {neg_heading} | Latency (ms) | Prompt Tokens | Completion Tokens | Reasoning Tokens | Total Tokens |"
    );
    let _ = writeln!(
        out,
        "| :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |"
    );
    for t in &cfg.trial_summaries {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            t.trial,
            t.positive_count,
            t.negative_count,
            t.duration_ms,
            t.prompt_tokens,
            t.completion_tokens,
            t.reasoning_tokens,
            t.total_tokens
        );
    }
}

fn render_flippers_section(out: &mut String, cfg: &ConfigConsistencyReport) {
    let flippers = cfg.flippers();
    let _ = writeln!(
        out,
        "#### Flippers (`{} / {}`)",
        flippers.len(),
        cfg.subject_count
    );

    if flippers.is_empty() {
        let _ = writeln!(
            out,
            "\n_No flippers detected across {} trial(s) (100.0% unanimous agreement)._",
            cfg.trials
        );
        return;
    }

    out.push('\n');
    let pos_label = cfg.stage.positive_status_label();
    let neg_label = cfg.stage.negative_status_label();
    for subj in flippers {
        render_flipper_subject(out, subj, pos_label, neg_label);
    }
}

fn render_flipper_subject(
    out: &mut String,
    subj: &SubjectConsistency,
    pos_label: &str,
    neg_label: &str,
) {
    let total = subj.observations.len();
    let agreement_pct = subj.agreement_rate * 100.0;
    let _ = writeln!(
        out,
        "- **`{}`** — `{}/{} {}, {}/{} {}` (agreement `{:.1}%`, mean `{:.4}`, stdev `{:.4}`, range `{:.4}..{:.4}`)",
        sanitize_inline(&subj.display_id()),
        subj.positive_count,
        total,
        pos_label,
        subj.negative_count,
        total,
        neg_label,
        agreement_pct,
        subj.score_mean,
        subj.score_stdev,
        subj.score_min,
        subj.score_max
    );
    for obs in &subj.observations {
        let _ = writeln!(
            out,
            "  - Trial {}: `{}` (`{:.4}`) — {}",
            obs.trial,
            sanitize_inline(&obs.status),
            obs.score,
            sanitize_line(&obs.summary)
        );
    }
}

fn render_stable_section(out: &mut String, cfg: &ConfigConsistencyReport) {
    let stable: Vec<&SubjectConsistency> = cfg.subjects.iter().filter(|s| !s.is_flipper).collect();
    let _ = writeln!(
        out,
        "#### Unanimous Subjects (`{} / {}`)\n",
        stable.len(),
        cfg.subject_count
    );

    if stable.is_empty() {
        let _ = writeln!(out, "_No unanimous subjects._");
        return;
    }

    let _ = writeln!(
        out,
        "| Subject | Status | Mean Score | Score StdDev | Score Range |"
    );
    let _ = writeln!(out, "| :--- | :---: | :---: | :---: | :---: |");
    for subj in stable {
        let status = subj
            .observations
            .first()
            .map_or("n/a", |obs| obs.status.as_str());
        let _ = writeln!(
            out,
            "| `{}` | `{}` | `{:.4}` | `{:.4}` | `{:.4}..{:.4}` |",
            sanitize_inline(&subj.display_id()),
            sanitize_inline(status),
            subj.score_mean,
            subj.score_stdev,
            subj.score_min,
            subj.score_max
        );
    }
}

fn format_counts(counts: &[usize]) -> String {
    let joined: Vec<String> = counts.iter().map(usize::to_string).collect();
    format!("[{}]", joined.join(", "))
}

fn sanitize_inline(text: &str) -> String {
    sanitize_line(text).replace('|', "\\|").replace('`', "'")
}

fn sanitize_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apprise::DEFAULT_APPRISE_THRESHOLD;
    use crate::bench::extract::{ExtractedAssessment, SubjectKey};
    use crate::bench::stats::{TrialRecord, analyze_config_trials};
    use crate::bench::{BenchmarkStage, ConsistencyConfigSpec};
    use crate::provider::{DEFAULT_GENERATION_TIMEOUT_MS, TokenUsage};
    use std::collections::BTreeMap;

    fn sample_spec(label: &str) -> ConsistencyConfigSpec {
        ConsistencyConfigSpec {
            label: label.to_string(),
            threshold: DEFAULT_APPRISE_THRESHOLD,
            max_canons_per_call: Some(15),
            max_concurrent_calls: Some(4),
            timeout_ms: DEFAULT_GENERATION_TIMEOUT_MS,
        }
    }

    mod single_and_multi_config_tables {
        use super::*;

        #[test]
        fn test_renders_overview_trials_and_flipper_rationales() {
            let mut t1 = BTreeMap::new();
            t1.insert(
                SubjectKey::canon(".canons/cli/flip.md"),
                ExtractedAssessment {
                    status: "docketed".to_string(),
                    is_positive: true,
                    score: 0.85,
                    summary: "Touches CLI flag\nwith multi-line rationale.".to_string(),
                },
            );
            t1.insert(
                SubjectKey::canon(".canons/cli/steady.md"),
                ExtractedAssessment {
                    status: "dismissed".to_string(),
                    is_positive: false,
                    score: 0.1,
                    summary: "Orthogonal.".to_string(),
                },
            );

            let mut t2 = BTreeMap::new();
            t2.insert(
                SubjectKey::canon(".canons/cli/flip.md"),
                ExtractedAssessment {
                    status: "dismissed".to_string(),
                    is_positive: false,
                    score: 0.2,
                    summary: "No direct mutation.".to_string(),
                },
            );
            t2.insert(
                SubjectKey::canon(".canons/cli/steady.md"),
                ExtractedAssessment {
                    status: "dismissed".to_string(),
                    is_positive: false,
                    score: 0.1,
                    summary: "Orthogonal.".to_string(),
                },
            );

            let cfg_report = analyze_config_trials(
                &sample_spec("batch=15,conc=4"),
                BenchmarkStage::Docket,
                "google:gemini-flash-lite-latest",
                &[
                    TrialRecord {
                        trial: 1,
                        duration_ms: 1100,
                        usage: TokenUsage {
                            prompt_tokens: 200,
                            completion_tokens: 40,
                            reasoning_tokens: 0,
                            total_tokens: 240,
                        },
                        assessments: t1,
                    },
                    TrialRecord {
                        trial: 2,
                        duration_ms: 1300,
                        usage: TokenUsage {
                            prompt_tokens: 200,
                            completion_tokens: 45,
                            reasoning_tokens: 0,
                            total_tokens: 245,
                        },
                        assessments: t2,
                    },
                ],
            );

            let report = ConsistencyBenchReport {
                stage: BenchmarkStage::Docket,
                trials: 2,
                candidate_canon_count: 2,
                configs: vec![cfg_report],
            };

            let md = render_consistency_markdown(&report, false);
            assert!(md.contains("## Consistency Benchmark (`docket`, N = 2, 2 candidate canons)"));
            assert!(md.contains("| `batch=15,conc=4` | `google:gemini-flash-lite-latest` | `[1, 0]` | `1 (0..1)` | `1 / 2` | `50.0%` |"));
            assert!(md.contains("#### Flippers (`1 / 2`)"));
            assert!(md.contains("**`.canons/cli/flip.md`** — `1/2 docketed, 1/2 dismissed`"));
            assert!(md.contains(
                "Trial 1: `docketed` (`0.8500`) — Touches CLI flag with multi-line rationale."
            ));
            assert!(md.contains("Trial 2: `dismissed` (`0.2000`) — No direct mutation."));
            assert!(!md.contains("Unanimous Subjects"));

            let md_with_stable = render_consistency_markdown(&report, true);
            assert!(md_with_stable.contains("#### Unanimous Subjects (`1 / 2`)"));
            assert!(md_with_stable.contains("| `.canons/cli/steady.md` | `dismissed` |"));
        }
    }

    mod zero_flipper_and_exhibit_modes {
        use super::*;

        #[test]
        fn test_renders_zero_flippers_notice_and_composite_exhibit_ids() {
            let mut t1 = BTreeMap::new();
            t1.insert(
                SubjectKey::exhibit(".canons/cli/flags.md", "src/cli/args.rs"),
                ExtractedAssessment {
                    status: "admitted".to_string(),
                    is_positive: true,
                    score: 0.91,
                    summary: "Relevant diff hunk.".to_string(),
                },
            );
            let cfg_report = analyze_config_trials(
                &sample_spec("admit-default"),
                BenchmarkStage::Admit,
                "google:gemini-flash-lite-latest",
                &[TrialRecord {
                    trial: 1,
                    duration_ms: 420,
                    usage: TokenUsage::default(),
                    assessments: t1,
                }],
            );
            let report = ConsistencyBenchReport {
                stage: BenchmarkStage::Admit,
                trials: 1,
                candidate_canon_count: 1,
                configs: vec![cfg_report],
            };

            let md = render_consistency_markdown(&report, true);
            assert!(md.contains(
                "_No flippers detected across 1 trial(s) (100.0% unanimous agreement)._"
            ));
            assert!(md.contains(".canons/cli/flags.md :: src/cli/args.rs"));
        }
    }
}
