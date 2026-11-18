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
    Caseload, DEFAULT_GOOGLE_GENERATOR_MODEL, DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL, HeuristicNode,
    ThinkingLevel,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn bin_path() -> &'static str {
    env!("CARGO_BIN_EXE_canon-clerk")
}

fn temp_xdg_dir(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let xdg_dir = std::env::temp_dir().join(format!("canon-clerk-336-{tag}-{unique}"));
    fs::create_dir_all(xdg_dir.join("canon-clerk")).expect("create xdg dir");
    xdg_dir
}

#[test]
fn test_cli_typesafe_provider_and_compact_format() {
    let xdg_dir = temp_xdg_dir("provider");

    let ts_out = Command::new(bin_path())
        .args(["configure", "--provider", "typesafe", "--json"])
        .env("TYPESAFE_API_KEY", "test-ts-key")
        .env_remove("GEMINI_API_KEY")
        .env("XDG_CONFIG_HOME", &xdg_dir)
        .output()
        .expect("execute configure --provider typesafe --json");
    assert_eq!(ts_out.status.code(), Some(0));
    let ts_caseload: Caseload = serde_json::from_slice(&ts_out.stdout).expect("parse caseload");
    let ts_cfg = ts_caseload.config.expect("config");
    assert_eq!(ts_cfg.provider, "typesafe");
    assert_eq!(ts_cfg.nodes.len(), 4);
    assert_eq!(
        ts_cfg.model_for(HeuristicNode::Vet),
        Some(DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL)
    );
    assert_eq!(ts_cfg.model_for(HeuristicNode::Audit), None);

    let compact_out = Command::new(bin_path())
        .args([
            "configure",
            "--provider",
            "typesafe",
            "--audit-model",
            DEFAULT_GOOGLE_GENERATOR_MODEL,
            "--format",
            "compact",
        ])
        .env("TYPESAFE_API_KEY", "test-ts-key")
        .env("GEMINI_API_KEY", "test-gemini-key")
        .env("XDG_CONFIG_HOME", &xdg_dir)
        .output()
        .expect("execute configure --format compact");
    assert_eq!(compact_out.status.code(), Some(0));
    let compact_text = String::from_utf8_lossy(&compact_out.stdout);
    assert!(compact_text.contains("provider\ttypesafe\n"));
    assert!(compact_text.contains("vet_model\ttypesafe:jev-latest\n"));
    assert!(compact_text.contains("audit_model\tgoogle:gemini-4-carbon\n"));
    assert!(compact_text.contains("credentials\tGEMINI_API_KEY,TYPESAFE_API_KEY\n"));

    let _ = fs::remove_dir_all(&xdg_dir);
}

#[test]
fn test_cli_rejects_bare_specifiers_retired_env_and_legacy_flat_config_keys() {
    let xdg_dir = temp_xdg_dir("retired");
    let cfg_file = xdg_dir.join("canon-clerk").join("config.json");

    let bare_spec = Command::new(bin_path())
        .args(["configure", "--docket-model", "gemini-decision-1"])
        .env("GEMINI_API_KEY", "test-gemini-key")
        .env("XDG_CONFIG_HOME", &xdg_dir)
        .output()
        .expect("execute configure with bare specifier");
    assert_eq!(bare_spec.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bare_spec.stderr).contains("google:gemini-decision-1"));

    let retired_env = Command::new(bin_path())
        .arg("configure")
        .env("GEMINI_API_KEY", "test-gemini-key")
        .env("CANON_CLERK_SCREENER_MODEL", "google:gemini-decision-1")
        .env("XDG_CONFIG_HOME", &xdg_dir)
        .output()
        .expect("execute configure with retired env var");
    assert_eq!(retired_env.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&retired_env.stderr).contains("Hint:"));

    fs::write(
        &cfg_file,
        r#"{"screenerModel": "google:gemini-decision-1"}"#,
    )
    .expect("write legacy config");
    fs::set_permissions(&cfg_file, fs::Permissions::from_mode(0o600)).expect("chmod 0600");
    let legacy_cfg_out = Command::new(bin_path())
        .arg("configure")
        .env("GEMINI_API_KEY", "test-gemini-key")
        .env("XDG_CONFIG_HOME", &xdg_dir)
        .output()
        .expect("execute configure with legacy config.json");
    assert_eq!(legacy_cfg_out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&legacy_cfg_out.stderr).contains("Hint:"));

    let _ = fs::remove_dir_all(&xdg_dir);
}

#[test]
fn test_cli_resolves_structured_providers_defaults_and_nodes_config_json() {
    let xdg_dir = temp_xdg_dir("structured");
    let cfg_file = xdg_dir.join("canon-clerk").join("config.json");

    fs::write(
        &cfg_file,
        r#"{
            "providers": {
                "google": { "apiKey": "cfg-google-key" },
                "typesafe": { "apiKey": "cfg-typesafe-key" }
            },
            "defaults": {
                "provider": "typesafe",
                "thinkingLevel": "high"
            },
            "nodes": {
                "audit": { "model": "google:gemini-4-carbon" }
            }
        }"#,
    )
    .expect("write structured config.json");
    fs::set_permissions(&cfg_file, fs::Permissions::from_mode(0o600)).expect("chmod 0600");

    let structured_out = Command::new(bin_path())
        .args(["configure", "--json"])
        .env_remove("GEMINI_API_KEY")
        .env_remove("TYPESAFE_API_KEY")
        .env("XDG_CONFIG_HOME", &xdg_dir)
        .output()
        .expect("execute configure against structured config.json");
    assert_eq!(structured_out.status.code(), Some(0));
    let resolved: Caseload = serde_json::from_slice(&structured_out.stdout).expect("parse json");
    let resolved_cfg = resolved.config.expect("config");
    assert_eq!(resolved_cfg.provider, "typesafe");
    assert_eq!(
        resolved_cfg.model_for(HeuristicNode::Docket),
        Some("typesafe:jev-latest")
    );
    assert_eq!(
        resolved_cfg
            .node(HeuristicNode::Docket)
            .and_then(|n| n.thinking_level),
        None
    );
    assert_eq!(
        resolved_cfg.model_for(HeuristicNode::Audit),
        Some("google:gemini-4-carbon")
    );
    assert_eq!(
        resolved_cfg
            .node(HeuristicNode::Audit)
            .and_then(|n| n.thinking_level),
        Some(ThinkingLevel::High)
    );

    let _ = fs::remove_dir_all(&xdg_dir);
}
