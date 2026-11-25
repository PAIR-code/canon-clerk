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

//! Top-level CLI routing, `--help` visibility, and cross-subcommand dispatch tests for `--github-issue`.

use canon_clerk::cli::{Cli, run};
use clap::{CommandFactory, Parser};

#[test]
fn test_github_issue_conflicts_with_github_pr_and_stage_scoping() {
    // 1. Mutual exclusivity with --github-pr and zero issue number rejection on intake and apprise
    for stage in ["intake", "apprise"] {
        let err = Cli::try_parse_from([
            "canon-clerk",
            stage,
            "--github-issue",
            "397",
            "--github-pr",
            "42",
        ])
        .expect_err("--github-issue and --github-pr must conflict");
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);

        let zero_err = Cli::try_parse_from(["canon-clerk", stage, "--github-issue", "0"])
            .expect_err("--github-issue 0 must be rejected at CLI parse time");
        assert_eq!(zero_err.kind(), clap::error::ErrorKind::ValueValidation);
    }

    // 2. Help visibility: shown on intake & apprise, hidden on all other stages
    let mut root = Cli::command();
    root.build();
    for shown in ["intake", "apprise"] {
        let help = root
            .find_subcommand_mut(shown)
            .unwrap()
            .render_help()
            .to_string();
        assert!(
            help.contains("--github-issue"),
            "`{shown}` --help must document --github-issue"
        );
    }
    for hidden in ["discover", "validate", "docket", "admit", "vet", "audit"] {
        let help = root
            .find_subcommand_mut(hidden)
            .unwrap()
            .render_help()
            .to_string();
        assert!(
            !help.contains("--github-issue"),
            "`{hidden}` --help must hide --github-issue"
        );
    }

    // 3. Contentious stages reject --github-issue with exit 2 and --linked-issue Hint
    for stage in ["docket", "admit", "vet", "audit"] {
        let cli = Cli::try_parse_from(["canon-clerk", stage, "--github-issue", "397"])
            .unwrap_or_else(|e| panic!("hidden capture on {stage} must parse: {e}"));
        let err = run(cli).expect_err(&format!("{stage} --github-issue must fail"));
        assert_eq!(err.exit_code(), 2);
        assert!(
            err.to_string().contains("--linked-issue"),
            "{stage} error must hint --linked-issue: {err}"
        );
    }

    // 4. Intermediate stages (discover, validate) reject --github-issue with exit 2 and actionable Hint
    for stage in ["discover", "validate"] {
        let cli = Cli::try_parse_from(["canon-clerk", stage, "--github-issue"])
            .unwrap_or_else(|e| panic!("hidden capture on {stage} must parse: {e}"));
        let err = run(cli).expect_err(&format!("{stage} --github-issue must fail"));
        assert_eq!(err.exit_code(), 2);
        assert!(
            err.to_string().contains("Hint:"),
            "{stage} error must include Hint: {err}"
        );
    }
}
