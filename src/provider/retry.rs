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

//! Retry policy, quota guardrails, and failure classification (`docs/architecture/transport.md` §4).

pub use super::retry_delay::extract_advised_retry_delay_ms;
use crate::provider::ProviderError;

/// Default maximum retry attempts per heuristic provider call (`2` retries = up to `3` total attempts).
pub const DEFAULT_MAX_RETRIES: u32 = 2;

/// Default maximum per-wait backoff duration in milliseconds (`90,000ms` = `90s` for rolling 60s TPM windows).
pub const DEFAULT_MAX_BACKOFF_MS: u64 = 90_000;

/// Environment variable name for overriding `--max-retries`.
pub const ENV_MAX_RETRIES: &str = "CANON_CLERK_MAX_RETRIES";

/// Environment variable name for overriding `--max-backoff-ms`.
pub const ENV_MAX_BACKOFF_MS: &str = "CANON_CLERK_MAX_BACKOFF_MS";

/// Default initial backoff duration in milliseconds when no upstream retry delay is advised.
pub const DEFAULT_BASE_BACKOFF_MS: u64 = 1_000;

/// Default staggered slot spacing in milliseconds when releasing multiple cooldown-parked workers.
pub const DEFAULT_COOLDOWN_SLOT_MS: u64 = 150;

/// Bounded retry and rate-limit cooldown policy applied by [`super::SharedHttpRequester`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Maximum retry attempts after Attempt 1 (`0` disables retries).
    pub max_retries: u32,
    /// Maximum permitted wait duration per cooldown or backoff sleep in milliseconds.
    pub max_backoff_ms: u64,
    /// Base delay in milliseconds for jittered exponential backoff (`1,000ms` -> `3,000ms`).
    pub base_backoff_ms: u64,
    /// Staggered release slot spacing in milliseconds across concurrent cooldown-parked workers.
    pub slot_spacing_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            max_backoff_ms: DEFAULT_MAX_BACKOFF_MS,
            base_backoff_ms: DEFAULT_BASE_BACKOFF_MS,
            slot_spacing_ms: DEFAULT_COOLDOWN_SLOT_MS,
        }
    }
}

impl RetryPolicy {
    /// Constructs a [`RetryPolicy`] with explicit `max_retries` and `max_backoff_ms` bounds.
    pub const fn new(max_retries: u32, max_backoff_ms: u64) -> Self {
        Self {
            max_retries,
            max_backoff_ms,
            base_backoff_ms: DEFAULT_BASE_BACKOFF_MS,
            slot_spacing_ms: DEFAULT_COOLDOWN_SLOT_MS,
        }
    }

    /// Overrides base exponential backoff and staggered cooldown slot spacing (useful for sub-second loopback tests).
    pub const fn with_base_and_slot_ms(
        mut self,
        base_backoff_ms: u64,
        slot_spacing_ms: u64,
    ) -> Self {
        self.base_backoff_ms = base_backoff_ms;
        self.slot_spacing_ms = slot_spacing_ms;
        self
    }

    pub(crate) fn effective_slot_spacing_ms(&self) -> u64 {
        if self.max_backoff_ms == 0 {
            0
        } else {
            self.slot_spacing_ms.min((self.max_backoff_ms / 4).max(1))
        }
    }
}

/// Out-of-band operational notice emitted immediately prior to sleeping on an origin cooldown or retry backoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryNotification {
    /// Optional concise work-unit identifier (e.g., `canon_path` or batch/slice label).
    pub label: Option<String>,
    /// Normalized target origin host (`generativelanguage.googleapis.com` or `127.0.0.1:<port>`).
    pub host: String,
    /// Retry attempt index (`0` for pre-dispatch origin cooldown waits before Attempt 1; `1..=max_retries` for retries).
    pub attempt: u32,
    /// Configured maximum retries per call.
    pub max_retries: u32,
    /// Planned wait duration in milliseconds.
    pub wait_ms: u64,
    /// Concise human-readable reason for the wait.
    pub reason: String,
}

/// Disposition computed for a non-2xx HTTP response under an active [`RetryPolicy`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HttpRetryDisposition {
    /// Response is transient; sleep `wait_ms` (and optionally coalesce origin cooldown) before retrying.
    Retry {
        wait_ms: u64,
        update_origin_cooldown: bool,
        reason: String,
    },
    /// Response is non-recoverable or retries are exhausted; fail immediately with zero sleep.
    FailFast { diagnostic_suffix: Option<String> },
}

