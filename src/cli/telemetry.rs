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

//! CLI driving adapters for `--log-file` JSONL persistence and `stderr` diagnostic progress.

use crate::cli::CliError;
use crate::telemetry::{CompositeTelemetrySink, JsonlWriterSink, TelemetryEvent, TelemetrySink};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

/// Environment variable fallback for `--log-file <PATH>`.
pub const ENV_LOG_FILE: &str = "CANON_CLERK_LOG_FILE";

/// Environment variable fallback for `--verbose` (`-v`).
pub const ENV_VERBOSE: &str = "CANON_CLERK_VERBOSE";

/// Resolves the target `--log-file` JSONL path from explicit CLI flag or `CANON_CLERK_LOG_FILE`.
pub fn resolve_log_file_path(
    flag_path: Option<&Path>,
    env: &BTreeMap<String, String>,
) -> Result<Option<PathBuf>, CliError> {
    if let Some(explicit) = flag_path {
        validate_log_file_operand(explicit, "--log-file")?;
        return Ok(Some(explicit.to_path_buf()));
    }

    if let Some(raw_env) = env
        .get(ENV_LOG_FILE)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let candidate = Path::new(raw_env);
        validate_log_file_operand(candidate, ENV_LOG_FILE)?;
        return Ok(Some(candidate.to_path_buf()));
    }

    Ok(None)
}

fn validate_log_file_operand(path: &Path, source_label: &str) -> Result<(), CliError> {
    let text = path.to_string_lossy();
    if text.trim().is_empty() {
        return Err(CliError::usage(format!(
            "Empty path supplied for `{source_label}`.\n  \
             Hint: Pass a writable file path via `--log-file <PATH>` or set `{ENV_LOG_FILE}=<PATH>`."
        )));
    }
    if text.trim() == "-" {
        return Err(CliError::usage(format!(
            "Standard output/input token (`-`) is not valid for `{source_label}` because `stdout` is reserved for Caseload payloads.\n  \
             Hint: Pass a file path such as `--log-file telemetry.jsonl`, or pass `--verbose` (`-v`) to stream diagnostic progress to `stderr`."
        )));
    }
    Ok(())
}

/// Resolves effective verbose diagnostic mode from `--verbose` (`-v`) or `CANON_CLERK_VERBOSE`.
pub fn resolve_verbose_flag(
    flag_verbose: bool,
    env: &BTreeMap<String, String>,
) -> Result<bool, CliError> {
    if flag_verbose {
        return Ok(true);
    }

    if let Some(raw_env) = env
        .get(ENV_VERBOSE)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return match raw_env.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(CliError::usage(format!(
                "Invalid boolean value `{raw_env}` in `{ENV_VERBOSE}`.\n  \
                 Hint: Set `{ENV_VERBOSE}=true` (or `1`, `false`, `0`) or pass `--verbose` (`-v`)."
            ))),
        };
    }

    Ok(false)
}

/// Opens a buffered file writer wrapped in [`JsonlWriterSink`], creating parent directories if needed.
pub fn open_jsonl_log_sink(path: &Path) -> Result<JsonlWriterSink<BufWriter<File>>, CliError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|err| {
            CliError::usage(format!(
                "Failed to create parent directory `{}` for `--log-file`: {err}\n  \
                 Hint: Verify directory write permissions or pass a writable `--log-file <PATH>`.",
                parent.display()
            ))
        })?;
    }

    let file = File::create(path).map_err(|err| {
        CliError::usage(format!(
            "Failed to open telemetry log file `{}` for writing: {err}\n  \
             Hint: Pass a writable file path via `--log-file <PATH>` or update `{ENV_LOG_FILE}`.",
            path.display()
        ))
    })?;

    Ok(JsonlWriterSink::new(BufWriter::new(file)))
}

/// Terminal and environment context governing `stderr` diagnostic progress rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StderrTelemetryOptions {
    /// Whether `stderr` is attached to an interactive terminal (`IsTerminal`).
    pub is_tty: bool,
    /// Whether `--verbose` (`-v` or `CANON_CLERK_VERBOSE`) is active.
    pub verbose: bool,
    /// Whether subcommand `-q` / `--quiet` mode suppresses non-essential diagnostics.
    pub quiet: bool,
    /// Whether `CI` environment indicates an automated continuous integration runner.
    pub is_ci: bool,
    /// Whether ANSI color escapes are suppressed via `NO_COLOR` or `TERM=dumb`.
    pub no_color: bool,
}

impl StderrTelemetryOptions {
    /// Derives [`StderrTelemetryOptions`] from terminal TTY state, flags, and environment variables.
    pub fn from_env(
        is_tty: bool,
        verbose: bool,
        quiet: bool,
        env: &BTreeMap<String, String>,
    ) -> Self {
        let is_ci = env
            .get("CI")
            .map(|v| v.trim().to_ascii_lowercase())
            .is_some_and(|v| !v.is_empty() && v != "0" && v != "false");

        let has_no_color = env.get("NO_COLOR").is_some_and(|v| !v.is_empty());
        let is_dumb_term = env
            .get("TERM")
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("dumb"));

