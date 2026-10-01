# Spec Delta
 
## Purpose
 
Extends the `canon-linter` capability with streaming workspace-wide static linting orchestration in `@canon-clerk/core` (`src/linter.ts`), discovering and evaluating canon files on the fly and yielding structured `FileLintResult` records.
 
## ADDED Requirements
 
### Requirement: Streaming Workspace Canon Linting Orchestration
The system SHALL provide a streaming workspace linting generator (`lintWorkspace`) in `@canon-clerk/core` that yields individual `FileLintResult` records as canon files are discovered, read from disk, and evaluated against static lint rules via `lintCanon()`.
Discovery MUST ignore default noise directories (`node_modules`, `dist`, `.bare`, `.git`, `.turbo`) unless disabled, support custom ignore patterns adhering to `.gitignore` semantics, and support target path filtering with explicit target precedence.
Files with zero diagnostics SHALL be yielded with empty diagnostic arrays.
 
#### Scenario: Linting a clean workspace
- **WHEN** linting a workspace where all discovered canons conform to active rules
- **THEN** each yielded file result contains zero diagnostics and error/warning counts of zero
 
#### Scenario: Yielding multi-file diagnostics
- **WHEN** linting a workspace containing rule violations across multiple canon files
- **THEN** file results retain source coordinates and messages for each evaluated file
 
#### Scenario: Handling unreadable or missing target files
- **WHEN** a specified target canon file does not exist or cannot be read from disk
- **THEN** the system yields an error diagnostic associated with the file path without crashing the runner
 
#### Scenario: Custom directory and path ignores
- **WHEN** linting a workspace with custom ignore paths or patterns specified in options adhering to `.gitignore` semantics
- **THEN** matching directories and files are ignored during discovery and linting
 
#### Scenario: Explicit target precedence over default ignores
- **WHEN** linting with an explicit target pointing to a canon file inside a default-ignored directory
- **THEN** the explicitly targeted canon file is included in linting results
 
### Requirement: Structured Presentation-Agnostic File Lint Results
The system SHALL yield linting results as structured data objects (`FileLintResult`) containing the relative POSIX `filePath`, `diagnostics` array, `errorCount`, and `warningCount`. Output MUST remain strictly presentation-agnostic with zero terminal styling or ANSI codes.
 
#### Scenario: Presentation agnosticism in file results
- **WHEN** inspecting messages or file paths in `FileLintResult`
- **THEN** no ANSI color escape sequences or terminal formatting codes are present

