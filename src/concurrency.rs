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

//! Shared bounded-concurrency stage work executor guaranteeing uniform worker topology
//! and clean drain-to-zero semantics at every Caseload DAG node seam.

use crate::provider::StructuredModelProvider;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

type WorkSlot<O, E> = Option<Result<O, E>>;

/// Executes `items` across at most `concurrency_limit` scoped worker threads (`>= 1`),
/// draining all in-flight worker activity before returning (`docs/architecture/scheduling.md` §3).
///
/// - Preserves deterministic input index ordering (`0..items.len()`) in the returned `Vec<O>`
///   regardless of worker completion order.
/// - Emits monotonic `1..=total` `provider.record_progress(completed, total, &label)` ticks
///   as each work unit succeeds.
/// - Upon error, cooperatively stops workers from claiming additional items, drains all
///   currently in-flight workers cleanly before exiting `std::thread::scope`, and surfaces
///   the lowest-index error deterministically.
pub(crate) fn execute_bounded_stage_work<I, O, E, F>(
    items: &[I],
    concurrency_limit: usize,
    provider: &dyn StructuredModelProvider,
    evaluate: F,
) -> Result<Vec<O>, E>
where
    I: Sync,
    O: Send,
    E: Send,
    F: Fn(usize, &I) -> Result<(O, String), E> + Sync,
{
    let total = items.len();
    if total == 0 {
        return Ok(Vec::new());
    }

    let worker_count = concurrency_limit.max(1).min(total);
    let next_index = AtomicUsize::new(0);
    let aborted = AtomicBool::new(false);
    let outcomes: Mutex<(usize, Vec<WorkSlot<O, E>>)> =
        Mutex::new((0, (0..total).map(|_| None).collect()));

    provider.clear_abort();
    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| {
                loop {
                    if aborted.load(Ordering::Acquire) {
                        break;
                    }
                    let idx = next_index.fetch_add(1, Ordering::SeqCst);
                    if idx >= total {
                        break;
                    }
                    match evaluate(idx, &items[idx]) {
                        Ok((output, label)) => {
                            if let Ok(mut guard) = outcomes.lock() {
                                guard.1[idx] = Some(Ok(output));
                                guard.0 += 1;
                                provider.record_progress(guard.0, total, &label);
                            }
                        }
                        Err(err) => {
                            aborted.store(true, Ordering::Release);
                            provider.notify_abort();
                            if let Ok(mut guard) = outcomes.lock() {
                                guard.1[idx] = Some(Err(err));
                            }
                        }
                    }
                }
            });
        }
    });
    provider.clear_abort();

    let (_, collected) = outcomes.into_inner().unwrap_or_default();
    let mut ordered = Vec::with_capacity(total);
    for slot in collected.into_iter().flatten() {
        ordered.push(slot?);
    }
    Ok(ordered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{GenerationRequest, GenerationResponse, ProviderError};
    use std::thread;
    use std::time::Duration;

    struct ProgressRecordingStubProvider {
        ticks: Mutex<Vec<(usize, usize, String)>>,
    }

    impl ProgressRecordingStubProvider {
        fn new() -> Self {
            Self {
                ticks: Mutex::new(Vec::new()),
            }
        }

        fn recorded_ticks(&self) -> Vec<(usize, usize, String)> {
            self.ticks.lock().expect("lock ticks").clone()
        }
    }

    impl StructuredModelProvider for ProgressRecordingStubProvider {
        fn generate_structured(
            &self,
            request: &GenerationRequest,
        ) -> Result<GenerationResponse, ProviderError> {
            Err(ProviderError::UnsupportedProvider {
                provider: "stub".to_string(),
                model_specifier: request.model_specifier.clone(),
            })
        }

        fn record_progress(&self, completed: usize, total: usize, label: &str) {
            self.ticks
                .lock()
                .expect("lock ticks")
                .push((completed, total, label.to_string()));
        }
    }

    mod deterministic_ordering_and_progress {
        use super::*;

        #[test]
        fn test_empty_and_singleton_slices_execute_uniformly_and_drain() {
            let provider = ProgressRecordingStubProvider::new();
            let empty: Vec<u32> = Vec::new();
            let out_empty: Vec<u32> = execute_bounded_stage_work(&empty, 4, &provider, |_, &v| {
                Ok::<_, &str>((v, "unused".to_string()))
            })
            .expect("empty slice succeeds");
            assert!(out_empty.is_empty());
            assert!(provider.recorded_ticks().is_empty());

            let caller_tid = thread::current().id();
            let single = [42_u32];
            let out_single = execute_bounded_stage_work(&single, 4, &provider, |idx, &v| {
                assert_ne!(
                    thread::current().id(),
                    caller_tid,
                    "1-item stage work must run on a scoped worker rather than mutating caller thread state"
                );
                Ok::<_, &str>((v * 2, format!("unit-{}", idx + 1)))
            })
            .expect("singleton slice succeeds");
            assert_eq!(out_single, vec![84]);
            assert_eq!(
                provider.recorded_ticks(),
                vec![(1, 1, "unit-1".to_string())]
            );
        }

        #[test]
        fn test_out_of_order_completions_fold_in_deterministic_input_order() {
            let provider = ProgressRecordingStubProvider::new();
            let delays_ms = [60_u64, 5, 35, 10];

            let outputs = execute_bounded_stage_work(&delays_ms, 4, &provider, |idx, &delay| {
                thread::sleep(Duration::from_millis(delay));
                Ok::<_, &str>((format!("item-{idx}"), format!("label-{idx}")))
            })
            .expect("concurrent items succeed");

            assert_eq!(outputs, vec!["item-0", "item-1", "item-2", "item-3"]);
            let ticks = provider.recorded_ticks();
            assert_eq!(ticks.len(), 4);
            let counters: Vec<usize> = ticks.iter().map(|(c, _, _)| *c).collect();
            assert_eq!(counters, vec![1, 2, 3, 4]);
            assert!(ticks.iter().all(|(_, total, _)| *total == 4));
        }
    }

    mod cooperative_abort_and_seam_drain {
        use super::*;

        #[test]
        fn test_abort_stops_unqueued_items_drains_inflight_workers_and_returns_lowest_index_error()
        {
            let provider = ProgressRecordingStubProvider::new();
            let in_flight = AtomicUsize::new(0);
            let started = AtomicUsize::new(0);
            let items: Vec<usize> = (0..8).collect();

            let err = execute_bounded_stage_work(&items, 2, &provider, |idx, _| {
                started.fetch_add(1, Ordering::SeqCst);
                in_flight.fetch_add(1, Ordering::SeqCst);
                let result = if idx == 0 {
                    thread::sleep(Duration::from_millis(40));
                    Err("failure-at-0")
                } else if idx == 1 {
                    Err("failure-at-1")
                } else {
                    Ok((idx, format!("ok-{idx}")))
                };
                in_flight.fetch_sub(1, Ordering::SeqCst);
                result
            })
            .expect_err("stage must fail when worker errors");

            assert_eq!(
                in_flight.load(Ordering::SeqCst),
                0,
                "all in-flight workers must drain to zero before stage seam exits"
            );
            assert_eq!(
                err, "failure-at-0",
                "lowest-index error must win deterministically even when idx 1 fails earlier"
            );
            assert!(
                started.load(Ordering::SeqCst) < items.len(),
                "cooperative abort must prevent draining the full unstarted tail"
            );
        }
    }
}
