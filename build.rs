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

use std::env;
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

struct BuiltinEntry {
    source_ref: String,
    domain: String,
    stem: String,
    rel_path: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let builtins_root = manifest_dir.join("builtins");
    println!("cargo:rerun-if-changed=builtins");

    let mut entries = collect_builtin_entries(&builtins_root)?;
    entries.sort_by(|a, b| a.source_ref.cmp(&b.source_ref));

    let mut generated = String::from("pub const BUILTIN_CANONS: &[BuiltinCanon] = &[\n");
    for entry in &entries {
        let _ = writeln!(generated, "    BuiltinCanon {{");
        let _ = writeln!(generated, "        source_ref: {:?},", entry.source_ref);
        let _ = writeln!(generated, "        domain: {:?},", entry.domain);
        let _ = writeln!(generated, "        stem: {:?},", entry.stem);
        let _ = writeln!(
            generated,
            "        raw_markdown: include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), {:?})),",
            format!("/{}", entry.rel_path)
        );
        let _ = writeln!(generated, "    }},");
    }
    generated.push_str("];\n");

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    fs::write(out_dir.join("builtins_generated.rs"), generated)?;
    Ok(())
}

fn collect_builtin_entries(builtins_root: &PathBuf) -> Result<Vec<BuiltinEntry>, Box<dyn Error>> {
    let mut entries = Vec::new();
    let mut domain_dirs = Vec::new();

    for item in fs::read_dir(builtins_root)? {
        let item = item?;
        let path = item.path();
        if !path.is_dir() {
            continue;
        }
        let domain = item.file_name().to_string_lossy().into_owned();
        if domain.starts_with('.') {
            continue;
        }
        domain_dirs.push((domain, path));
    }
    domain_dirs.sort_by(|a, b| a.0.cmp(&b.0));

    for (domain, dir_path) in domain_dirs {
        println!("cargo:rerun-if-changed=builtins/{domain}");
        let mut files = Vec::new();
        for item in fs::read_dir(&dir_path)? {
            let item = item?;
            let path = item.path();
            if !path.is_file() {
                continue;
            }
            let file_name = item.file_name().to_string_lossy().into_owned();
            if let Some(stem) = file_name.strip_suffix(".canon.md") {
                files.push((file_name.clone(), stem.to_string()));
            }
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));

        for (file_name, stem) in files {
            let rel_path = format!("builtins/{domain}/{file_name}");
            println!("cargo:rerun-if-changed={rel_path}");
            entries.push(BuiltinEntry {
                source_ref: format!("builtin:{domain}/{stem}"),
                domain: domain.clone(),
                stem,
                rel_path,
            });
        }
    }

    Ok(entries)
}
