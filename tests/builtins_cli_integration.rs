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

mod common;

use canon_clerk::Caseload;
use std::process::Command;

fn bin_path() -> &'static str {
    env!("CARGO_BIN_EXE_canon-clerk")
}

#[test]
fn test_cli_builtins_toggle_filtering_and_caseload_roundtrip() {
    let ws = common::TempCanonWorkspace::new("builtins-roundtrip");

    // 1. Default discovery includes both local workspace canons and embedded builtin:* canons
    let with_builtins = Command::new(bin_path())
        .current_dir(ws.path())
        .args(["discover", "--all-canons", "--json"])
        .output()
        .expect("discover --all-canons");
    assert_eq!(with_builtins.status.code(), Some(0));
    let with_builtins_cl: Caseload =
        serde_json::from_slice(&with_builtins.stdout).expect("parse caseload");
    let with_cands = with_builtins_cl
        .discovery
        .expect("discovery")
        .candidate_canons;
    assert!(
        with_cands
            .iter()
            .any(|c| c == "builtin:cli/cli-flags-must-use-positive-polarity")
    );
    assert!(
        with_cands
            .iter()
            .any(|c| c == ".canons/cli/flags-must-use-positive-polarity.canon.md")
    );
    assert!(
        !with_cands.iter().any(|c| c == ".canons/README.md"),
        "non-canon .canons/README.md must not be discovered"
    );

    // 2. --no-builtins excludes all embedded builtin:* canons
    let without_builtins = Command::new(bin_path())
        .current_dir(ws.path())
        .args(["discover", "--all-canons", "--no-builtins", "--json"])
        .output()
        .expect("discover --all-canons --no-builtins");
    assert_eq!(without_builtins.status.code(), Some(0));
    let without_builtins_cl: Caseload =
        serde_json::from_slice(&without_builtins.stdout).expect("parse caseload");
    let without_cands = without_builtins_cl
        .discovery
        .expect("discovery")
        .candidate_canons;
    assert!(without_cands.iter().all(|c| !c.starts_with("builtin:")));
    assert_eq!(without_cands.len(), 2);

    // 3. --canon builtin:cli filters strictly to builtin:cli/* and round-trips through validate --caseload
    let caseload_file = ws.path().join("builtin-discovery.json");
    let filtered = Command::new(bin_path())
        .current_dir(ws.path())
        .args([
            "discover",
            "--all-canons",
            "--canon",
            "builtin:cli",
            "--output-caseload",
            caseload_file.to_str().expect("utf8 path"),
            "--json",
        ])
        .output()
        .expect("discover --canon builtin:cli");
    assert_eq!(filtered.status.code(), Some(0));
    let filtered_cl: Caseload = serde_json::from_slice(&filtered.stdout).expect("parse caseload");
    let cli_cands = filtered_cl.discovery.expect("discovery").candidate_canons;
    assert!(!cli_cands.is_empty());
    assert!(cli_cands.iter().all(|c| c.starts_with("builtin:cli/")));

    let validated = Command::new(bin_path())
        .current_dir(ws.path())
        .args([
            "validate",
            "--caseload",
            caseload_file.to_str().expect("utf8 path"),
            "--max-warnings",
            "0",
            "--json",
        ])
        .output()
        .expect("validate --caseload builtin-discovery.json");
    assert_eq!(validated.status.code(), Some(0));
    let validated_cl: Caseload = serde_json::from_slice(&validated.stdout).expect("parse caseload");
    let validation = validated_cl.validation.expect("validation");
    assert!(!validation.has_errors);
    assert_eq!(validation.results.len(), cli_cands.len());
}