/// Extracts `(origin_bucket_key, display_host)` from a request URL (`scheme://host[:port]`).
pub(crate) fn extract_origin_key_and_host(url: &str) -> (String, String) {
    if let Ok(parsed) = reqwest::Url::parse(url)
        && let Some(host) = parsed.host_str()
    {
        let host_lower = host.to_ascii_lowercase();
        let display_host = match parsed.port() {
            Some(port) => format!("{host_lower}:{port}"),
            None => host_lower,
        };
        let origin_key = format!("{}://{display_host}", parsed.scheme());
        return (origin_key, display_host);
    }
    (url.to_string(), url.to_string())
}

/// Computes bounded jittered exponential backoff (`base * 3^(attempt - 1) + jitter`) capped at `policy.max_backoff_ms`.
pub(crate) fn compute_exponential_backoff_ms(
    policy: &RetryPolicy,
    retry_attempt: u32,
    jitter_seed: u64,
) -> u64 {
    if policy.max_backoff_ms == 0 {
        return 0;
    }
    let exp = 3_u64.saturating_pow(retry_attempt.saturating_sub(1));
    let base = policy.base_backoff_ms.saturating_mul(exp);
    let jitter = if policy.base_backoff_ms == 0 {
        0
    } else {
        let span = (policy.base_backoff_ms / 4).max(1);
        jitter_seed % (span + 1)
    };
    base.saturating_add(jitter).min(policy.max_backoff_ms)
}

/// Returns `true` when an upstream `429` payload signals non-recoverable hard/daily/billing quota exhaustion.
pub fn is_hard_quota_exhausted(body: &str) -> bool {
    let compact: String = body
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    let lower = body.to_ascii_lowercase();

    if contains_zero_limit_token(&lower) {
        return true;
    }
    if compact.contains("\"quotalimitvalue\":\"0\"")
        || compact.contains("\"quota_limit_value\":\"0\"")
    {
        return true;
    }

    const HARD_QUOTA_MARKERS: [&str; 10] = [
        "perday",
        "per_day",
        "per day",
        "requestsperday",
        "tokensperday",
        "daily quota",
        "daily limit",
        "insufficient_quota",
        "billing hard limit",
        "plan and billing",
    ];
    HARD_QUOTA_MARKERS.iter().any(|m| lower.contains(m))
}

fn contains_zero_limit_token(lower: &str) -> bool {
    let needle = "limit: 0";
    let mut rest = lower;
    while let Some(pos) = rest.find(needle) {
        let after = &rest[pos + needle.len()..];
        let next_ch = after.chars().next();
        if !next_ch.is_some_and(|c| c.is_ascii_digit() || c == '.') {
            return true;
        }
        rest = after;
    }
    false
}

/// Upstream non-2xx HTTP response context evaluated against [`RetryPolicy`].
pub(crate) struct HttpErrorSample<'a> {
    pub(crate) status_code: u16,
    pub(crate) retry_after_header: Option<&'a str>,
    pub(crate) body: &'a str,
    pub(crate) completed_attempts: u32,
    pub(crate) now_unix_ms: u64,
    pub(crate) jitter_seed: u64,
}

/// Classifies an HTTP error status and payload into retry vs. fast-fail under `policy`.
pub(crate) fn classify_http_retry_disposition(
    sample: &HttpErrorSample<'_>,
    policy: &RetryPolicy,
) -> HttpRetryDisposition {
    let status_code = sample.status_code;
    if status_code == 429 && is_hard_quota_exhausted(sample.body) {
        return HttpRetryDisposition::FailFast {
            diagnostic_suffix: Some(
                "Non-recoverable daily or `limit: 0` billing quota exhausted; automatic retry skipped. \
                 Hint: Upgrade your provider quota/billing tier or switch models via `--screener-model` / `--auditor-model`."
                    .to_string(),
            ),
        };
    }

    let is_retryable_status = matches!(status_code, 429 | 500 | 502 | 503 | 504);
    if !is_retryable_status {
        return HttpRetryDisposition::FailFast {
            diagnostic_suffix: None,
        };
    }

    let advised_ms =
        extract_advised_retry_delay_ms(sample.retry_after_header, sample.body, sample.now_unix_ms);
    if let Some(delay_ms) = advised_ms
        && delay_ms > policy.max_backoff_ms
    {
        return HttpRetryDisposition::FailFast {
            diagnostic_suffix: Some(format!(
                "Upstream advised retry wait ({:.1}s) exceeds `--max-backoff-ms {}` ({:.1}s); failing fast without sleeping. \
                 Hint: Wait for the quota window to reset, pass a higher `--max-backoff-ms <MS>` (env: `{ENV_MAX_BACKOFF_MS}`), or upgrade your quota tier.",
                delay_ms as f64 / 1000.0,
                policy.max_backoff_ms,
                policy.max_backoff_ms as f64 / 1000.0,
            )),
        };
    }

    if sample.completed_attempts > policy.max_retries
        || (status_code == 429 && policy.max_backoff_ms == 0)
    {
        return HttpRetryDisposition::FailFast {
            diagnostic_suffix: None,
        };
    }

    let wait_ms = advised_ms.unwrap_or_else(|| {
        compute_exponential_backoff_ms(policy, sample.completed_attempts, sample.jitter_seed)
    });
    let update_origin_cooldown = status_code == 429 || (status_code == 503 && advised_ms.is_some());
    let reason = if status_code == 429 {
        "HTTP 429 rate limit".to_string()
    } else {
        format!("HTTP {status_code} upstream server error")
    };

    HttpRetryDisposition::Retry {
        wait_ms,
        update_origin_cooldown,
        reason,
    }
}

