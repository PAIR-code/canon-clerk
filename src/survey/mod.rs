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

//! Branch C (`survey`): Deterministic repository-identity contextualization stage
//! (`docs/architecture/nodes/survey.md`).

use crate::caseload::{
    Caseload, CaseloadSurvey, DEFAULT_SURVEY_MAX_FILE_CHARS, DEFAULT_SURVEY_MAX_TOTAL_CHARS,
    SurveyFile, SurveyOmissionReason,
};
use crate::discover::glob_matcher::{GlobPattern, is_ignored_path};
use crate::policy::{PolicyError, WorkspacePolicySet, discover_policy_files_on_disk};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::num::NonZeroUsize;
use std::path::Path;

/// Environment variable overriding the default per-file character ceiling for `survey`.
pub const ENV_SURVEY_MAX_FILE_CHARS: &str = "CANON_CLERK_SURVEY_MAX_FILE_CHARS";

/// Environment variable overriding the default aggregate character ceiling for `survey`.
pub const ENV_SURVEY_MAX_TOTAL_CHARS: &str = "CANON_CLERK_SURVEY_MAX_TOTAL_CHARS";

/// Default per-file character ceiling as a non-zero domain integer (`4000`).
pub const DEFAULT_SURVEY_MAX_FILE_CHARS_NONZERO: NonZeroUsize =
    NonZeroUsize::new(DEFAULT_SURVEY_MAX_FILE_CHARS).expect("default max_file_chars is non-zero");

/// Default aggregate character ceiling as a non-zero domain integer (`12000`).
pub const DEFAULT_SURVEY_MAX_TOTAL_CHARS_NONZERO: NonZeroUsize =
    NonZeroUsize::new(DEFAULT_SURVEY_MAX_TOTAL_CHARS).expect("default max_total_chars is non-zero");

/// Configuration options controlling `survey` execution (`docs/architecture/nodes/survey.md` §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurveyOptions {
    /// Explicit workspace-relative globs overriding `<scope>/.canons/policy-chains.yaml` and built-in defaults.
    pub context_globs: Vec<String>,
    /// Explicitly disable repository-identity context collection (produces an empty `.survey` record).
    pub no_survey_context: bool,
    /// Maximum materialized Unicode scalar characters per surveyed file (`>= 1`).
    pub max_file_chars: NonZeroUsize,
    /// Maximum aggregate materialized Unicode scalar characters across all surveyed files (`>= 1`).
    pub max_total_chars: NonZeroUsize,
}

impl Default for SurveyOptions {
    fn default() -> Self {
        Self {
            context_globs: Vec::new(),
            no_survey_context: false,
            max_file_chars: DEFAULT_SURVEY_MAX_FILE_CHARS_NONZERO,
            max_total_chars: DEFAULT_SURVEY_MAX_TOTAL_CHARS_NONZERO,
        }
    }
}

/// Domain error returned by [`execute_survey_on_disk`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurveyError {
    /// Both `--survey-context` and `--no-survey-context` were supplied together.
    ConflictingContextFlags,
    /// Per-file or aggregate character ceiling was zero or unparseable.
    InvalidCeiling {
        /// Flag or environment variable name.
        name: &'static str,
        /// Invalid supplied value.
        value: String,
    },
    /// Supplied `--survey-context` glob pattern was empty or syntactically invalid.
    InvalidContextGlob {
        /// Raw pattern string.
        pattern: String,
        /// Underlying parser diagnostic.
        message: String,
    },
    /// Supplied `--survey-context` pattern attempted directory traversal (`../`) or used a leading `/`.
    PathEscapesWorkspace {
        /// Offending pattern string.
        path: String,
    },
    /// Workspace `<scope>/.canons/policy-chains.yaml` discovery or validation error.
    Policy(PolicyError),
    /// Local filesystem I/O error while scanning or reading workspace files.
    Io {
        /// Workspace-relative path or root directory.
        path: String,
        /// Underlying OS error description.
        message: String,
    },
}

impl fmt::Display for SurveyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConflictingContextFlags => write!(
                f,
                "Conflicting survey options: `--survey-context` cannot be combined with `--no-survey-context`."
            ),
            Self::InvalidCeiling { name, value } => write!(
                f,
                "Invalid `{name}` value `{value}`: expected a positive integer >= 1."
            ),
            Self::InvalidContextGlob { pattern, message } => {
                write!(f, "Invalid `--survey-context` glob `{pattern}`: {message}")
            }
            Self::PathEscapesWorkspace { path } => write!(
                f,
                "Survey context pattern `{path}` escapes workspace root (leading `/` and `../` traversal are forbidden)."
            ),
            Self::Policy(err) => write!(f, "{err}"),
            Self::Io { path, message } => {
                write!(f, "Failed to read workspace path `{path}`: {message}")
            }
        }
    }
}

