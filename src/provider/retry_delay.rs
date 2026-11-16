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
    let trimmed = input.trim();
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let (year, month, day, hms) = match parts.as_slice() {
        [_, day_s, mon_s, year_s, time_s, "GMT"] => (
            year_s.parse::<i64>().ok()?,
            parse_month(mon_s)?,
            day_s.parse::<u32>().ok()?,
            *time_s,
        ),
        [_, dmy_s, time_s, "GMT"] => {
            let mut segs = dmy_s.split('-');
            let day = segs.next()?.parse::<u32>().ok()?;
            let month = parse_month(segs.next()?)?;
            let two_yr = segs.next()?.parse::<i64>().ok()?;
            let year = if two_yr >= 70 {
                1900 + two_yr
            } else {
                2000 + two_yr
            };
            (year, month, day, *time_s)
        }
        [_, mon_s, day_s, time_s, year_s] => (
            year_s.parse::<i64>().ok()?,
            parse_month(mon_s)?,
            day_s.parse::<u32>().ok()?,
            *time_s,
        ),
        _ => return None,
    };

    let (hour, min, sec) = parse_hms(hms)?;
    if !(1..=31).contains(&day) || year < 1970 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    if days < 0 {
        return None;
    }
    let total_secs = (days as u64)
        .checked_mul(86_400)?
        .checked_add(u64::from(hour) * 3_600 + u64::from(min) * 60 + u64::from(sec))?;
    total_secs.checked_mul(1_000)
}

fn parse_month(mon: &str) -> Option<u32> {
    match mon {
        "Jan" => Some(1),
        "Feb" => Some(2),
        "Mar" => Some(3),
        "Apr" => Some(4),
        "May" => Some(5),
        "Jun" => Some(6),
        "Jul" => Some(7),
        "Aug" => Some(8),
        "Sep" => Some(9),
        "Oct" => Some(10),
        "Nov" => Some(11),
        "Dec" => Some(12),
        _ => None,
    }
}

fn parse_hms(hms: &str) -> Option<(u32, u32, u32)> {
    let mut it = hms.split(':');
    let h: u32 = it.next()?.parse().ok()?;
    let m: u32 = it.next()?.parse().ok()?;
    let s: u32 = it.next()?.parse().ok()?;
    if it.next().is_some() || h > 23 || m > 59 || s > 60 {
        return None;
    }
    Some((h, m, s))
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = year - i64::from(month <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m = u64::from(month);
    let d = u64::from(day);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + (doe as i64) - 719_468
}
