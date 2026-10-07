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

fn main() {
    match Cli::try_parse() {
        Ok(cli) => {
            if let Err(err) = run(cli) {
                eprintln!("Error: {err}");
                std::process::exit(1);
            }
        }
        Err(err) => {
            let exit_code = if err.use_stderr() { 2 } else { 0 };
            err.print().expect("Failed to write CLI output to terminal");
            std::process::exit(exit_code);
        }
    }
}
