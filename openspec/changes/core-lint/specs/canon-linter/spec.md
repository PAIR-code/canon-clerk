# Spec Delta

## Purpose

Extends the `canon-linter` capability with workspace-wide static linting orchestration and structured diagnostic aggregation in `@canon-clerk/core` (`src/linter.ts`), consuming discovered canons and executing `@canon-clerk/schema`'s pure rule engine.

## ADDED Requirements

### Requirement: Workspace Canon Linting Orchestration
The system SHALL provide a workspace linting service (`lintWorkspace`) in `@canon-clerk/core` that discovers canon files, reads raw contents from disk, evaluates them against static lint rules via `lintCanon()`, and aggregates diagnostics into structured file results. Files with zero diagnostics SHALL be included in the file results list with empty diagnostic arrays.

#### Scenario: Linting a clean workspace
- **WHEN** linting a workspace where all discovered canons conform to active rules
- **THEN** each file result contains zero diagnostics, total error and warning counts are zero, and `hasErrors` is false

#### Scenario: Aggregating multi-file diagnostics
- **WHEN** linting a workspace containing rule violations across multiple canon files
- **THEN** file results retain source coordinates and messages, and workspace totals accurately sum all errors and warnings

#### Scenario: Handling unreadable or missing target files
- **WHEN** a specified target canon file does not exist or cannot be read from disk
- **THEN** the system reports an error diagnostic associated with the file path without crashing the runner

### Requirement: Structured Presentation-Agnostic Lint Results
The system SHALL return workspace linting results as structured data objects (`WorkspaceLintResult`) containing individual `FileLintResult` records, total file counts, total error counts, total warning counts, and a boolean `hasErrors` flag. Output MUST remain strictly presentation-agnostic with zero terminal styling or ANSI codes.

#### Scenario: Calculating summary counts
- **WHEN** workspace linting finishes across multiple files with mixed errors and warnings
- **THEN** `totalFiles` equals the number of processed files, `errorCount` equals the total errors, `warningCount` equals total warnings, and `hasErrors` is true if `errorCount > 0`

#### Scenario: Presentation agnosticism in core results
- **WHEN** inspecting messages or file paths in `WorkspaceLintResult`
- **THEN** no ANSI color escape sequences or terminal formatting codes are present