        Self {
            is_tty,
            verbose,
            quiet,
            is_ci,
            no_color: has_no_color || is_dumb_term,
        }
    }

    /// Returns `true` when human-readable telemetry progress should be emitted to `stderr`.
    ///
    /// Suppresses output when `-q` / `--quiet` is active, and stays silent by default in
    /// non-TTY pipes or `CI` runners unless `--verbose` (`-v`) is explicitly enabled.
    pub const fn should_emit_stderr(self) -> bool {
        !self.quiet && (self.verbose || (self.is_tty && !self.is_ci))
    }

    /// Returns `true` when ANSI styling escapes are safe to emit on `stderr`.
    pub const fn use_ansi_color(self) -> bool {
        self.is_tty && !self.is_ci && !self.no_color
    }
}

/// Human-readable `stderr` diagnostic [`TelemetrySink`] respecting non-TTY, `CI`, `NO_COLOR`, and `--quiet`.
pub struct StderrProgressSink<W: Write + Send> {
    writer: Mutex<W>,
    options: StderrTelemetryOptions,
}

impl<W: Write + Send> StderrProgressSink<W> {
    /// Constructs a `stderr` progress sink writing to `writer` under `options`.
    pub fn new(writer: W, options: StderrTelemetryOptions) -> Self {
        Self {
            writer: Mutex::new(writer),
            options,
        }
    }

    /// Consumes the sink and returns the underlying writer.
    pub fn into_inner(self) -> W {
        self.writer
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn prefix(&self) -> &'static str {
        if self.options.use_ansi_color() {
            "\x1b[2m[telemetry]\x1b[0m"
        } else {
            "[telemetry]"
        }
    }

    fn format_line(&self, event: &TelemetryEvent) -> Option<String> {
        if !self.options.should_emit_stderr() {
            return None;
        }
        let pfx = self.prefix();
        match event {
            TelemetryEvent::StageStart { stage, .. } => {
                if self.options.verbose {
                    Some(format!("{pfx} -> {stage} started"))
                } else {
                    None
                }
            }
            TelemetryEvent::Thought { stage, delta } => {
                if self.options.verbose {
                    let compact = delta.split_whitespace().collect::<Vec<_>>().join(" ");
                    if compact.is_empty() {
                        None
                    } else {
                        Some(format!("{pfx} [{stage}:thought] {compact}"))
                    }
                } else {
                    None
                }
            }
            TelemetryEvent::StageFinish {
                stage,
                duration_ms,
                usage,
            } => match usage {
                Some(u) => Some(format!(
                    "{pfx} ok {stage} ({duration_ms}ms, {} tokens)",
                    u.total_tokens
                )),
                None => Some(format!("{pfx} ok {stage} ({duration_ms}ms)")),
            },
            TelemetryEvent::PipelineFinish {
                total_duration_ms,
                total_tokens,
            } => {
                if *total_tokens > 0 {
                    Some(format!(
                        "{pfx} done ({total_duration_ms}ms, {total_tokens} total tokens)"
                    ))
                } else {
                    Some(format!("{pfx} done ({total_duration_ms}ms)"))
                }
            }
        }
    }
}

impl<W: Write + Send> TelemetrySink for StderrProgressSink<W> {
    fn is_enabled(&self) -> bool {
        self.options.should_emit_stderr()
    }

    fn emit(&self, event: &TelemetryEvent) {
        if let Some(line) = self.format_line(event) {
            let mut guard = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
            let _ = writeln!(guard, "{line}");
            let _ = guard.flush();
        }
    }
}

