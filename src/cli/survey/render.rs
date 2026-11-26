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

//! Stylish, compact, and JSON output renderers for `canon-clerk survey`.

use super::{SurveyArgs, SurveyFormat};
use crate::caseload::{Caseload, CaseloadSurvey};
use crate::cli::CliError;
use crate::cli::provenance::{StageDisposition, format_thousands, write_survey_provenance};
use std::io::Write;

pub(super) fn render_survey_output(
    args: &SurveyArgs,
    caseload: &Caseload,
    survey: &CaseloadSurvey,
    disp: StageDisposition,
    stdout: &mut dyn Write,
) -> Result<(), CliError> {
    let effective_format = if args.io.json {
        SurveyFormat::Json
    } else {
        args.format
    };

    match effective_format {
        SurveyFormat::Json => {
            let serialized = serde_json::to_string_pretty(caseload).map_err(|err| {
                CliError::runtime(format!("Failed to serialize Caseload JSON: {err}"))
            })?;
            writeln!(stdout, "{serialized}")
                .map_err(|err| CliError::runtime(format!("Failed to write to stdout: {err}")))?;
        }
        SurveyFormat::Compact => {
            render_compact(survey, stdout)
                .map_err(|err| CliError::runtime(format!("Failed to write to stdout: {err}")))?;
        }
        SurveyFormat::Stylish => {
            render_stylish(survey, disp, stdout)
                .map_err(|err| CliError::runtime(format!("Failed to write to stdout: {err}")))?;
        }
    }

    Ok(())
}

fn render_compact(survey: &CaseloadSurvey, stdout: &mut dyn Write) -> std::io::Result<()> {
    for (path, file) in &survey.files {
        let state = if file.truncated { "truncated" } else { "full" };
        writeln!(
            stdout,
            "file\t{path}\t{}\t{}\t{state}",
            file.chars, file.original_chars
        )?;
    }
    for (path, reason) in &survey.omitted {
        writeln!(stdout, "omitted\t{path}\t{reason}")?;
    }
    Ok(())
}

fn render_stylish(
    survey: &CaseloadSurvey,
    disp: StageDisposition,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    writeln!(stdout, "Pipeline Provenance:")?;
    write_survey_provenance(Some(survey), disp, stdout)?;
    writeln!(stdout)?;
    writeln!(
        stdout,
        "Repository Survey (ceilings: {} chars/file, {} chars total):",
        format_thousands(survey.max_file_chars),
        format_thousands(survey.max_total_chars)
    )?;

    if !survey.policy_files.is_empty() {
        writeln!(
            stdout,
            "  Policy Files : {}",
            survey.policy_files.join(", ")
        )?;
    }

    if survey.files.is_empty() {
        writeln!(stdout, "  Context Files: (none)")?;
    } else {
        writeln!(
            stdout,
            "  Context Files ({}, {} chars):",
            survey.files.len(),
            format_thousands(survey.total_chars)
        )?;
        for (path, file) in &survey.files {
            if file.truncated {
                writeln!(
                    stdout,
                    "    - {path} ({} / {} chars, truncated)",
                    format_thousands(file.chars),
                    format_thousands(file.original_chars)
                )?;
            } else {
                writeln!(
                    stdout,
                    "    - {path} ({} chars)",
                    format_thousands(file.chars)
                )?;
            }
        }
    }

    if !survey.omitted.is_empty() {
        writeln!(stdout, "  Omitted ({}):", survey.omitted.len())?;
        for (path, reason) in &survey.omitted {
            writeln!(stdout, "    - {path} ({reason})")?;
        }
    }

    Ok(())
}