impl std::error::Error for SurveyError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateKind {
    File,
    OutsideWorkspace,
}

/// Identifies which repository-survey character ceiling knob is being resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurveyCeilingKind {
    /// Per-file character ceiling (`--survey-max-file-chars` / `CANON_CLERK_SURVEY_MAX_FILE_CHARS`, default `4000`).
    MaxFileChars,
    /// Aggregate character ceiling (`--survey-max-total-chars` / `CANON_CLERK_SURVEY_MAX_TOTAL_CHARS`, default `12000`).
    MaxTotalChars,
}

impl SurveyCeilingKind {
    /// Returns the CLI flag name associated with this ceiling.
    pub const fn flag_name(self) -> &'static str {
        match self {
            Self::MaxFileChars => "--survey-max-file-chars",
            Self::MaxTotalChars => "--survey-max-total-chars",
        }
    }

    /// Returns the environment variable key associated with this ceiling.
    pub const fn env_key(self) -> &'static str {
        match self {
            Self::MaxFileChars => ENV_SURVEY_MAX_FILE_CHARS,
            Self::MaxTotalChars => ENV_SURVEY_MAX_TOTAL_CHARS,
        }
    }

    /// Returns the built-in positive default ceiling value.
    pub const fn default_value(self) -> NonZeroUsize {
        match self {
            Self::MaxFileChars => DEFAULT_SURVEY_MAX_FILE_CHARS_NONZERO,
            Self::MaxTotalChars => DEFAULT_SURVEY_MAX_TOTAL_CHARS_NONZERO,
        }
    }
}

/// Resolves a positive integer character ceiling (`>= 1`) for `kind` using standard CLI > env > default precedence.
///
/// - When `cli_value` is `Some(value)`, returns `value` immediately without consulting `env`.
/// - When `cli_value` is `None`, looks up `kind.env_key()` in `env` (trimming surrounding whitespace) and parses it
///   as a positive integer.
/// - When `cli_value` is `None` and `kind.env_key()` is absent or blank in `env`, falls back to `kind.default_value()`.
///
/// # Errors
///
/// Returns [`SurveyError::InvalidCeiling`] when `cli_value` is `None` and a non-empty environment value
/// is `0` or cannot be parsed as a positive integer.
pub fn resolve_survey_ceiling(
    kind: SurveyCeilingKind,
    cli_value: Option<NonZeroUsize>,
    env: &BTreeMap<String, String>,
) -> Result<NonZeroUsize, SurveyError> {
    if let Some(val) = cli_value {
        return Ok(val);
    }

    let env_key = kind.env_key();
    if let Some(raw) = env.get(env_key).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let parsed = raw
            .parse::<NonZeroUsize>()
            .map_err(|_| SurveyError::InvalidCeiling {
                name: env_key,
                value: raw.to_string(),
            })?;
        return Ok(parsed);
    }

    Ok(kind.default_value())
}

/// Executes the deterministic `survey` stage against a trusted workspace checkout on disk,
/// populating `.survey` on the cumulative [`Caseload`] while preserving all existing namespaces.
///
/// When `incoming` is `None`, starts from a fresh empty [`Caseload`]; when `Some(caseload)`,
/// enriches `.survey` while preserving all other populated stage namespaces.
///
/// # Errors
///
/// Returns [`SurveyError`] on conflicting `--survey-context` / `--no-survey-context` flags,
/// malformed `<scope>/.canons/policy-chains.yaml` files, invalid/escaping `--survey-context` globs,
/// or local workspace I/O errors.
pub fn execute_survey_on_disk(
    options: &SurveyOptions,
    incoming: Option<Caseload>,
    workspace_root: &Path,
) -> Result<Caseload, SurveyError> {
    if options.no_survey_context && !options.context_globs.is_empty() {
        return Err(SurveyError::ConflictingContextFlags);
    }

    let override_globs = compile_override_globs(&options.context_globs)?;
    let base = incoming.unwrap_or_default();

    if options.no_survey_context {
        return Ok(base.with_survey(CaseloadSurvey {
            policy_files: Vec::new(),
            max_file_chars: options.max_file_chars.get(),
            max_total_chars: options.max_total_chars.get(),
            total_chars: 0,
            files: BTreeMap::new(),
            omitted: BTreeMap::new(),
        }));
    }

    let canonical_root = fs::canonicalize(workspace_root).map_err(|err| SurveyError::Io {
        path: workspace_root.display().to_string(),
        message: err.to_string(),
    })?;

    let (policy_set, policy_files) = if override_globs.is_empty() {
        let discovered =
            discover_policy_files_on_disk(workspace_root).map_err(SurveyError::Policy)?;
        for nested in &discovered.nested_paths {
            tracing::warn!(
                path = %nested,
                "Ignoring nested policy-chains.yaml inside .canons/ subdirectory"
            );
        }
        let compiled =
            WorkspacePolicySet::compile(&discovered.files).map_err(SurveyError::Policy)?;
        let paths = compiled.policy_files().to_vec();
        (Some(compiled), paths)
    } else {
        (None, Vec::new())
    };

    let mut candidates = BTreeMap::new();
    walk_survey_candidates(
        workspace_root,
        &canonical_root,
        workspace_root,
        &mut candidates,
    )?;

    let survey = materialize_survey_candidates(
        workspace_root,
        options,
        policy_files,
        &override_globs,
        policy_set.as_ref(),
        candidates,
    )?;

    Ok(base.with_survey(survey))
}

