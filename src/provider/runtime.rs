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

//! Persistent background Tokio reactor and hardened pooled HTTP transport helpers.

use super::gemini_wire::JSON_MIME_TYPE;
use crate::provider::ProviderError;
use serde::Serialize;
use std::sync::{OnceLock, mpsc};
use std::time::{Duration, Instant};

/// Default TCP + TLS connection establishment timeout for pooled provider HTTP clients.
pub(crate) const DEFAULT_HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Default idle retention window for pooled keep-alive sockets (`< 60s` NAT/GFE eviction).
pub(crate) const DEFAULT_HTTP_POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Default idle delay before OS-level TCP keep-alive probes begin on pooled sockets.
pub(crate) const DEFAULT_TCP_KEEPALIVE_IDLE: Duration = Duration::from_secs(10);

/// Default interval between OS-level TCP keep-alive probes (`10s + 3 * 5s = 25s < 60s`).
pub(crate) const DEFAULT_TCP_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);

/// Default unacknowledged TCP keep-alive probe count before declaring a socket dead.
pub(crate) const DEFAULT_TCP_KEEPALIVE_RETRIES: u32 = 3;

static HTTP_REACTOR: OnceLock<Result<tokio::runtime::Handle, String>> = OnceLock::new();

/// Raw status, decoded UTF-8 body, and elapsed wall-clock latency from an outbound JSON `POST`.
pub(crate) struct RawHttpOutcome {
    pub(crate) status_code: u16,
    pub(crate) body: String,
    pub(crate) latency_ms: u64,
}

/// Outbound authentication header scheme for a provider HTTP request.
pub(crate) enum HttpAuth<'a> {
    /// Google Generative Language API `x-goog-api-key` header.
    GoogApiKey(&'a str),
    /// Standard `Authorization: Bearer <token>` header.
    Bearer(&'a str),
}

/// Outbound JSON `POST` envelope parameters executed on the persistent background reactor.
pub(crate) struct JsonPostRequest<'a> {
    pub(crate) url: &'a str,
    pub(crate) model_specifier: &'a str,
    pub(crate) auth: HttpAuth<'a>,
    pub(crate) timeout_ms: u64,
}

/// Builds a connection-pooled [`reqwest::Client`] hardened against stale idle sockets,
/// silent NAT blackholes, and blackholed TCP/TLS handshakes.
pub(crate) fn build_pooled_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(DEFAULT_HTTP_CONNECT_TIMEOUT)
        .pool_idle_timeout(DEFAULT_HTTP_POOL_IDLE_TIMEOUT)
        .tcp_keepalive(DEFAULT_TCP_KEEPALIVE_IDLE)
        .tcp_keepalive_interval(DEFAULT_TCP_KEEPALIVE_INTERVAL)
        .tcp_keepalive_retries(DEFAULT_TCP_KEEPALIVE_RETRIES)
        .build()
        .unwrap_or_default()
}

fn spawn_background_http_reactor() -> Result<tokio::runtime::Handle, String> {
    let (init_tx, init_rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("canon-clerk-http".to_string())
        .spawn(move || {
            match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => {
                    let handle = runtime.handle().clone();
                    if init_tx.send(Ok(handle)).is_ok() {
                        runtime.block_on(std::future::pending::<()>());
                    }
                }
                Err(err) => {
                    let _ = init_tx.send(Err(err.to_string()));
                }
            }
        })
        .map_err(|err| err.to_string())?;

    init_rx
        .recv()
        .map_err(|err| format!("background HTTP reactor exited during startup ({err})"))?
}

fn http_reactor_handle(
    model_specifier: &str,
) -> Result<&'static tokio::runtime::Handle, (ProviderError, u64)> {
    match HTTP_REACTOR.get_or_init(spawn_background_http_reactor) {
        Ok(handle) => Ok(handle),
        Err(err) => Err((
            ProviderError::Transport {
                model_specifier: model_specifier.to_string(),
                message: format!("failed to initialize persistent async HTTP runtime ({err})"),
            },
            0,
        )),
    }
}

fn classify_reqwest_error(
    err: &reqwest::Error,
    model_specifier: &str,
    timeout_ms: u64,
) -> ProviderError {
    if err.is_connect() {
        ProviderError::Transport {
            model_specifier: model_specifier.to_string(),
            message: err.to_string(),
        }
    } else if err.is_timeout() {
        ProviderError::Timeout {
            model_specifier: model_specifier.to_string(),
            timeout_ms,
        }
    } else {
        ProviderError::Transport {
            model_specifier: model_specifier.to_string(),
            message: err.to_string(),
        }
    }
}

/// Serializes `payload` as JSON on the caller thread and drives the HTTP `POST` + full body read
/// on the persistent background Tokio reactor so pooled connections never bind to caller threads.
pub(crate) fn execute_json_post<T: Serialize>(
    client: &reqwest::Client,
    req: &JsonPostRequest<'_>,
    payload: &T,
) -> Result<RawHttpOutcome, (ProviderError, u64)> {
    let start = Instant::now();
    let body_bytes = serde_json::to_vec(payload).map_err(|err| {
        let latency_ms = start.elapsed().as_millis() as u64;
        (
            ProviderError::Transport {
                model_specifier: req.model_specifier.to_string(),
                message: format!("failed to serialize JSON request payload ({err})"),
            },
            latency_ms,
        )
    })?;

    let builder = client
        .post(req.url)
        .timeout(Duration::from_millis(req.timeout_ms))
        .header("Content-Type", JSON_MIME_TYPE);
    let builder = match req.auth {
        HttpAuth::GoogApiKey(key) => builder.header("x-goog-api-key", key),
        HttpAuth::Bearer(token) => builder.bearer_auth(token),
    };
    let http_request = builder.body(body_bytes).build().map_err(|err| {
        let latency_ms = start.elapsed().as_millis() as u64;
        (
            classify_reqwest_error(&err, req.model_specifier, req.timeout_ms),
            latency_ms,
        )
    })?;

    let handle = http_reactor_handle(req.model_specifier)?;
    let client_clone = client.clone();
    let model_specifier = req.model_specifier.to_string();
    let timeout_ms = req.timeout_ms;
    let (done_tx, done_rx) = mpsc::sync_channel(1);

    handle.spawn(async move {
        let outcome = async {
            let response = client_clone.execute(http_request).await.map_err(|err| {
                let latency_ms = start.elapsed().as_millis() as u64;
                (
                    classify_reqwest_error(&err, &model_specifier, timeout_ms),
                    latency_ms,
                )
            })?;

            let status_code = response.status().as_u16();
            let body = response.text().await.map_err(|err| {
                let latency_ms = start.elapsed().as_millis() as u64;
                (
                    classify_reqwest_error(&err, &model_specifier, timeout_ms),
                    latency_ms,
                )
            })?;
            let latency_ms = start.elapsed().as_millis() as u64;

            Ok(RawHttpOutcome {
                status_code,
                body,
                latency_ms,
            })
        }
        .await;
        let _ = done_tx.send(outcome);
    });

    done_rx.recv().map_err(|_| {
        let latency_ms = start.elapsed().as_millis() as u64;
        (
            ProviderError::Transport {
                model_specifier: req.model_specifier.to_string(),
                message: "background HTTP request task terminated unexpectedly".to_string(),
            },
            latency_ms,
        )
    })?
}
