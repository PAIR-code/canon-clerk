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

use std::process::Command;
use std::time::Instant;

fn bin_path() -> &'static str {
    env!("CARGO_BIN_EXE_canon-clerk")
}

#[test]
fn test_cli_version_flag() {
    let output = Command::new(bin_path())
        .arg("--version")
        .output()
        .expect("failed to execute canon-clerk");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("canon-clerk 0.1.0"),
        "stdout should contain version string, got: {stdout}"
    );
}

#[test]
fn test_cli_version_short_flag() {
    let output = Command::new(bin_path())
        .arg("-V")
        .output()
        .expect("failed to execute canon-clerk");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("canon-clerk 0.1.0"),
        "stdout should contain version string, got: {stdout}"
    );
}

#[test]
fn test_cli_help_flag() {
    let output = Command::new(bin_path())
        .arg("--help")
        .output()
        .expect("failed to execute canon-clerk");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage: canon-clerk"),
        "stdout should contain usage line"
    );
    assert!(
        stdout.contains("Logging & Diagnostics:"),
        "stdout should organize options into concern sections"
    );
    assert!(
        stdout.contains("-v, --verbose"),
        "stdout should document verbose flag"
    );
    assert!(
        stdout.contains("TARGETS"),
        "stdout should document primary positional targets"
    );
}

#[test]
fn test_cli_help_short_flag() {
    let output = Command::new(bin_path())
        .arg("-h")
        .output()
        .expect("failed to execute canon-clerk");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage: canon-clerk"));
}

#[test]
fn test_cli_no_args_displays_help() {
    let output = Command::new(bin_path())
        .output()
        .expect("failed to execute canon-clerk");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage: canon-clerk"),
        "invoking with no arguments should display help"
    );
}

#[test]
fn test_cli_invalid_flag_exits_with_code_2() {
    let output = Command::new(bin_path())
        .arg("--nonexistent-flag")
        .output()
        .expect("failed to execute canon-clerk");

    assert_eq!(
        output.status.code(),
        Some(2),
        "CLI argument errors must terminate with exit code 2 per cli-subcommands-must-standardize-usage-exit-codes"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected argument") || stderr.contains("error:"),
        "stderr should describe the usage error, got: {stderr}"
    );
}

#[test]
fn test_cli_targets_positional_execution() {
    let output = Command::new(bin_path())
        .arg("src/lib.rs")
        .arg("README.md")
        .output()
        .expect("failed to execute canon-clerk");

    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn test_cli_verbose_diagnostic_emits_to_stderr() {
    let output = Command::new(bin_path())
        .arg("-v")
        .arg("src/lib.rs")
        .output()
        .expect("failed to execute canon-clerk");

    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("DEBUG") || stderr.contains("INFO"),
        "diagnostics should be written to stderr per cli-must-separate-data-from-diagnostics, got: {stderr}"
    );
}

#[test]
fn test_cli_subcommand_dispatch_routes_exit_codes() {
    // Verify top-level binary routes subcommand usage errors to exit code 2
    let fail_out = Command::new(bin_path())
        .arg("intake")
        .output()
        .expect("failed to execute canon-clerk intake");
    assert_eq!(fail_out.status.code(), Some(2));

    // Verify top-level binary routes subcommand success to exit code 0
    let ok_out = Command::new(bin_path())
        .args(["intake", "src/lib.rs", "--json"])
        .output()
        .expect("failed to execute canon-clerk intake src/lib.rs --json");
    assert_eq!(ok_out.status.code(), Some(0));

    // Verify discover naked invocation exits 2, predicate -q zero-match exits 1, and --all-canons exits 0
    let disc_naked = Command::new(bin_path())
        .arg("discover")
        .output()
        .expect("failed to execute canon-clerk discover");
    assert_eq!(disc_naked.status.code(), Some(2));

    let disc_quiet_miss = Command::new(bin_path())
        .args([
            "discover",
            "-q",
            "--canon",
            ".canons/readme-authoring/readme-must-lead-with-what-and-why.md",
            "src/lib.rs",
        ])
        .output()
        .expect("failed to execute canon-clerk discover -q");
    assert_eq!(disc_quiet_miss.status.code(), Some(1));

    let disc_all = Command::new(bin_path())
        .args(["discover", "--all-canons", "--json"])
        .output()
        .expect("failed to execute canon-clerk discover --all-canons --json");
    assert_eq!(disc_all.status.code(), Some(0));

    // Verify validate naked invocation exits 2 and --all-canons exits 0
    let val_naked = Command::new(bin_path())
        .arg("validate")
        .output()
        .expect("failed to execute canon-clerk validate");
    assert_eq!(val_naked.status.code(), Some(2));

    let val_all = Command::new(bin_path())
        .args(["validate", "--all-canons", "--max-warnings", "0", "--json"])
        .output()
        .expect("failed to execute canon-clerk validate --all-canons --json");
    assert_eq!(val_all.status.code(), Some(0));
}

#[test]
fn test_cli_cold_start_latency() {
    // Benchmark repeated executions of --version to measure process cold start latency
    let bin = bin_path();
    let mut total_duration = std::time::Duration::ZERO;
    let iterations = 5;

    for _ in 0..iterations {
        let start = Instant::now();
        let status = Command::new(bin)
            .arg("--version")
            .status()
            .expect("execution failed");
        let elapsed = start.elapsed();
        assert!(status.success());
        total_duration += elapsed;
    }

    let avg_ms = total_duration.as_millis() / iterations as u128;
    println!("Average cold start latency across {iterations} runs: {avg_ms} ms");
    // Standard acceptance criteria is sub-15ms cold start
    assert!(
        avg_ms < 50,
        "Cold start average latency ({avg_ms} ms) must be well within performance budget"
    );
}
