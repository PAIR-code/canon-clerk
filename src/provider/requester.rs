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

//! Centralized pooled HTTP requester coordinating per-origin cooldown gates and bounded transient retries
//! (`docs/architecture/transport.md` §2–§4).

use super::retry::{
    HttpErrorSample, HttpRetryDisposition, RetryNotification, RetryPolicy,
    classify_http_retry_disposition, classify_transport_retry_reason,
    compute_exponential_backoff_ms, enrich_http_status_error, extract_origin_key_and_host,
};
use super::runtime::{
    HttpAuth, OwnedJsonPostRequest, RawHttpOutcome, build_pooled_http_client, execute_wire_attempt,
    http_reactor_handle, serialize_json_payload,
};
use crate::provider::ProviderError;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Optional borrowed callback invoked immediately before sleeping on a cooldown or retry backoff.
pub type RetryObserver<'a> = &'a (dyn Fn(&RetryNotification) + Send + Sync);

/// Outbound JSON `POST` specification executed through [`SharedHttpRequester`].
pub(crate) struct RetryablePostRequest<'a> {
    pub(crate) url: &'a str,
    pub(crate) model_specifier: &'a str,
    pub(crate) auth: HttpAuth<'a>,
    pub(crate) timeout_ms: u64,
    pub(crate) call_label: Option<&'a str>,
    pub(crate) format_http_error: fn(u16, &str, &str) -> ProviderError,
}

#[derive(Debug, Clone, Copy)]
struct OriginCooldownState {
    cooldown_until: Instant,
    next_slot_at: Instant,
}

impl OriginCooldownState {
    fn new_expired(now: Instant) -> Self {
        Self {
            cooldown_until: now,
            next_slot_at: now,
        }
    }

    fn acquire_pre_dispatch_slot(
        &mut self,
        now: Instant,
        slot_spacing: Duration,
    ) -> Option<Instant> {
        if now >= self.cooldown_until && now >= self.next_slot_at {
            return None;
        }
        let base = self.cooldown_until.max(now);
        let release_at = self.next_slot_at.max(base);
        self.next_slot_at = release_at + slot_spacing;
        Some(release_at)
    }

    fn acquire_extended_cooldown_slot(
        &mut self,
        now: Instant,
        slot_spacing: Duration,
    ) -> Option<Instant> {
        if now >= self.cooldown_until {
            return None;
        }
        let release_at = self.next_slot_at.max(self.cooldown_until);
        self.next_slot_at = release_at + slot_spacing;
        Some(release_at)
    }

    fn record_origin_cooldown(
        &mut self,
        now: Instant,
        delay: Duration,
        slot_spacing: Duration,
    ) -> Instant {
        let candidate_until = now + delay;
        self.cooldown_until = self.cooldown_until.max(candidate_until);
        let release_at = if self.next_slot_at < self.cooldown_until {
            self.cooldown_until
        } else {
            self.next_slot_at
        };
        self.next_slot_at = release_at + slot_spacing;
        release_at
    }
}

struct RequesterInner {
    client: reqwest::Client,
    policy: RetryPolicy,
    origins: Mutex<BTreeMap<String, OriginCooldownState>>,
    aborted: AtomicBool,
    abort_notify: tokio::sync::Notify,
    jitter_counter: AtomicU64,
}

enum ReactorControlMsg {
    Retry(RetryNotification),
    Done(Result<RawHttpOutcome, ProviderError>),
}

/// Cloneable pooled HTTP transport handle shared across generative and discriminative provider adapters.
///
/// Coordinates per-origin `429`/`503` pre-dispatch cooldown gates, monotonic deadline coalescing,
/// staggered worker release, wire-only per-attempt timeouts, and bounded transient retries on the
/// persistent background Tokio reactor.
#[derive(Clone)]
pub struct SharedHttpRequester {
    inner: Arc<RequesterInner>,
}

impl Default for SharedHttpRequester {
    fn default() -> Self {
        Self::new(RetryPolicy::default())
    }
}

