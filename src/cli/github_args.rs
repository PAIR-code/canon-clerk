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

//! Composable Clap argument group for opt-in GitHub Checks REST API publication.

use crate::cli::CliError;
use crate::github::{DEFAULT_GITHUB_CHECK_NAME, GithubCheckTarget, resolve_github_check_target};
use clap::Args;
use std::collections::BTreeMap;

/// Composable CLI argument group for opt-in GitHub Checks REST API publication.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct GithubCheckArgs {
    /// Publish stage findings as a GitHub Check Run via the Checks REST API (batched 50 annotations/request).
    #[arg(
        long = "publish-github-check",
        help_heading = "GitHub Checks Publication",
        help = "Publish findings via the GitHub Checks REST API (requires GITHUB_TOKEN or GH_TOKEN with checks:write)"
    )]
    pub publish_github_check: bool,

    /// Target GitHub repository in `<owner>/<repo>` format (fallback env: `GITHUB_REPOSITORY`).
    #[arg(
        long = "github-repo",
        value_name = "OWNER/REPO",
        help_heading = "GitHub Checks Publication",
        help = "Target GitHub repository '<owner>/<repo>' (fallback env: GITHUB_REPOSITORY)"
    )]
    pub github_repo: Option<String>,

    /// Target commit SHA to annotate (fallback: `pull_request.head.sha` in `GITHUB_EVENT_PATH`, then `GITHUB_SHA`).
    #[arg(
        long = "github-head-sha",
        value_name = "SHA",
        help_heading = "GitHub Checks Publication",
        help = "Target commit SHA (fallback: pull_request.head.sha in GITHUB_EVENT_PATH, then GITHUB_SHA)"
    )]
    pub github_head_sha: Option<String>,

    /// Name of the published GitHub Check Run (default: `"Canon Clerk Audit"`).
    #[arg(
        long = "github-check-name",
        value_name = "NAME",
        help_heading = "GitHub Checks Publication",
        help = "GitHub Check Run name (default: 'Canon Clerk Audit')"
    )]
    pub github_check_name: Option<String>,
}

impl GithubCheckArgs {
    /// Validates and resolves the GitHub Checks publication target when `--publish-github-check` is active.
    pub fn resolve_target(
        &self,
        env: &BTreeMap<String, String>,
    ) -> Result<Option<GithubCheckTarget>, CliError> {
        if !self.publish_github_check {
            return Ok(None);
        }
        resolve_github_check_target(
            self.github_repo.as_deref(),
            self.github_head_sha.as_deref(),
            env,
        )
        .map(Some)
        .map_err(|err| CliError::usage(err.to_string()))
    }

    /// Returns the resolved Check Run name, defaulting to [`DEFAULT_GITHUB_CHECK_NAME`].
    pub fn resolved_check_name(&self) -> String {
        self.github_check_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_GITHUB_CHECK_NAME)
            .to_string()
    }
}
