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

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Self-contained temporary workspace populated with hermetic `.canons/` and `src/` fixtures.
pub(crate) struct TempCanonWorkspace {
    path: PathBuf,
}

impl TempCanonWorkspace {
    pub(crate) fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "canon-clerk-cli-it-{label}-{unique}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join(".canons/cli")).expect("create .canons/cli");
        fs::create_dir_all(path.join("src")).expect("create src");
        fs::write(
            path.join(".canons/cli/flags-must-use-positive-polarity.md"),
            "---\ntriggers:\n  - \"src/**/*.rs\"\ntags:\n  - cli\n---\nCLI flags MUST use positive polarity.\n",
        )
        .expect("write fixture canon");
        fs::write(path.join("src/lib.rs"), "pub fn fixture() {}\n").expect("write fixture target");
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempCanonWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