impl fmt::Debug for SharedHttpRequester {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SharedHttpRequester")
            .field("policy", &self.inner.policy)
            .finish()
    }
}

impl SharedHttpRequester {
    /// Constructs a [`SharedHttpRequester`] with a hardened connection-pooled [`reqwest::Client`] and `policy`.
    pub fn new(policy: RetryPolicy) -> Self {
        Self::with_client(build_pooled_http_client(), policy)
    }

    /// Constructs a [`SharedHttpRequester`] wrapping an explicit [`reqwest::Client`] and `policy`.
    pub fn with_client(client: reqwest::Client, policy: RetryPolicy) -> Self {
        Self {
            inner: Arc::new(RequesterInner {
                client,
                policy,
                origins: Mutex::new(BTreeMap::new()),
                aborted: AtomicBool::new(false),
                abort_notify: tokio::sync::Notify::new(),
                jitter_counter: AtomicU64::new(1),
            }),
        }
    }

    /// Returns a reference to the underlying pooled [`reqwest::Client`].
    pub fn client(&self) -> &reqwest::Client {
        &self.inner.client
    }

    /// Returns the configured [`RetryPolicy`].
    pub fn retry_policy(&self) -> RetryPolicy {
        self.inner.policy
    }

    /// Resets cooperative stage-abort state prior to or following a bounded stage worker scope.
    pub fn clear_abort(&self) {
        self.inner.aborted.store(false, Ordering::Release);
    }

    /// Signals cooperative stage abort and immediately wakes all cooldown- or backoff-parked tasks.
    pub fn notify_abort(&self) {
        self.inner.aborted.store(true, Ordering::Release);
        self.inner.abort_notify.notify_waiters();
    }

    pub(crate) fn execute_json_post<T: Serialize>(
        &self,
        req: &RetryablePostRequest<'_>,
        payload: &T,
        on_retry: Option<RetryObserver<'_>>,
    ) -> Result<RawHttpOutcome, ProviderError> {
        let body_bytes =
            serialize_json_payload(payload, req.model_specifier).map_err(|(err, _)| err)?;
        let handle = http_reactor_handle(req.model_specifier).map_err(|(err, _)| err)?;

        let owned_req = OwnedJsonPostRequest {
            url: req.url.to_string(),
            model_specifier: req.model_specifier.to_string(),
            auth: req.auth.into_owned(),
            timeout_ms: req.timeout_ms,
            call_label: req
                .call_label
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned),
        };
        let format_http_error = req.format_http_error;
        let emit_retries = on_retry.is_some();
        let inner = Arc::clone(&self.inner);
        let (tx, rx) = mpsc::channel();

        handle.spawn(async move {
            let result = drive_retryable_post(
                &inner,
                &owned_req,
                &body_bytes,
                format_http_error,
                emit_retries.then_some(&tx),
            )
            .await;
            let _ = tx.send(ReactorControlMsg::Done(result));
        });

        while let Ok(msg) = rx.recv() {
            match msg {
                ReactorControlMsg::Retry(notice) => {
                    if let Some(observer) = on_retry {
                        observer(&notice);
                    }
                }
                ReactorControlMsg::Done(result) => return result,
            }
        }

        Err(ProviderError::Transport {
            model_specifier: req.model_specifier.to_string(),
            endpoint_host: None,
            message: "background HTTP request task terminated unexpectedly".to_string(),
        })
    }
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

async fn sleep_until_cancellable(
    inner: &RequesterInner,
    deadline: Instant,
    model_specifier: &str,
) -> Result<(), ProviderError> {
    use std::future::Future;
    use std::task::Poll;

    let notified = inner.abort_notify.notified();
    tokio::pin!(notified);
    if inner.aborted.load(Ordering::Acquire) {
        return Err(aborted_error(model_specifier));
    }
    if deadline <= Instant::now() {
        return Ok(());
    }
    let sleep = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline));
    tokio::pin!(sleep);

    std::future::poll_fn(|cx| {
        if inner.aborted.load(Ordering::Acquire) || notified.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(aborted_error(model_specifier)));
        }
        match sleep.as_mut().poll(cx) {
            Poll::Ready(()) => {
                if inner.aborted.load(Ordering::Acquire) {
                    Poll::Ready(Err(aborted_error(model_specifier)))
                } else {
                    Poll::Ready(Ok(()))
                }
            }
            Poll::Pending => Poll::Pending,
        }
    })
    .await
}

