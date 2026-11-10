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

//! Explicit clock source port and ISO-8601 UTC timestamp formatting for out-of-band telemetry.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Hexagonal clock port supplying UTC wall-clock timestamps and monotonic elapsed milliseconds.
pub trait TelemetryClock: Send + Sync {
    /// Returns the current wall-clock time formatted as an ISO-8601 UTC string (`YYYY-MM-DDTHH:MM:SS.sssZ`).
    fn now_iso8601_utc(&self) -> String;

    /// Returns monotonic elapsed milliseconds relative to an arbitrary process or test origin.
    fn monotonic_ms(&self) -> u64;
}

/// Zero-cost placeholder clock paired with [`super::NoopTelemetrySink`].
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopTelemetryClock;

impl TelemetryClock for NoopTelemetryClock {
    fn now_iso8601_utc(&self) -> String {
        String::new()
    }

    fn monotonic_ms(&self) -> u64 {
        0
    }
}

/// Host operating system clock constructed by top-level CLI entrypoints.
#[derive(Debug)]
pub struct SystemTelemetryClock {
    origin: Instant,
}

impl SystemTelemetryClock {
    /// Captures a monotonic baseline `Instant` for elapsed stage and pipeline duration measurement.
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemTelemetryClock {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryClock for SystemTelemetryClock {
    fn now_iso8601_utc(&self) -> String {
        let epoch_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        format_unix_millis_iso8601(epoch_ms)
    }

    fn monotonic_ms(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// Deterministic stepping clock for hermetic unit and integration tests.
#[derive(Debug)]
pub struct SteppingTelemetryClock {
    base_epoch_ms: u128,
    step_ms: u64,
    ticks: AtomicU64,
}

impl SteppingTelemetryClock {
    /// Constructs a deterministic clock starting at `base_epoch_ms` and advancing by `step_ms` per tick.
    pub const fn new(base_epoch_ms: u128, step_ms: u64) -> Self {
        Self {
            base_epoch_ms,
            step_ms,
            ticks: AtomicU64::new(0),
        }
    }
}

impl TelemetryClock for SteppingTelemetryClock {
    fn now_iso8601_utc(&self) -> String {
        let current = self.ticks.load(Ordering::SeqCst);
        let offset = u128::from(current.saturating_mul(self.step_ms));
        format_unix_millis_iso8601(self.base_epoch_ms.saturating_add(offset))
    }

    fn monotonic_ms(&self) -> u64 {
        let tick = self.ticks.fetch_add(1, Ordering::SeqCst);
        tick.saturating_mul(self.step_ms)
    }
}

/// Formats milliseconds since the Unix epoch (`1970-01-01T00:00:00.000Z`) into an ISO-8601 UTC string.
///
/// Uses Howard Hinnant's proleptic Gregorian `civil_from_days` algorithm to convert UTC days
/// without ambient locale or timezone dependencies.
pub fn format_unix_millis_iso8601(epoch_ms: u128) -> String {
    let millis = epoch_ms % 1_000;
    let total_secs = epoch_ms / 1_000;
    let secs_of_day = total_secs % 86_400;
    let days_since_epoch = i64::try_from(total_secs / 86_400).unwrap_or(i64::MAX);

    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let second = secs_of_day % 60;

    let (year, month, day) = civil_from_days(days_since_epoch);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i64, u64, u64) {
    let z = days_since_unix_epoch.saturating_add(719_468);
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = u64::try_from(z - era * 146_097).unwrap_or(0);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = i64::try_from(yoe).unwrap_or(0) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod iso8601_utc_formatting {
        use super::*;

        #[test]
        fn test_formats_epoch_and_known_calendar_dates() {
            assert_eq!(format_unix_millis_iso8601(0), "1970-01-01T00:00:00.000Z");
            // 2000-02-29T12:34:56.789Z (leap century)
            assert_eq!(
                format_unix_millis_iso8601(951_827_696_789),
                "2000-02-29T12:34:56.789Z"
            );
            // 2026-11-09T20:05:18.042Z
            assert_eq!(
                format_unix_millis_iso8601(1_794_254_718_042),
                "2026-11-09T20:05:18.042Z"
            );
        }
    }

    mod stepping_clock_determinism {
        use super::*;

        #[test]
        fn test_stepping_clock_advances_monotonically() {
            // Base: 2026-01-01T00:00:00.000Z = 1_767_225_600_000 ms
            let clock = SteppingTelemetryClock::new(1_767_225_600_000, 25);
            assert_eq!(clock.now_iso8601_utc(), "2026-01-01T00:00:00.000Z");
            assert_eq!(clock.monotonic_ms(), 0);
            assert_eq!(clock.now_iso8601_utc(), "2026-01-01T00:00:00.025Z");
            assert_eq!(clock.monotonic_ms(), 25);
            assert_eq!(clock.monotonic_ms(), 50);
        }
    }
}
