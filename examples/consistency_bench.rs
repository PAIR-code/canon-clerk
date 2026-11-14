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

//! Developer runner entrypoint for the multi-run consistency and flipper benchmark harness.

use canon_clerk::bench::{BenchIoStreams, ConsistencyBenchCli, run_consistency_bench_cli};
use clap::Parser;
use std::io;

fn main() {
    match ConsistencyBenchCli::try_parse() {
        Ok(cli) => {
            let mut stdin = io::stdin();
            let mut stdout = io::stdout();
            let mut stderr = io::stderr();
            let mut streams = BenchIoStreams {
                stdin: &mut stdin,
                stdout: &mut stdout,
                stderr: &mut stderr,
            };
            if let Err(err) = run_consistency_bench_cli(&cli, &mut streams) {
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