fn aborted_error(model_specifier: &str) -> ProviderError {
    ProviderError::Transport {
        model_specifier: model_specifier.to_string(),
        endpoint_host: None,
        message: "stage aborted while waiting on provider retry cooldown".to_string(),
    }
}

fn acquire_gate_deadline(
    inner: &RequesterInner,
    origin_key: &str,
    now: Instant,
    first_attempt: bool,
) -> Option<Instant> {
    let slot_spacing = Duration::from_millis(inner.policy.effective_slot_spacing_ms());
    let mut guard = inner.origins.lock().unwrap_or_else(PoisonError::into_inner);
    let state = guard
        .entry(origin_key.to_string())
        .or_insert_with(|| OriginCooldownState::new_expired(now));
    if first_attempt {
        state.acquire_pre_dispatch_slot(now, slot_spacing)
    } else {
        state.acquire_extended_cooldown_slot(now, slot_spacing)
    }
}

fn coalesce_origin_cooldown(
    inner: &RequesterInner,
    origin_key: &str,
    now: Instant,
    wait_ms: u64,
) -> Instant {
    let slot_spacing = Duration::from_millis(inner.policy.effective_slot_spacing_ms());
    let mut guard = inner.origins.lock().unwrap_or_else(PoisonError::into_inner);
    let state = guard
        .entry(origin_key.to_string())
        .or_insert_with(|| OriginCooldownState::new_expired(now));
    state.record_origin_cooldown(now, Duration::from_millis(wait_ms), slot_spacing)
}

fn emit_retry_notice(tx: Option<&mpsc::Sender<ReactorControlMsg>>, notice: RetryNotification) {
    if let Some(sender) = tx
        && notice.wait_ms > 0
    {
        let _ = sender.send(ReactorControlMsg::Retry(notice));
    }
}

struct AttemptContext<'a> {
    inner: &'a RequesterInner,
    req: &'a OwnedJsonPostRequest,
    origin_key: &'a str,
    display_host: &'a str,
    format_http_error: fn(u16, &str, &str) -> ProviderError,
    retry_tx: Option<&'a mpsc::Sender<ReactorControlMsg>>,
}

async fn drive_retryable_post(
    inner: &RequesterInner,
    req: &OwnedJsonPostRequest,
    body_bytes: &[u8],
    format_http_error: fn(u16, &str, &str) -> ProviderError,
    retry_tx: Option<&mpsc::Sender<ReactorControlMsg>>,
) -> Result<RawHttpOutcome, ProviderError> {
    let (origin_key, display_host) = extract_origin_key_and_host(&req.url);
    let ctx = AttemptContext {
        inner,
        req,
        origin_key: &origin_key,
        display_host: &display_host,
        format_http_error,
        retry_tx,
    };
    let mut completed_attempts: u32 = 0;

    loop {
        if inner.aborted.load(Ordering::Acquire) {
            return Err(aborted_error(&req.model_specifier));
        }

        let now = Instant::now();
        if let Some(gate_deadline) =
            acquire_gate_deadline(inner, &origin_key, now, completed_attempts == 0)
        {
            let wait_ms = gate_deadline.saturating_duration_since(now).as_millis() as u64;
            emit_retry_notice(
                retry_tx,
                RetryNotification {
                    label: req.call_label.clone(),
                    host: display_host.clone(),
                    attempt: completed_attempts,
                    max_retries: inner.policy.max_retries,
                    wait_ms,
                    reason: "origin rate-limit cooldown active".to_string(),
                },
            );
            sleep_until_cancellable(inner, gate_deadline, &req.model_specifier).await?;
        }

        completed_attempts = completed_attempts.saturating_add(1);
        let wire_res = execute_wire_attempt(&inner.client, req, body_bytes.to_vec()).await;
        if let Some(outcome) = handle_wire_outcome(&ctx, completed_attempts, wire_res).await? {
            return Ok(outcome);
        }
    }
}

