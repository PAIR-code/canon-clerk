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

use canon_clerk::{
    BenchIoStreams, Caseload, ConsistencyBenchCli, ConsistencyBenchReport, HermeticBenchDeps,
    SteppingTelemetryClock, WorkspaceSnapshot, run_consistency_bench_cli,
    run_consistency_bench_hermetic,
};
use clap::Parser;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Cursor;
use std::time::{SystemTime, UNIX_EPOCH};

mod cli_surface_and_exit_codes {
    use super::*;

    #[test]
    fn test_clap_help_version_and_unknown_flag_exit_semantics() {
        let help_err = ConsistencyBenchCli::try_parse_from(["consistency_bench", "--help"])
            .expect_err("--help exits early via clap DisplayHelp");
        assert!(
            !help_err.use_stderr(),
            "--help must route to stdout (exit 0)"
        );
        let rendered = help_err.to_string();
        assert!(rendered.contains("Benchmark Options:"));
        assert!(rendered.contains("Emit structured benchmark report JSON to stdout"));

        let ver_err = ConsistencyBenchCli::try_parse_from(["consistency_bench", "--version"])
            .expect_err("--version exits early via clap DisplayVersion");
        assert!(
            !ver_err.use_stderr(),
            "--version must route to stdout (exit 0)"
        );
        assert!(ver_err.to_string().contains("consistency_bench"));

        let unknown_err =
            ConsistencyBenchCli::try_parse_from(["consistency_bench", "--unknown-flag"])
                .expect_err("unknown flag must fail parsing");
        assert!(
            unknown_err.use_stderr(),
            "unknown flag must route to stderr (exit 2)"
        );
    }

    #[test]
    fn test_missing_stage_and_missing_credential_errors_exit_with_code_2_and_hints() {
        let no_stage =
            ConsistencyBenchCli::try_parse_from(["consistency_bench", "src/lib.rs"]).expect("cli");
        let mut stdin = Cursor::new(b"");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let err = run_consistency_bench_cli(
            &no_stage,
            &mut BenchIoStreams {
                stdin: &mut stdin,
                stdout: &mut stdout,
                stderr: &mut stderr,
            },
        )
        .expect_err("missing stage must fail");
        assert_eq!(err.exit_code(), 2);
        assert!(err.to_string().contains("Hint:"));

        // Isolated environment without GEMINI_API_KEY fails fast with exit code 2 + remediation
        let ws = WorkspaceSnapshot::from_disk(
            &env::current_dir().expect("cwd"),
            &[".canons/cli/cli-flags-must-use-positive-polarity.md".to_string()],
        )
        .expect("load disk workspace");
        let empty_env = BTreeMap::new();
        let clock = SteppingTelemetryClock::new(1_767_225_600_000, 50);
        let dummy_provider = canon_clerk::GeminiModelProvider::new("unused");
        let cli_missing_key = ConsistencyBenchCli::try_parse_from([
            "consistency_bench",
            "--stage",
            "apprise",
            "--canon",
            ".canons/cli/cli-flags-must-use-positive-polarity.md",
            "--intent",
            "Add CLI flag",
            "src/cli/mod.rs",
        ])
        .expect("parse apprise cli");

        let key_err = run_consistency_bench_hermetic(
            &cli_missing_key,
            &HermeticBenchDeps {
                workspace: &ws,
                env: &empty_env,
                provider: &dummy_provider,
                clock: &clock,
            },
            &mut BenchIoStreams {
                stdin: &mut stdin,
                stdout: &mut stdout,
                stderr: &mut stderr,
            },
        )
        .expect_err("missing GEMINI_API_KEY must fail");
        assert_eq!(key_err.exit_code(), 2);
        assert!(key_err.to_string().contains("GEMINI_API_KEY"));
        assert!(key_err.to_string().contains("Hint:"));
    }
}

mod live_disk_zero_candidate_and_artifact_io {
    use super::*;

    #[test]
    fn test_live_disk_zero_candidate_run_writes_json_and_caseload_artifacts_offline() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = env::temp_dir().join(format!("canon-clerk-bench-it-{unique}"));
        let report_path = dir.join("nested").join("bench-report.json");
        let caseload_path = dir.join("prepared-caseload.json");

        let cli = ConsistencyBenchCli::try_parse_from([
            "consistency_bench",
            "--stage",
            "apprise",
            "--trials",
            "3",
            "--canon",
            ".canons/nonexistent-rule.md",
            "--intent",
            "Verify zero candidate short-circuit",
            "--output-json",
            report_path.to_str().expect("utf8 path"),
            "--output-caseload",
            caseload_path.to_str().expect("utf8 path"),
            "--json",
            "src/lib.rs",
        ])
        .expect("parse zero-candidate cli");

        let mut stdin = Cursor::new(b"");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let report = run_consistency_bench_cli(
            &cli,
            &mut BenchIoStreams {
                stdin: &mut stdin,
                stdout: &mut stdout,
                stderr: &mut stderr,
            },
        )
        .expect("live disk zero-candidate run");

        assert_eq!(report.trials, 3);
        assert_eq!(report.candidate_canon_count, 0);

        let stdout_parsed: ConsistencyBenchReport =
            serde_json::from_slice(&stdout).expect("parse stdout JSON");
        assert_eq!(stdout_parsed, report);

        let file_parsed: ConsistencyBenchReport =
            serde_json::from_str(&fs::read_to_string(&report_path).expect("read report file"))
                .expect("parse report file");
        assert_eq!(file_parsed, report);

        let saved_caseload: Caseload =
            serde_json::from_str(&fs::read_to_string(&caseload_path).expect("read caseload file"))
                .expect("parse caseload file");
        assert!(saved_caseload.intake.is_some());
        assert!(saved_caseload.discovery.is_some());

        let _ = fs::remove_dir_all(&dir);
    }
}