fn materialize_survey_candidates(
    workspace_root: &Path,
    options: &SurveyOptions,
    policy_files: Vec<String>,
    override_globs: &[GlobPattern],
    policy_set: Option<&WorkspacePolicySet>,
    candidates: BTreeMap<String, CandidateKind>,
) -> Result<CaseloadSurvey, SurveyError> {
    let mut files = BTreeMap::new();
    let mut omitted = BTreeMap::new();
    let mut total_chars: usize = 0;

    for (rel_path, kind) in candidates {
        let matched = if !override_globs.is_empty() {
            override_globs.iter().any(|g| g.is_match(&rel_path))
        } else {
            policy_set.is_some_and(|ps| ps.allows_survey_file(&rel_path))
        };
        if !matched {
            continue;
        }

        if kind == CandidateKind::OutsideWorkspace {
            omitted.insert(rel_path, SurveyOmissionReason::OutsideWorkspace);
            continue;
        }

        let full_path = workspace_root.join(&rel_path);
        let bytes = fs::read(&full_path).map_err(|err| SurveyError::Io {
            path: rel_path.clone(),
            message: err.to_string(),
        })?;

        if bytes.contains(&0) {
            omitted.insert(rel_path, SurveyOmissionReason::Binary);
            continue;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            omitted.insert(rel_path, SurveyOmissionReason::Binary);
            continue;
        };

        let remaining = options.max_total_chars.get().saturating_sub(total_chars);
        if remaining == 0 {
            omitted.insert(rel_path, SurveyOmissionReason::BudgetExceeded);
            continue;
        }

        let original_chars = text.chars().count();
        let limit = options.max_file_chars.get().min(remaining);
        let (content, chars, truncated) = if original_chars > limit {
            (text.chars().take(limit).collect(), limit, true)
        } else {
            (text, original_chars, false)
        };

        total_chars += chars;
        files.insert(
            rel_path,
            SurveyFile {
                content,
                chars,
                original_chars,
                truncated,
            },
        );
    }

    Ok(CaseloadSurvey {
        policy_files,
        max_file_chars: options.max_file_chars.get(),
        max_total_chars: options.max_total_chars.get(),
        total_chars,
        files,
        omitted,
    })
}

fn compile_override_globs(raw_patterns: &[String]) -> Result<Vec<GlobPattern>, SurveyError> {
    let mut compiled = Vec::with_capacity(raw_patterns.len());
    for raw in raw_patterns {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(SurveyError::InvalidContextGlob {
                pattern: raw.clone(),
                message: "glob pattern must not be empty".to_string(),
            });
        }
        let posix = trimmed.replace('\\', "/");
        if posix.starts_with('/') || posix.split('/').any(|seg| seg == "..") {
            return Err(SurveyError::PathEscapesWorkspace { path: raw.clone() });
        }
        let normalized = posix.trim_start_matches("./");
        let glob =
            GlobPattern::parse(normalized).map_err(|err| SurveyError::InvalidContextGlob {
                pattern: raw.clone(),
                message: err.to_string(),
            })?;
        compiled.push(glob);
    }
    Ok(compiled)
}

fn is_canons_dir_path(rel_posix: &str) -> bool {
    rel_posix.split('/').any(|seg| seg == ".canons")
}

