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

//! Multi-source upstream retry-delay extraction (`Retry-After` RFC 9110 headers, JSON `RetryInfo`, and message hints).

use serde_json::Value;
use std::time::UNIX_EPOCH;

/// Extracts an upstream-advised retry delay in milliseconds across Tiers 1–3 in descending precedence:
/// 1. HTTP `Retry-After` response header (delta-seconds or RFC 9110 HTTP-date).
/// 2. Structured JSON metadata (`google.rpc.RetryInfo.retryDelay`, `retry_after`, `retry_after_ms`).
/// 3. Embedded diagnostic message hints (`"Please retry in <N>s"`, `"retry after <N>ms"`, etc.).
pub(super) fn extract_advised_retry_delay_ms(
    retry_after_header: Option<&str>,
    body: &str,
    now_unix_ms: u64,
) -> Option<u64> {
    if let Some(header) = retry_after_header
        && let Some(ms) = parse_retry_after_header(header, now_unix_ms)
    {
        return Some(ms);
    }
    if let Some(ms) = parse_json_retry_delay_ms(body) {
        return Some(ms);
    }
    parse_message_retry_delay_ms(body)
}

fn parse_non_negative_seconds_to_ms(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if !trimmed.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let secs: f64 = trimmed.parse().ok()?;
    if !secs.is_finite() || secs < 0.0 {
        return None;
    }
    Some((secs * 1000.0).ceil() as u64)
}

fn parse_duration_token_ms(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if let Some(ms_part) = trimmed.strip_suffix("ms") {
        let val: f64 = ms_part.trim().parse().ok()?;
        return (val.is_finite() && val >= 0.0).then(|| val.ceil() as u64);
    }
    if let Some(s_part) = trimmed.strip_suffix('s') {
        return parse_non_negative_seconds_to_ms(s_part);
    }
    parse_non_negative_seconds_to_ms(trimmed)
}

fn parse_json_retry_delay_ms(body: &str) -> Option<u64> {
    let parsed: Value = serde_json::from_str(body).ok()?;
    let containers = [parsed.get("error"), Some(&parsed)];

    for container in containers.into_iter().flatten() {
        if let Some(details) = container.get("details").and_then(Value::as_array) {
            for item in details {
                if let Some(delay_str) = item.get("retryDelay").and_then(Value::as_str)
                    && let Some(ms) = parse_duration_token_ms(delay_str)
                {
                    return Some(ms);
                }
            }
        }
        for key in ["retry_after_ms", "retryAfterMs"] {
            if let Some(v) = container.get(key) {
                if let Some(ms) = v.as_f64().filter(|n| n.is_finite() && *n >= 0.0) {
                    return Some(ms.ceil() as u64);
                }
                if let Some(s) = v.as_str()
                    && let Some(ms) = parse_duration_token_ms(s)
                {
                    return Some(ms);
                }
            }
        }
        for key in ["retry_after", "retryAfter"] {
            if let Some(v) = container.get(key) {
                if let Some(secs) = v.as_f64().filter(|n| n.is_finite() && *n >= 0.0) {
                    return Some((secs * 1000.0).ceil() as u64);
                }
                if let Some(s) = v.as_str()
                    && let Some(ms) = parse_duration_token_ms(s)
                {
                    return Some(ms);
                }
            }
        }
    }
    None
}

fn parse_message_retry_delay_ms(body: &str) -> Option<u64> {
    let lower = body.to_ascii_lowercase();
    for prefix in ["please retry in ", "retry after ", "retry in "] {
        let mut search_slice = lower.as_str();
        while let Some(idx) = search_slice.find(prefix) {
            let after = search_slice[idx + prefix.len()..].trim_start();
            if let Some(ms) = parse_leading_number_and_unit_ms(after) {
                return Some(ms);
            }
            search_slice = &search_slice[idx + prefix.len()..];
        }
    }
    None
}

fn parse_leading_number_and_unit_ms(text: &str) -> Option<u64> {
    let bytes = text.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let mut end_num = 0;
    let mut seen_dot = false;
    while end_num < bytes.len() {
        let b = bytes[end_num];
        if b.is_ascii_digit() {
            end_num += 1;
        } else if b == b'.' && !seen_dot {
            seen_dot = true;
            end_num += 1;
        } else {
            break;
        }
    }
    let val: f64 = text[..end_num].parse().ok()?;
    if !val.is_finite() || val < 0.0 {
        return None;
    }

    let unit_slice = text[end_num..].trim_start();
    let unit_len = unit_slice
        .bytes()
        .take_while(u8::is_ascii_alphabetic)
        .count();
    if unit_len == 0 {
        return None;
    }
    let multiplier_ms = match &unit_slice[..unit_len] {
        "ms" | "millisecond" | "milliseconds" => 1.0,
        "s" | "sec" | "secs" | "second" | "seconds" => 1_000.0,
        "m" | "min" | "mins" | "minute" | "minutes" => 60_000.0,
        _ => return None,
    };
    Some((val * multiplier_ms).ceil() as u64)
}

fn parse_retry_after_header(header: &str, now_unix_ms: u64) -> Option<u64> {
    let trimmed = header.trim();
    if let Some(ms) = parse_non_negative_seconds_to_ms(trimmed) {
        return Some(ms);
    }
    let target_unix_ms = parse_http_date_unix_ms(trimmed)?;
    Some(target_unix_ms.saturating_sub(now_unix_ms))
}

/// Parses RFC 9110 §5.6.7 HTTP-date formats (`IMF-fixdate`, `rfc850-date`, `asctime-date`) into UTC epoch ms.
pub(crate) fn parse_http_date_unix_ms(input: &str) -> Option<u64> {
    let system_time = httpdate::parse_http_date(input.trim()).ok()?;
    let duration = system_time.duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(duration.as_millis()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_http_date_formats_and_rejects_invalid_calendar_dates() {
        // IMF-fixdate: 2026-11-16T00:00:00Z = 1_794_787_200_000 ms
        assert_eq!(
            parse_http_date_unix_ms("Mon, 16 Nov 2026 00:00:00 GMT"),
            Some(1_794_787_200_000)
        );
        // RFC 850 obsolete format: 1994-11-06T08:49:37Z = 784_111_777_000 ms
        assert_eq!(
            parse_http_date_unix_ms("Sunday, 06-Nov-94 08:49:37 GMT"),
            Some(784_111_777_000)
        );
        // ANSI C asctime() format: 1994-11-06T08:49:37Z
        assert_eq!(
            parse_http_date_unix_ms("Sun Nov  6 08:49:37 1994"),
            Some(784_111_777_000)
        );
        // Invalid calendar dates must be rejected rather than rolled forward.
        assert_eq!(
            parse_http_date_unix_ms("Sat, 31 Feb 2026 00:00:00 GMT"),
            None
        );
        assert_eq!(
            parse_http_date_unix_ms("Sun, 29 Feb 2026 00:00:00 GMT"),
            None
        );
    }
}