/// Returns `Some(reason)` when a [`ProviderError`] from a failed wire attempt is transient and retryable.
pub(crate) fn classify_transport_retry_reason(err: &ProviderError) -> Option<String> {
    match err {
        ProviderError::Timeout { timeout_ms, .. } => Some(format!("wire timeout ({timeout_ms}ms)")),
        ProviderError::Transport { message, .. } => {
            let lower = message.to_ascii_lowercase();
            let non_retryable = [
                "failed to serialize json",
                "failed to initialize persistent async http runtime",
                "stage aborted",
                "connection refused",
                "certificate",
                "builder error",
            ];
            if non_retryable.iter().any(|m| lower.contains(m)) {
                None
            } else {
                Some("transient network/transport error".to_string())
            }
        }
        _ => None,
    }
}

/// Appends `suffix` to a [`ProviderError::HttpStatus`] diagnostic when fast-failing on quota/backoff bounds.
pub(crate) fn enrich_http_status_error(
    mut err: ProviderError,
    suffix: Option<&str>,
) -> ProviderError {
    if let (ProviderError::HttpStatus { diagnostic, .. }, Some(extra)) = (&mut err, suffix) {
        diagnostic.push(' ');
        diagnostic.push_str(extra);
    }
    err
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::retry_delay::parse_http_date_unix_ms;
    use serde_json::json;

    mod multi_source_delay_parsing {
        use super::*;

        #[test]
        fn test_precedence_retry_after_header_seconds_and_http_date() {
            // 2026-11-16T00:00:00Z = 1794787200000 ms
            let base_ms = parse_http_date_unix_ms("Mon, 16 Nov 2026 00:00:00 GMT")
                .expect("parse IMF-fixdate");
            let future_hdr = "Mon, 16 Nov 2026 00:00:56 GMT";

            // Header overrides conflicting body RetryInfo.
            let body = json!({
                "error": {
                    "message": "Please retry in 12s.",
                    "details": [{
                        "@type": "type.googleapis.com/google.rpc.RetryInfo",
                        "retryDelay": "25s"
                    }]
                }
            })
            .to_string();

            assert_eq!(
                extract_advised_retry_delay_ms(Some("42"), &body, base_ms),
                Some(42_000)
            );
            assert_eq!(
                extract_advised_retry_delay_ms(Some("0.08"), &body, base_ms),
                Some(80)
            );
            assert_eq!(
                extract_advised_retry_delay_ms(Some(future_hdr), &body, base_ms),
                Some(56_000)
            );
            // Past HTTP-date clamps cleanly to 0ms.
            assert_eq!(
                extract_advised_retry_delay_ms(Some(future_hdr), "", base_ms + 90_000),
                Some(0)
            );
        }

        #[test]
        fn test_structured_google_retry_info_and_message_fallbacks() {
            let retry_info_body = json!({
                "error": {
                    "code": 429,
                    "message": "Resource has been exhausted. Please retry in 10s.",
                    "details": [{
                        "@type": "type.googleapis.com/google.rpc.RetryInfo",
                        "retryDelay": "56.125s"
                    }]
                }
            })
            .to_string();
            assert_eq!(
                extract_advised_retry_delay_ms(None, &retry_info_body, 0),
                Some(56_125)
            );

            let msg_only_body = json!({
                "error": {
                    "message": "Rate limit exceeded for model gemini-decision-1 (limit: 1000000 input tokens per minute on Tier 1). Please retry in 56s or upgrade your tier at https://ai.dev/rate-limit."
                }
            })
            .to_string();
            assert_eq!(
                extract_advised_retry_delay_ms(None, &msg_only_body, 0),
                Some(56_000)
            );

            assert_eq!(
                extract_advised_retry_delay_ms(None, "Throttled: please retry in 0.08s.", 0),
                Some(80)
            );
            assert_eq!(
                extract_advised_retry_delay_ms(None, "Overloaded, retry after 250ms", 0),
                Some(250)
            );
        }
    }

    mod quota_guardrails_and_classification {
        use super::*;

        #[test]
        fn test_hard_quota_and_max_backoff_ceiling_fail_fast_without_sleep() {
            let policy = RetryPolicy::default();

            let zero_limit_body = r#"{"error":{"message":"Quota exceeded for metric: limit: 0, model: gemini-4-carbon"}}"#;
            assert!(is_hard_quota_exhausted(zero_limit_body));
            assert!(matches!(
                classify_http_retry_disposition(
                    &HttpErrorSample {
                        status_code: 429,
                        retry_after_header: None,
                        body: zero_limit_body,
                        completed_attempts: 1,
                        now_unix_ms: 0,
                        jitter_seed: 0,
                    },
                    &policy,
                ),
                HttpRetryDisposition::FailFast {
                    diagnostic_suffix: Some(_)
                }
            ));

            let daily_body = r#"{"error":{"message":"GenerateRequestsPerDayPerProjectPerModel quota exceeded"}}"#;
            assert!(is_hard_quota_exhausted(daily_body));
            assert!(matches!(
                classify_http_retry_disposition(
                    &HttpErrorSample {
                        status_code: 429,
                        retry_after_header: None,
                        body: daily_body,
                        completed_attempts: 1,
                        now_unix_ms: 0,
                        jitter_seed: 0,
                    },
                    &policy,
                ),
                HttpRetryDisposition::FailFast {
                    diagnostic_suffix: Some(_)
                }
            ));

            // 120s exceeds default 90s ceiling -> fast-fail on Attempt 1.
            let over_cap = r#"{"error":{"message":"Please retry in 120s"}}"#;
            assert!(matches!(
                classify_http_retry_disposition(
                    &HttpErrorSample {
                        status_code: 429,
                        retry_after_header: None,
                        body: over_cap,
                        completed_attempts: 1,
                        now_unix_ms: 0,
                        jitter_seed: 0,
                    },
                    &policy,
                ),
                HttpRetryDisposition::FailFast {
                    diagnostic_suffix: Some(_)
                }
            ));

            // 56s TPM window is <= 90s ceiling -> retryable + updates origin cooldown.
            let tpm_body = r#"{"error":{"message":"limit: 1000000 input tokens per minute. Please retry in 56s"}}"#;
            assert!(!is_hard_quota_exhausted(tpm_body));
            assert_eq!(
                classify_http_retry_disposition(
                    &HttpErrorSample {
                        status_code: 429,
                        retry_after_header: None,
                        body: tpm_body,
                        completed_attempts: 1,
                        now_unix_ms: 0,
                        jitter_seed: 0,
                    },
                    &policy,
                ),
                HttpRetryDisposition::Retry {
                    wait_ms: 56_000,
                    update_origin_cooldown: true,
                    reason: "HTTP 429 rate limit".to_string(),
                }
            );
        }

        #[test]
        fn test_origin_authority_normalizes_paths_and_isolates_loopback_ports() {
            let (k1, h1) = extract_origin_key_and_host(
                "https://generativelanguage.googleapis.com/v1beta/models/gemini-4-carbon:generateContent",
            );
            let (k2, h2) = extract_origin_key_and_host(
                "https://generativelanguage.googleapis.com/v1beta/decisions",
            );
            assert_eq!(k1, "https://generativelanguage.googleapis.com");
            assert_eq!(k1, k2);
            assert_eq!(h1, "generativelanguage.googleapis.com");
            assert_eq!(h1, h2);

            let (p1, _) = extract_origin_key_and_host("http://127.0.0.1:41001/v1beta/decisions");
            let (p2, _) = extract_origin_key_and_host("http://127.0.0.1:41002/v1beta/decisions");
            assert_ne!(p1, p2);
        }
    }
}