fn walk_survey_candidates(
    root: &Path,
    canonical_root: &Path,
    current_dir: &Path,
    out: &mut BTreeMap<String, CandidateKind>,
) -> Result<(), SurveyError> {
    let entries = match fs::read_dir(current_dir) {
        Ok(iter) => iter,
        Err(err) => {
            if current_dir == root {
                return Err(SurveyError::Io {
                    path: current_dir.display().to_string(),
                    message: err.to_string(),
                });
            }
            return Ok(());
        }
    };

    let mut sorted_entries: Vec<_> = entries.filter_map(Result::ok).collect();
    sorted_entries.sort_by_key(|e| e.file_name());

    for entry in sorted_entries {
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let rel_posix = rel.to_string_lossy().replace('\\', "/");
        if rel_posix.is_empty() || is_ignored_path(&rel_posix) || is_canons_dir_path(&rel_posix) {
            continue;
        }

        let Ok(sym_meta) = fs::symlink_metadata(&path) else {
            continue;
        };

        if sym_meta.file_type().is_symlink() {
            match fs::canonicalize(&path) {
                Ok(resolved) if resolved.starts_with(canonical_root) => {
                    if resolved.is_file() {
                        out.insert(rel_posix, CandidateKind::File);
                    }
                }
                _ => {
                    out.insert(rel_posix, CandidateKind::OutsideWorkspace);
                }
            }
        } else if sym_meta.is_dir() {
            walk_survey_candidates(root, canonical_root, &path, out)?;
        } else if sym_meta.is_file() {
            out.insert(rel_posix, CandidateKind::File);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_workspace(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("canon_clerk_survey_{label}_{nanos}"));
        fs::create_dir_all(&dir).expect("create temp workspace");
        dir
    }

    #[test]
    fn test_greenfield_zero_match_returns_empty_survey() {
        let root = make_temp_workspace("greenfield");
        fs::write(root.join("src_notes.txt"), "not a default file").unwrap();

        let caseload =
            execute_survey_on_disk(&SurveyOptions::default(), None, &root).expect("survey");
        let _ = fs::remove_dir_all(&root);

        let survey = caseload.survey.expect("survey namespace populated");
        assert!(survey.files.is_empty());
        assert!(survey.omitted.is_empty());
        assert!(survey.policy_files.is_empty());
        assert_eq!(survey.total_chars, 0);
        assert_eq!(survey.max_file_chars, DEFAULT_SURVEY_MAX_FILE_CHARS);
        assert_eq!(survey.max_total_chars, DEFAULT_SURVEY_MAX_TOTAL_CHARS);
    }

    #[test]
    fn test_default_stack_and_lexicographical_truncation_ceilings() {
        let root = make_temp_workspace("ceilings");
        fs::write(root.join("Cargo.toml"), "abcdefghij").unwrap(); // 10 chars
        fs::write(root.join("README.md"), "0123456789").unwrap(); // 10 chars
        fs::write(root.join("llms.txt"), "zzzzzzzzzz").unwrap(); // 10 chars

        let opts = SurveyOptions {
            max_file_chars: NonZeroUsize::new(6).expect("non-zero"),
            max_total_chars: NonZeroUsize::new(10).expect("non-zero"),
            ..SurveyOptions::default()
        };
        let caseload = execute_survey_on_disk(&opts, None, &root).expect("survey");
        let _ = fs::remove_dir_all(&root);

        let survey = caseload.survey.expect("survey");
        let keys: Vec<String> = survey.files.keys().cloned().collect();
        // Lexicographical order: Cargo.toml (6 chars), README.md (4 remaining chars), llms.txt (budget_exceeded)
        assert_eq!(keys, vec!["Cargo.toml", "README.md"]);
        assert_eq!(survey.files["Cargo.toml"].content, "abcdef");
        assert_eq!(survey.files["Cargo.toml"].chars, 6);
        assert_eq!(survey.files["Cargo.toml"].original_chars, 10);
        assert!(survey.files["Cargo.toml"].truncated);

        assert_eq!(survey.files["README.md"].content, "0123");
        assert_eq!(survey.files["README.md"].chars, 4);
        assert_eq!(survey.files["README.md"].original_chars, 10);
        assert!(survey.files["README.md"].truncated);

        assert_eq!(survey.total_chars, 10);
        assert_eq!(
            survey.omitted.get("llms.txt"),
            Some(&SurveyOmissionReason::BudgetExceeded)
        );
    }

    #[test]
    fn test_binary_and_symlink_escape_omissions() {
        let root = make_temp_workspace("omissions");
        let outside = make_temp_workspace("outside");
        let secret_file = outside.join("secret.txt");
        fs::write(&secret_file, "outside secret").unwrap();

        fs::write(root.join("README.md"), b"binary\0payload").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&secret_file, root.join("llms.txt")).unwrap();

        let caseload =
            execute_survey_on_disk(&SurveyOptions::default(), None, &root).expect("survey");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);

        let survey = caseload.survey.expect("survey");
        assert!(survey.files.is_empty());
        assert_eq!(
            survey.omitted.get("README.md"),
            Some(&SurveyOmissionReason::Binary)
        );
        #[cfg(unix)]
        assert_eq!(
            survey.omitted.get("llms.txt"),
            Some(&SurveyOmissionReason::OutsideWorkspace)
        );
    }

    #[test]
    fn test_nested_policy_chains_and_subordinate_overrides() {
        let root = make_temp_workspace("policy");
        fs::create_dir_all(root.join(".canons")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::create_dir_all(root.join("packages/vendored/.canons")).unwrap();

        fs::write(root.join("README.md"), "root readme").unwrap();
        fs::write(root.join("docs/arch.md"), "arch doc").unwrap();
        fs::write(root.join("packages/vendored/README.md"), "vendored").unwrap();
        fs::write(
            root.join(".canons/policy-chains.yaml"),
            "survey-context-chain:\n  - allow: 'docs/arch.md'\n  - allow: 'packages/*/README.md'\n",
        )
        .unwrap();
        fs::write(
            root.join("packages/vendored/.canons/policy-chains.yaml"),
            "survey-context-chain:\n  - deny: all\n",
        )
        .unwrap();

        let caseload =
            execute_survey_on_disk(&SurveyOptions::default(), None, &root).expect("policy survey");
        let survey = caseload.survey.expect("survey");
        let keys: Vec<String> = survey.files.keys().cloned().collect();
        assert_eq!(keys, vec!["README.md", "docs/arch.md"]);
        assert_eq!(
            survey.policy_files,
            vec![
                ".canons/policy-chains.yaml",
                "packages/vendored/.canons/policy-chains.yaml"
            ]
        );

        // Subordinate --survey-context overrides policy-chains and built-in defaults
        let override_opts = SurveyOptions {
            context_globs: vec!["docs/*.md".to_string()],
            ..SurveyOptions::default()
        };
        let override_caseload =
            execute_survey_on_disk(&override_opts, None, &root).expect("override survey");
        let override_survey = override_caseload.survey.expect("override survey");
        assert_eq!(
            override_survey.files.keys().cloned().collect::<Vec<_>>(),
            vec!["docs/arch.md"]
        );
        assert!(override_survey.policy_files.is_empty());

        // --no-survey-context yields explicit empty survey
        let disabled_opts = SurveyOptions {
            no_survey_context: true,
            ..SurveyOptions::default()
        };
        let disabled_caseload =
            execute_survey_on_disk(&disabled_opts, None, &root).expect("disabled survey");
        let disabled_survey = disabled_caseload.survey.expect("disabled survey");
        assert!(disabled_survey.files.is_empty());
        assert_eq!(disabled_survey.total_chars, 0);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_rejects_traversal_and_zero_ceilings() {
        let root = make_temp_workspace("invalid");

        let traversal = SurveyOptions {
            context_globs: vec!["../secret.txt".to_string()],
            ..SurveyOptions::default()
        };
        assert!(matches!(
            execute_survey_on_disk(&traversal, None, &root),
            Err(SurveyError::PathEscapesWorkspace { .. })
        ));

        let absolute = SurveyOptions {
            context_globs: vec!["/etc/passwd".to_string()],
            ..SurveyOptions::default()
        };
        assert!(matches!(
            execute_survey_on_disk(&absolute, None, &root),
            Err(SurveyError::PathEscapesWorkspace { .. })
        ));

        let mut zero_env = BTreeMap::new();
        zero_env.insert(ENV_SURVEY_MAX_FILE_CHARS.to_string(), "0".to_string());
        assert!(matches!(
            resolve_survey_ceiling(SurveyCeilingKind::MaxFileChars, None, &zero_env),
            Err(SurveyError::InvalidCeiling { .. })
        ));

        let conflict = SurveyOptions {
            context_globs: vec!["README.md".to_string()],
            no_survey_context: true,
            ..SurveyOptions::default()
        };
        assert!(matches!(
            execute_survey_on_disk(&conflict, None, &root),
            Err(SurveyError::ConflictingContextFlags)
        ));

        let _ = fs::remove_dir_all(&root);
    }
}
