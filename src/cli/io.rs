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

//! Consolidated stdin validation and Caseload envelope file/stream I/O.

use crate::caseload::Caseload;
use crate::cli::CliError;
use crate::cli::args::FilingSourceArgs;
use std::fs;
use std::io::Read;
use std::path::Path;

/// Enforces the single-stdin guard: at most one operand may consume standard input (`-`).
pub fn ensure_single_stdin_source(
    filing: &FilingSourceArgs,
    caseload: Option<&str>,
) -> Result<(), CliError> {
    let mut stdin_consumers = Vec::new();

    let positional_stdin_count = filing.targets.iter().filter(|t| t.as_str() == "-").count();
    if positional_stdin_count > 1 {
        return Err(CliError::usage(
            "Standard input (`-`) was specified multiple times in positional `TARGETS`.\n  \
             Hint: Pass `-` at most once to read newline-delimited target paths from stdin."
                .to_string(),
        ));
    }
    if positional_stdin_count == 1 {
        stdin_consumers.push("positional `TARGETS` (`-`)");
    }
    if filing.diff.as_deref() == Some("-") {
        stdin_consumers.push("`--diff -`");
    }
    if filing.pr_body_file.as_deref() == Some("-") {
        stdin_consumers.push("`--pr-body-file -`");
    }
    if caseload == Some("-") {
        stdin_consumers.push("`--caseload -`");
    }

    if stdin_consumers.len() > 1 {
        return Err(CliError::usage(format!(
            "Conflicting standard input (`-`) sources specified: {}.\n  \
             Hint: Standard input can only be consumed by a single source per invocation; pass file paths for the remaining options.",
            stdin_consumers.join(", ")
        )));
    }

    Ok(())
}

/// Reads operand content from a file path or standard input when `source == "-"`.
pub fn read_operand_source(
    source: &str,
    flag_name: &str,
    stdin: &mut dyn Read,
) -> Result<String, CliError> {
    if source == "-" {
        let mut buf = String::new();
        stdin.read_to_string(&mut buf).map_err(|err| {
            CliError::usage(format!(
                "Failed to read standard input for `{flag_name} -`: {err}\n  \
                 Hint: Pipe valid UTF-8 text into standard input or provide a file path."
            ))
        })?;
        Ok(buf)
    } else {
        fs::read_to_string(source).map_err(|err| {
            CliError::usage(format!(
                "Failed to read file `{source}` for `{flag_name}`: {err}\n  \
                 Hint: Verify that `{source}` exists and is readable, or pass `-` to read from standard input."
            ))
        })
    }
}

/// Resolves positional target strings, expanding `-` into newline-delimited file paths read from `stdin`.
pub fn resolve_target_operands(
    targets: &[String],
    stdin: &mut dyn Read,
) -> Result<Option<Vec<String>>, CliError> {
    if targets.is_empty() {
        return Ok(None);
    }

    let mut resolved = Vec::new();
    for target in targets {
        if target == "-" {
            let raw = read_operand_source("-", "TARGETS", stdin)?;
            for line in raw.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    resolved.push(trimmed.to_string());
                }
            }
        } else {
            resolved.push(target.clone());
        }
    }

    Ok(Some(resolved))
}

/// Reads and parses an incoming Caseload JSON operand from a file path or stdin (`-`).
pub fn read_caseload_operand(
    caseload_source: Option<&str>,
    stdin: &mut dyn Read,
) -> Result<Option<Caseload>, CliError> {
    match caseload_source {
        Some(source) => {
            let raw = read_operand_source(source, "--caseload", stdin)?;
            let parsed: Caseload = serde_json::from_str(&raw).map_err(|err| {
                CliError::usage(format!(
                    "Failed to parse incoming Caseload JSON from `{source}`: {err}\n  \
                     Hint: Ensure `--caseload` points to a valid Canon Clerk Caseload JSON file."
                ))
            })?;
            Ok(Some(parsed))
        }
        None => Ok(None),
    }
}

/// Serializes and writes a `Caseload` to disk at `path`, creating intermediate parent directories as needed.
pub fn write_caseload_file(path: &Path, caseload: &Caseload) -> Result<(), CliError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|err| {
            CliError::usage(format!(
                "Failed to create parent directory `{}` for `--output-caseload`: {err}\n  \
                 Hint: Check directory write permissions for `{}`.",
                parent.display(),
                path.display()
            ))
        })?;
    }

    let json = serde_json::to_string_pretty(caseload)
        .map_err(|err| CliError::runtime(format!("Failed to serialize Caseload JSON: {err}")))?;
    fs::write(path, format!("{json}\n")).map_err(|err| {
        CliError::usage(format!(
            "Failed to write Caseload JSON to `{}`: {err}\n  \
             Hint: Verify that the output path is writable.",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_ensure_single_stdin_source_guards() {
        // Valid: single stdin consumer
        let filing_single = FilingSourceArgs {
            diff: Some("-".to_string()),
            ..FilingSourceArgs::default()
        };
        assert!(ensure_single_stdin_source(&filing_single, None).is_ok());

        // Invalid: multiple positional stdin
        let filing_dup = FilingSourceArgs {
            targets: vec!["-".to_string(), "-".to_string()],
            ..FilingSourceArgs::default()
        };
        let err = ensure_single_stdin_source(&filing_dup, None).expect_err("dup positional stdin");
        assert_eq!(err.exit_code(), 2);
        assert!(err.to_string().contains("specified multiple times"));

        // Invalid: conflicting stdin options
        let filing_conflict = FilingSourceArgs {
            diff: Some("-".to_string()),
            ..FilingSourceArgs::default()
        };
        let err =
            ensure_single_stdin_source(&filing_conflict, Some("-")).expect_err("conflicting stdin");
        assert_eq!(err.exit_code(), 2);
        assert!(err.to_string().contains("Conflicting standard input"));
    }

    #[test]
    fn test_read_operand_source_stdin() {
        let mut stdin = Cursor::new("piped content");
        let content = read_operand_source("-", "--diff", &mut stdin).expect("read stdin");
        assert_eq!(content, "piped content");
    }

    #[test]
    fn test_resolve_target_operands_stdin() {
        let targets = vec!["-".to_string()];
        let mut stdin = Cursor::new("foo.rs\nbar.rs\n\n");
        let resolved = resolve_target_operands(&targets, &mut stdin)
            .expect("resolve targets")
            .expect("some");
        assert_eq!(resolved, vec!["foo.rs", "bar.rs"]);
    }
}
