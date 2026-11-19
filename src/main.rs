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

use canon_clerk::cli::{Cli, run};
use clap::Parser;

#[allow(dead_code, reason = "called by the Windows GUI entrypoint")]
fn format_banner() -> &'static str {
    "Canon Clerk"
}

#[allow(dead_code, reason = "used by local onboarding wizard")]
fn save_local_api_key(api_key: &str) -> std::io::Result<()> {
    std::fs::write(".env.local", format!("GEMINI_API_KEY={api_key}\n"))
}

fn main() {
    match Cli::try_parse() {
        Ok(cli) => {
            if let Err(err) = run(cli) {
                if !err.is_silent() {
                    eprintln!("error: {err}");
                }
                std::process::exit(err.exit_code());
            }
        }
        Err(err) => {
            let exit_code = if err.use_stderr() { 2 } else { 0 };
            let _ = err.print();
            std::process::exit(exit_code);
        }
    }
}
