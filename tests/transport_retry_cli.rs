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

//! Integration tests verifying CLI retry/backoff help ergonomics, subcommand flag scoping,
//! and single-shot `probe` isolation (`docs/architecture/transport.md` §5–§6).

use canon_clerk::{MultiProviderEndpointProber, ProbeArgs, run_probe_with_env_and_prober};
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

fn bin_path() -> &'static str {
    env!("CARGO_BIN_EXE_canon-clerk")
}

mod cli_retry_flag_ergonomics_and_scoping {
    use super::*;

    #[test]
    fn test_heuristic_subcommand_help_documents_provider_transport_and_retry_flags() {
        for subcommand in ["apprise", "docket", "admit", "vet", "audit"] {
            let out = Command::new(bin_path())
                .args([subcommand, "--help"])
                .output()
                .expect("run --help");
            assert_eq!(out.status.code(), Some(0));
            let help_text = String::from_utf8_lossy(&out.stdout);
            assert!(
                help_text.contains("Provider Transport & Retry:"),
                "`{subcommand} --help` must group retry flags under Provider Transport & Retry"
            );
            assert!(
                help_text.contains("--max-retries <COUNT>")
                    && help_text.contains("CANON_CLERK_MAX_RETRIES")
                    && help_text.contains("0 disables retries"),
                "`{subcommand} --help` must clearly document --max-retries semantics"
            );
            assert!(
                help_text.contains("--max-backoff-ms <MS>")
                    && help_text.contains("CANON_CLERK_MAX_BACKOFF_MS")
                    && help_text.contains("90000"),
                "`{subcommand} --help` must clearly document --max-backoff-ms semantics"
            );
        }
    }

    #[test]
    fn test_non_heuristic_and_probe_subcommands_reject_retry_flags_with_exit_2() {
        for (sub, flag) in [
            ("probe", "--max-retries"),
            ("probe", "--max-backoff-ms"),
            ("configure", "--max-retries"),
            ("validate", "--max-backoff-ms"),
        ] {
            let out = Command::new(bin_path())
                .args([sub, flag, "1"])
                .output()
                .expect("invoke subcommand");
            assert_eq!(
                out.status.code(),
                Some(2),
                "`canon-clerk {sub} {flag} 1` must be rejected with exit code 2"
            );
        }
    }
}

mod single_shot_probe_isolation {
    use super::*;

    #[test]
    fn test_probe_executes_single_shot_without_sleeping_on_429_retry_after() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_ref = Arc::clone(&hits);

        let server = thread::spawn(move || {
            // `probe` checks screener + auditor concurrently (2 single-shot calls total).
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept probe connection");
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut buf = [0_u8; 2048];
                let _ = stream.read(&mut buf);
                hits_ref.fetch_add(1, Ordering::SeqCst);
                let body = r#"{"error":{"message":"Please retry in 5s"}}"#;
                let resp = format!(
                    "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nRetry-After: 5\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
        });

        let prober = MultiProviderEndpointProber::with_base_urls(
            format!("http://{addr}/v1beta"),
            format!("http://{addr}/v1"),
        );
        let mut env = BTreeMap::new();
        env.insert("GEMINI_API_KEY".to_string(), "synthetic-key".to_string());
        let args = ProbeArgs::default();
        let mut stdin = Cursor::new(Vec::<u8>::new());
        let mut stdout = Vec::<u8>::new();

        let start = Instant::now();
        let err = run_probe_with_env_and_prober(&args, &mut stdin, &mut stdout, &env, &prober)
            .expect_err("429 probe must return unhealthy exit 2");
        assert_eq!(err.exit_code(), 2);
        assert!(
            start.elapsed() < Duration::from_millis(800),
            "probe must never sleep for 5s Retry-After cooldown"
        );
        assert_eq!(
            hits.load(Ordering::SeqCst),
            2,
            "probe must issue exactly 1 single-shot request per configured tier (0 retries)"
        );
        server.join().expect("server joined");
    }
}
