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

use canon_clerk::Caseload;
use std::process::Command;

fn bin_path() -> &'static str {
    env!("CARGO_BIN_EXE_canon-clerk")
}

#[test]
fn test_cli_vet_guards_and_zero_candidate_short_circuit() {
    // 1. Case-or-Controversy guard exits 2
    let vet_all_canons_no_filing = Command::new(bin_path())
        .args(["vet", "--all-canons"])
        .output()
        .expect("execute canon-clerk vet --all-canons");
    assert_eq!(vet_all_canons_no_filing.status.code(), Some(2));

    // 2. Invalid --vet-concurrency 0 and out-of-closure --audit-threshold exit 2
    let vet_zero_conc = Command::new(bin_path())
        .args(["vet", "src/lib.rs", "--vet-concurrency", "0"])
        .output()
        .expect("execute canon-clerk vet --vet-concurrency 0");
    assert_eq!(vet_zero_conc.status.code(), Some(2));

    let vet_out_of_closure = Command::new(bin_path())
        .args(["vet", "src/lib.rs", "--audit-threshold", "0.5"])
        .output()
        .expect("execute canon-clerk vet --audit-threshold 0.5");
    assert_eq!(vet_out_of_closure.status.code(), Some(2));

    // 3. Discriminative model on --auditor-model exits 2 on configure
    let bad_auditor = Command::new(bin_path())
        .args(["configure", "--auditor-model", "jev-latest"])
        .env("TYPESAFE_API_KEY", "test-typesafe-key")
        .output()
        .expect("execute canon-clerk configure --auditor-model jev-latest");
    assert_eq!(bad_auditor.status.code(), Some(2));

    // 4. Zero-candidate short-circuit exits 0 without API keys and attaches empty .vetting
    let vet_zero_cand = Command::new(bin_path())
        .args([
            "vet",
            "--canon",
            ".canons/readme-authoring/readme-must-lead-with-what-and-why.md",
            "src/lib.rs",
            "--json",
        ])
        .env_remove("GEMINI_API_KEY")
        .env_remove("TYPESAFE_API_KEY")
        .env("XDG_CONFIG_HOME", "/nonexistent-canon-clerk-xdg-dir")
        .output()
        .expect("execute canon-clerk vet zero-candidate run");
    assert_eq!(vet_zero_cand.status.code(), Some(0));
    let parsed: Caseload =
        serde_json::from_slice(&vet_zero_cand.stdout).expect("parse vet Caseload JSON");
    assert!(parsed.vetting.expect("vetting").is_all_vetted());
}