/// Assembles a [`CompositeTelemetrySink`] combining optional `--log-file` JSONL persistence
/// and `stderr` progress output.
pub fn build_cli_telemetry_sink(
    log_file: Option<&Path>,
    stderr_options: StderrTelemetryOptions,
) -> Result<CompositeTelemetrySink, CliError> {
    let mut composite = CompositeTelemetrySink::new();
    if let Some(path) = log_file {
        let file_sink = open_jsonl_log_sink(path)?;
        composite.push(Box::new(file_sink));
    }
    if stderr_options.should_emit_stderr() {
        let stderr_sink = StderrProgressSink::new(std::io::stderr(), stderr_options);
        composite.push(Box::new(stderr_sink));
    }
    Ok(composite)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::TokenUsage;
    use crate::telemetry::PipelineStage;

    mod flag_and_env_resolution {
        use super::*;

        #[test]
        fn test_log_file_flag_takes_precedence_over_env_var() {
            let mut env = BTreeMap::new();
            env.insert(ENV_LOG_FILE.to_string(), "env-events.jsonl".to_string());

            let resolved_env = resolve_log_file_path(None, &env)
                .expect("resolve from env")
                .expect("some path");
            assert_eq!(resolved_env, PathBuf::from("env-events.jsonl"));

            let flag_path = Path::new("flag-events.jsonl");
            let resolved_flag = resolve_log_file_path(Some(flag_path), &env)
                .expect("resolve from flag")
                .expect("some path");
            assert_eq!(resolved_flag, PathBuf::from("flag-events.jsonl"));
        }

        #[test]
        fn test_log_file_rejects_blank_or_hyphen_with_actionable_hint() {
            let empty_env = BTreeMap::new();
            let err_hyphen = resolve_log_file_path(Some(Path::new("-")), &empty_env)
                .expect_err("hyphen must fail");
            assert_eq!(err_hyphen.exit_code(), 2);
            assert!(err_hyphen.to_string().contains("Hint:"));

            let err_blank = resolve_log_file_path(Some(Path::new("   ")), &empty_env)
                .expect_err("blank must fail");
            assert_eq!(err_blank.exit_code(), 2);
            assert!(err_blank.to_string().contains("Hint:"));
        }

        #[test]
        fn test_verbose_env_var_fallback_and_validation() {
            let mut env = BTreeMap::new();
            assert!(!resolve_verbose_flag(false, &env).expect("default false"));

            env.insert(ENV_VERBOSE.to_string(), "true".to_string());
            assert!(resolve_verbose_flag(false, &env).expect("env true"));

            env.insert(ENV_VERBOSE.to_string(), "0".to_string());
            assert!(!resolve_verbose_flag(false, &env).expect("env 0"));
            assert!(resolve_verbose_flag(true, &env).expect("flag overrides env 0"));

            env.insert(ENV_VERBOSE.to_string(), "invalid".to_string());
            let err = resolve_verbose_flag(false, &env).expect_err("invalid bool");
            assert_eq!(err.exit_code(), 2);
            assert!(err.to_string().contains("Hint:"));
        }
    }

    mod non_interactive_and_stderr_hygiene {
        use super::*;

        #[test]
        fn test_non_tty_and_ci_suppress_default_progress_and_ansi_escapes() {
            let mut ci_env = BTreeMap::new();
            ci_env.insert("CI".to_string(), "true".to_string());

            let ci_default = StderrTelemetryOptions::from_env(true, false, false, &ci_env);
            assert!(!ci_default.should_emit_stderr());
            assert!(!ci_default.use_ansi_color());

            let ci_verbose = StderrTelemetryOptions::from_env(true, true, false, &ci_env);
            assert!(ci_verbose.should_emit_stderr());
            assert!(!ci_verbose.use_ansi_color());

            let mut no_color_env = BTreeMap::new();
            no_color_env.insert("NO_COLOR".to_string(), "1".to_string());
            let tty_no_color = StderrTelemetryOptions::from_env(true, false, false, &no_color_env);
            assert!(tty_no_color.should_emit_stderr());
            assert!(!tty_no_color.use_ansi_color());

            let quiet_opts = StderrTelemetryOptions::from_env(true, true, true, &BTreeMap::new());
            assert!(!quiet_opts.should_emit_stderr());
        }

        #[test]
        fn test_stderr_sink_renders_plain_progress_without_ansi_when_non_tty() {
            let opts = StderrTelemetryOptions {
                is_tty: false,
                verbose: true,
                quiet: false,
                is_ci: false,
                no_color: false,
            };
            let sink = StderrProgressSink::new(Vec::<u8>::new(), opts);
            assert!(sink.is_enabled());

            sink.emit(&TelemetryEvent::StageStart {
                stage: PipelineStage::Apprise,
                timestamp: "2026-11-09T20:00:00.000Z".to_string(),
            });
            sink.emit(&TelemetryEvent::Thought {
                stage: PipelineStage::Apprise,
                delta: "First line\nsecond line".to_string(),
            });
            sink.emit(&TelemetryEvent::StageFinish {
                stage: PipelineStage::Apprise,
                duration_ms: 210,
                usage: Some(TokenUsage {
                    prompt_tokens: 100,
                    completion_tokens: 40,
                    reasoning_tokens: 10,
                    total_tokens: 150,
                }),
            });
            sink.emit(&TelemetryEvent::PipelineFinish {
                total_duration_ms: 220,
                total_tokens: 150,
            });

            let rendered = String::from_utf8(sink.into_inner()).expect("utf8 stderr");
            assert!(!rendered.contains("\x1b["), "must not contain ANSI escapes");
            assert!(rendered.contains("[telemetry] -> apprise started"));
            assert!(rendered.contains("[telemetry] [apprise:thought] First line second line"));
            assert!(rendered.contains("[telemetry] ok apprise (210ms, 150 tokens)"));
            assert!(rendered.contains("[telemetry] done (220ms, 150 total tokens)"));
        }
    }
}
