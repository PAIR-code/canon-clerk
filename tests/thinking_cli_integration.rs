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

fn bin_path() -> &'static str {
    env!("CARGO_BIN_EXE_canon-clerk")
}

#[test]
fn test_cli_conflicting_or_invalid_thinking_flags_exit_with_code_2() {
    let conflict_audit = Command::new(bin_path())
        .args([
            "audit",
            "Cargo.toml",
            "--thinking-level",
            "low",
            "--reasoning-budget",
            "2048",
        ])
        .env("XDG_CONFIG_HOME", "/nonexistent-canon-clerk-xdg-dir")
        .output()
        .expect("conflicting thinking flags on audit");
    assert_eq!(conflict_audit.status.code(), Some(2));
    let stderr_conflict = String::from_utf8_lossy(&conflict_audit.stderr);
    assert!(stderr_conflict.contains("mutually exclusive"));
    assert!(stderr_conflict.contains("Hint:"));

    let invalid_level = Command::new(bin_path())
        .args(["configure", "--thinking-level", "turbo"])
        .env("XDG_CONFIG_HOME", "/nonexistent-canon-clerk-xdg-dir")
        .output()
        .expect("invalid thinking level on configure");
    assert_eq!(invalid_level.status.code(), Some(2));
    let stderr_invalid = String::from_utf8_lossy(&invalid_level.stderr);
    assert!(stderr_invalid.contains("Invalid thinking level"));
    assert!(stderr_invalid.contains("Hint:"));
}