async fn handle_wire_outcome(
    ctx: &AttemptContext<'_>,
    completed_attempts: u32,
    wire_res: Result<RawHttpOutcome, (ProviderError, u64)>,
) -> Result<Option<RawHttpOutcome>, ProviderError> {
    let policy = ctx.inner.policy;
    match wire_res {
        Ok(outcome) if (200..300).contains(&outcome.status_code) => Ok(Some(outcome)),
        Ok(outcome) => {
            let seed = ctx.inner.jitter_counter.fetch_add(1, Ordering::Relaxed);
            let disposition = classify_http_retry_disposition(
                &HttpErrorSample {
                    status_code: outcome.status_code,
                    retry_after_header: outcome.retry_after.as_deref(),
                    body: &outcome.body,
                    completed_attempts,
                    now_unix_ms: current_unix_ms(),
                    jitter_seed: seed,
                },
                &policy,
            );
            match disposition {
                HttpRetryDisposition::FailFast { diagnostic_suffix } => {
                    let base_err = (ctx.format_http_error)(
                        outcome.status_code,
                        &ctx.req.model_specifier,
                        &outcome.body,
                    );
                    Err(enrich_http_status_error(
                        base_err,
                        diagnostic_suffix.as_deref(),
                    ))
                }
                HttpRetryDisposition::Retry {
                    wait_ms,
                    update_origin_cooldown,
                    reason,
                } => {
                    let post_now = Instant::now();
                    let deadline = if update_origin_cooldown {
                        coalesce_origin_cooldown(ctx.inner, ctx.origin_key, post_now, wait_ms)
                    } else {
                        post_now + Duration::from_millis(wait_ms)
                    };
                    let actual_wait_ms =
                        deadline.saturating_duration_since(post_now).as_millis() as u64;
                    emit_retry_notice(
                        ctx.retry_tx,
                        RetryNotification {
                            label: ctx.req.call_label.clone(),
                            host: ctx.display_host.to_string(),
                            attempt: completed_attempts,
                            max_retries: policy.max_retries,
                            wait_ms: actual_wait_ms,
                            reason,
                        },
                    );
                    sleep_until_cancellable(ctx.inner, deadline, &ctx.req.model_specifier).await?;
                    Ok(None)
                }
            }
        }
        Err((err, _latency_ms)) => {
            let Some(reason) = classify_transport_retry_reason(&err) else {
                return Err(err);
            };
            if completed_attempts > policy.max_retries {
                return Err(err);
            }
            let seed = ctx.inner.jitter_counter.fetch_add(1, Ordering::Relaxed);
            let wait_ms = compute_exponential_backoff_ms(&policy, completed_attempts, seed);
            let post_now = Instant::now();
            let deadline = post_now + Duration::from_millis(wait_ms);
            emit_retry_notice(
                ctx.retry_tx,
                RetryNotification {
                    label: ctx.req.call_label.clone(),
                    host: ctx.display_host.to_string(),
                    attempt: completed_attempts,
                    max_retries: policy.max_retries,
                    wait_ms,
                    reason,
                },
            );
            sleep_until_cancellable(ctx.inner, deadline, &ctx.req.model_specifier).await?;
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::concurrency::execute_bounded_stage_work;
    use crate::provider::{
        DecisionModelProvider, DecisionRequest, GeminiDecisionProvider, GeminiModelProvider,
        GenerationRequest, MultiModelProvider, StructuredModelProvider,
    };
    use crate::telemetry::{
        PipelineStage, RecordingTelemetrySink, SteppingTelemetryClock, TelemetryEvent,
        TelemetrySession,
    };
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::AtomicUsize;
    use std::thread;

    fn read_http_request(stream: &mut TcpStream) -> String {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let mut raw = Vec::new();
        let mut buf = [0_u8; 4096];
        loop {
            match stream.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    raw.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&raw);
                    if let Some((head, body)) = text.split_once("\r\n\r\n") {
                        let expected_len = head
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        if body.len() >= expected_len {
                            return text.into_owned();
                        }
                    }
                }
            }
        }
        String::from_utf8_lossy(&raw).into_owned()
    }

    fn write_http_response(
        stream: &mut TcpStream,
        status_line: &str,
        extra_headers: &[(&str, &str)],
        body: &str,
    ) {
        let mut resp = format!(
            "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n",
            body.len()
        );
        for (k, v) in extra_headers {
            resp.push_str(&format!("{k}: {v}\r\n"));
        }
        resp.push_str("\r\n");
        resp.push_str(body);
        let _ = stream.write_all(resp.as_bytes());
        let _ = stream.flush();
    }

    mod cooldown_gate_and_concurrency {
        use super::*;

        #[test]
        fn test_origin_cooldown_state_coalesces_monotonically_and_staggers_slots() {
            let t0 = Instant::now();
            let slot = Duration::from_millis(20);
            let mut state = OriginCooldownState::new_expired(t0);

            assert!(state.acquire_pre_dispatch_slot(t0, slot).is_none());

            let w1 = state.record_origin_cooldown(t0, Duration::from_millis(100), slot);
            assert_eq!(w1, t0 + Duration::from_millis(100));

            let w3 = state
                .acquire_pre_dispatch_slot(t0 + Duration::from_millis(5), slot)
                .expect("must wait on active cooldown");
            assert_eq!(w3, t0 + Duration::from_millis(120));

            let w2 = state.record_origin_cooldown(
                t0 + Duration::from_millis(10),
                Duration::from_millis(98),
                slot,
            );
            assert_eq!(state.cooldown_until, t0 + Duration::from_millis(108));
            assert_eq!(w2, t0 + Duration::from_millis(140));

            assert!(
                state
                    .acquire_pre_dispatch_slot(t0 + Duration::from_millis(200), slot)
                    .is_none()
            );
        }

        fn spawn_coalescing_429_server(
            listener: TcpListener,
            hits_ref: Arc<AtomicUsize>,
        ) -> thread::JoinHandle<Vec<u64>> {
            thread::spawn(move || {
                let start = Instant::now();
                let mut ok_timestamps_ms = Vec::new();
                while ok_timestamps_ms.len() < 4 {
                    let (mut stream, _) = listener.accept().expect("accept");
                    let _ = read_http_request(&mut stream);
                    let hit = hits_ref.fetch_add(1, Ordering::SeqCst) + 1;
                    if hit <= 2 {
                        let err_body = r#"{"error":{"message":"Rate limit exceeded (limit: 1000000 input tokens per minute). Please retry in 0.08s."}}"#;
                        write_http_response(&mut stream, "429 Too Many Requests", &[], err_body);
                    } else {
                        ok_timestamps_ms.push(start.elapsed().as_millis() as u64);
                        let ok_body = r#"{"model":"gemini-decision-1","answers":{"q_0":{"type":"noul","noul":0.91}},"usage":{"total_input_tokens":50,"total_output_tokens":0}}"#;
                        write_http_response(&mut stream, "200 OK", &[], ok_body);
                    }
                }
                ok_timestamps_ms
            })
        }

        #[test]
        fn test_four_worker_429_coalesces_gates_predispatch_and_emits_telemetry() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");
            let total_hits = Arc::new(AtomicUsize::new(0));
            let server = spawn_coalescing_429_server(listener, Arc::clone(&total_hits));

            let policy = RetryPolicy::new(2, 5_000).with_base_and_slot_ms(20, 15);
            let requester = SharedHttpRequester::new(policy);
            let base_url = format!("http://{addr}/v1beta");
            let multi = MultiModelProvider::with_providers(
                Some(GeminiModelProvider::with_requester_and_base_url(
                    "test-key",
                    requester.clone(),
                    &base_url,
                )),
                Some(GeminiDecisionProvider::with_requester_and_base_url(
                    "test-key", requester, &base_url,
                )),
                None,
            );

            let sink = RecordingTelemetrySink::new();
            let clock = SteppingTelemetryClock::new(1_767_225_600_000, 5);
            let session = TelemetrySession::new(&sink, &clock);
            let items = ["c0.md", "c1.md", "c2.md", "c3.md"];

            let run_start = Instant::now();
            let ordered = session
                .run_stage(PipelineStage::Vet, |scope| {
                    let instrumented = scope.instrument_provider(&multi);
                    execute_bounded_stage_work(&items, 4, &instrumented, |idx, canon| {
                        // Stagger arrivals slightly so workers 2 & 3 hit the pre-dispatch gate before Attempt 1!
                        if idx >= 2 {
                            thread::sleep(Duration::from_millis(12));
                        }
                        let req = DecisionRequest {
                            model_specifier: "google:gemini-decision-1".to_string(),
                            state: format!("diff-{idx}"),
                            questions: BTreeMap::from([(
                                (*canon).to_string(),
                                "Complies?".to_string(),
                            )]),
                            timeout_ms: 1_000,
                            call_label: Some((*canon).to_string()),
                        };
                        let resp = instrumented.evaluate_decisions(&req)?;
                        Ok::<_, ProviderError>((
                            (*canon, resp.scores[*canon]),
                            (*canon).to_string(),
                        ))
                    })
                })
                .expect("all 4 workers must recover and succeed");
            let elapsed = run_start.elapsed();

            let ok_times = server.join().expect("server joined");
            assert_eq!(
                ordered,
                vec![
                    ("c0.md", 0.91),
                    ("c1.md", 0.91),
                    ("c2.md", 0.91),
                    ("c3.md", 0.91),
                ],
                "stage outputs must fold in deterministic input order"
            );
            assert_eq!(
                total_hits.load(Ordering::SeqCst),
                6,
                "workers arriving during cooldown must pause at pre-dispatch gate before Attempt 1 (2 x 429 + 4 x 200 = 6 total hits)"
            );
            assert!(
                elapsed < Duration::from_millis(600),
                "coalesced 80ms cooldown must not stack multiplicatively across 4 workers (elapsed: {elapsed:?})"
            );
            assert!(
                ok_times.last().copied().unwrap_or(0)
                    >= ok_times.first().copied().unwrap_or(0) + 20,
                "staggered slot release must spread post-cooldown dispatches: {ok_times:?}"
            );

            let retry_events: Vec<_> = sink
                .events()
                .into_iter()
                .filter(|ev| matches!(ev, TelemetryEvent::ProviderRetry { .. }))
                .collect();
            assert!(
                retry_events.len() >= 3,
                "ProviderRetry telemetry must record both 429 retries and pre-dispatch gate waits: {retry_events:?}"
            );
        }

        #[test]
        fn test_predispatch_cooldown_wait_does_not_consume_per_attempt_wire_timeout() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");

            let server = thread::spawn(move || {
                // Request 1: triggers 95ms Retry-After cooldown (fails fast for caller 1 since max_retries=1 on call 1? No, caller 1 also retries after 95ms!).
                for idx in 1..=2 {
                    let (mut stream, _) = listener.accept().expect("accept");
                    let _ = read_http_request(&mut stream);
                    if idx == 1 {
                        write_http_response(
                            &mut stream,
                            "429 Too Many Requests",
                            &[("Retry-After", "0.095")],
                            r#"{"error":{"message":"throttled"}}"#,
                        );
                    } else {
                        thread::sleep(Duration::from_millis(15));
                        let ok_body = r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"{\"ok\":true}"}]},"finishReason":"STOP"}],"modelVersion":"gemini-4-carbon"}"#;
                        write_http_response(&mut stream, "200 OK", &[], ok_body);
                    }
                }
            });

            let policy = RetryPolicy::new(2, 2_000).with_base_and_slot_ms(20, 5);
            let requester = SharedHttpRequester::new(policy);
            let provider = GeminiModelProvider::with_requester_and_base_url(
                "test-key",
                requester,
                format!("http://{addr}/v1beta"),
            );

            // timeout_ms is 65ms — SMALLER than the 95ms Retry-After cooldown sleep!
            // If cooldown sleep consumed timeout_ms, Attempt 2 would time out before hitting the wire.
            let req = GenerationRequest {
                model_specifier: "google:gemini-4-carbon".to_string(),
                system_instruction: "Check".to_string(),
                user_prompt: "Diff".to_string(),
                response_schema: json!({"type": "object"}),
                thinking_level: None,
                reasoning_budget: None,
                timeout_ms: 65,
                call_label: Some("canon.md".to_string()),
            };

            let resp = provider.generate_structured(&req).expect(
                "wire timeout must start strictly on wire dispatch, not during 95ms cooldown sleep",
            );
            assert_eq!(resp.payload, json!({"ok": true}));
            server.join().expect("server joined");
        }
    }

    mod transient_recovery_and_fast_fail {
        use super::*;

        #[test]
        fn test_transient_timeout_and_503_recover_on_retry() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");

            let server = thread::spawn(move || {
                for attempt in 1..=3 {
                    let (mut stream, _) = listener.accept().expect("accept");
                    let _ = read_http_request(&mut stream);
                    match attempt {
                        1 => {
                            // Exceed 45ms wire timeout on Attempt 1.
                            thread::sleep(Duration::from_millis(90));
                        }
                        2 => {
                            write_http_response(
                                &mut stream,
                                "503 Service Unavailable",
                                &[("Retry-After", "0.02")],
                                r#"{"error":{"message":"backend overloaded"}}"#,
                            );
                        }
                        _ => {
                            let ok_body = r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"{\"compliance_score\":0.95}"}]},"finishReason":"STOP"}],"modelVersion":"gemini-4-carbon"}"#;
                            write_http_response(&mut stream, "200 OK", &[], ok_body);
                        }
                    }
                }
            });

            let policy = RetryPolicy::new(2, 2_000).with_base_and_slot_ms(15, 5);
            let provider = GeminiModelProvider::with_requester_and_base_url(
                "test-key",
                SharedHttpRequester::new(policy),
                format!("http://{addr}/v1beta"),
            );
            let notices = Mutex::new(Vec::new());
            let req = GenerationRequest {
                model_specifier: "google:gemini-4-carbon".to_string(),
                system_instruction: String::new(),
                user_prompt: "ping".to_string(),
                response_schema: json!({"type": "object"}),
                thinking_level: None,
                reasoning_budget: None,
                timeout_ms: 45,
                call_label: Some("slice 1/2".to_string()),
            };

            let observer = |n: &RetryNotification| {
                notices.lock().expect("lock").push(n.clone());
            };
            let resp = provider
                .generate_structured_observed(&req, Some(&observer))
                .expect("must recover on Attempt 3");
            assert_eq!(resp.payload["compliance_score"], json!(0.95));

            let recorded = notices.into_inner().expect("notices");
            assert_eq!(recorded.len(), 2);
            assert!(recorded[0].reason.contains("wire timeout"));
            assert!(recorded[1].reason.contains("HTTP 503"));
            server.join().expect("server joined");
        }

        #[test]
        fn test_non_retryable_4xx_and_hard_quota_fail_fast_single_shot() {
            for (status, body) in [
                (
                    "400 Bad Request",
                    r#"{"error":{"message":"max_tokens_exceeded"}}"#,
                ),
                (
                    "401 Unauthorized",
                    r#"{"error":{"message":"invalid api key"}}"#,
                ),
                (
                    "403 Forbidden",
                    r#"{"error":{"message":"permission denied"}}"#,
                ),
                (
                    "404 Not Found",
                    r#"{"error":{"message":"model not found"}}"#,
                ),
                (
                    "429 Too Many Requests",
                    r#"{"error":{"message":"Quota exceeded: limit: 0 for model"}}"#,
                ),
            ] {
                let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
                let addr = listener.local_addr().expect("local addr");
                let hits = Arc::new(AtomicUsize::new(0));
                let hits_ref = Arc::clone(&hits);

                let server = thread::spawn(move || {
                    let (mut stream, _) = listener.accept().expect("accept single attempt");
                    let _ = read_http_request(&mut stream);
                    hits_ref.fetch_add(1, Ordering::SeqCst);
                    write_http_response(&mut stream, status, &[("Retry-After", "5")], body);
                });

                let provider = GeminiDecisionProvider::with_requester_and_base_url(
                    "test-key",
                    SharedHttpRequester::new(RetryPolicy::new(2, 90_000)),
                    format!("http://{addr}/v1beta"),
                );
                let req = DecisionRequest {
                    model_specifier: "google:gemini-decision-1".to_string(),
                    state: "diff".to_string(),
                    questions: BTreeMap::from([("c.md".to_string(), "ok?".to_string())]),
                    timeout_ms: 500,
                    call_label: None,
                };

                let start = Instant::now();
                let err = provider
                    .evaluate_decisions(&req)
                    .expect_err("non-retryable status must fail");
                assert!(
                    start.elapsed() < Duration::from_millis(300),
                    "must not sleep 5s Retry-After on {status}"
                );
                assert_eq!(hits.load(Ordering::SeqCst), 1, "{status} must not retry");
                assert!(err.to_string().contains("Hint:"));
                server.join().expect("server joined");
            }
        }

        #[test]
        fn test_cooperative_stage_abort_wakes_cooldown_parked_sibling_immediately() {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");

            let server = thread::spawn(move || {
                // Accept both already-in-flight workers before sending 429 so worker-1 is in-flight
                // simultaneously when worker-0 enters its 3s cooldown sleep.
                let (mut s_a, _) = listener.accept().expect("accept first");
                let raw_a = read_http_request(&mut s_a);
                let (mut s_b, _) = listener.accept().expect("accept second");
                let raw_b = read_http_request(&mut s_b);

                let (mut s_429, mut s_401) = if raw_a.contains("worker-0") {
                    (s_a, s_b)
                } else {
                    debug_assert!(raw_b.contains("worker-0"));
                    (s_b, s_a)
                };

                write_http_response(
                    &mut s_429,
                    "429 Too Many Requests",
                    &[("Retry-After", "3")],
                    r#"{"error":{"message":"Please retry in 3s"}}"#,
                );
                thread::sleep(Duration::from_millis(15));
                write_http_response(
                    &mut s_401,
                    "401 Unauthorized",
                    &[],
                    r#"{"error":{"message":"bad key"}}"#,
                );
            });

            let requester = SharedHttpRequester::new(RetryPolicy::new(2, 10_000));
            let multi = MultiModelProvider::with_providers(
                None,
                Some(GeminiDecisionProvider::with_requester_and_base_url(
                    "test-key",
                    requester,
                    format!("http://{addr}/v1beta"),
                )),
                None,
            );

            let items = ["worker-0", "worker-1"];
            let start = Instant::now();
            let err = execute_bounded_stage_work(&items, 2, &multi, |_idx, tag| {
                let req = DecisionRequest {
                    model_specifier: "google:gemini-decision-1".to_string(),
                    state: (*tag).to_string(),
                    questions: BTreeMap::from([("c.md".to_string(), "ok?".to_string())]),
                    timeout_ms: 1_000,
                    call_label: Some((*tag).to_string()),
                };
                let resp = StructuredModelProvider::evaluate_decisions(&multi, &req)?;
                Ok::<_, ProviderError>((resp.resolved_model, (*tag).to_string()))
            })
            .expect_err("stage must abort when worker-1 hits 401");

            assert!(
                start.elapsed() < Duration::from_millis(800),
                "notify_abort must wake worker-0 out of its 3s cooldown sleep immediately"
            );
            assert!(err.to_string().contains("Hint:"));
            server.join().expect("server joined");
        }
    }
}
