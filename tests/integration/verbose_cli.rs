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

use crate::common::bin_path;
use std::process::Command;

#[test]
fn test_cli_verbose_suppresses_reqwest_and_hyper_debug_unless_rust_log_set() {
    // 1. Without `RUST_LOG`, `-v` scopes debug tracing to `canon_clerk` and omits
    //    `reqwest::connect` / `hyper_util::client` socket spam on stderr.
    let scoped = Command::new(bin_path())
        .args(["probe", "-v", "--timeout", "150", "--json"])
        .env("GEMINI_API_KEY", "test-dummy-key")
        .env("XDG_CONFIG_HOME", "/nonexistent-canon-clerk-xdg-dir")
        .env_remove("RUST_LOG")
        .env("NO_COLOR", "1")
        .output()
        .expect("execute canon-clerk probe -v");

    let scoped_stderr = String::from_utf8_lossy(&scoped.stderr);
    assert!(
        scoped_stderr.contains("Initialized Canon Clerk CLI runtime"),
        "expected canon_clerk debug log on stderr, got: {scoped_stderr}"
    );
    assert!(
        scoped_stderr.contains("[telemetry] -> probe started"),
        "expected stage telemetry on stderr, got: {scoped_stderr}"
    );
    assert!(
        !scoped_stderr.contains("reqwest::connect")
            && !scoped_stderr.contains("hyper_util::client"),
        "expected zero reqwest/hyper_util debug lines without RUST_LOG, got: {scoped_stderr}"
    );

    // 2. Explicit `RUST_LOG=reqwest=debug` opt-in surfaces `reqwest::connect` debug output.
    let explicit = Command::new(bin_path())
        .args(["probe", "-v", "--timeout", "150", "--json"])
        .env("GEMINI_API_KEY", "test-dummy-key")
        .env("XDG_CONFIG_HOME", "/nonexistent-canon-clerk-xdg-dir")
        .env("RUST_LOG", "reqwest=debug")
        .env("NO_COLOR", "1")
        .output()
        .expect("execute canon-clerk probe -v with RUST_LOG=reqwest=debug");

    let explicit_stderr = String::from_utf8_lossy(&explicit.stderr);
    assert!(
        explicit_stderr.contains("reqwest::connect"),
        "expected explicit RUST_LOG=reqwest=debug to surface reqwest::connect, got: {explicit_stderr}"
    );
}
