# Canon Check-Canons CLI Specification

## Purpose

Defines the Phase 1 static canon checking subcommand (`canon-clerk check-canons`), target operand handling, standard input ingestion, traversal options, terminal formatting, canonical JSON reporting, and sensitivity controls.

## Requirements

### Requirement: Primary Target Operands and Standard Input Content
The `check-canons` command SHALL accept optional positional file paths, directories, or globs as primary target operands. When `-` is provided as a target operand, the command SHALL read raw Markdown document content from standard input (`stdin`) and evaluate it using schema rule evaluation. An optional `--stdin-filename <path>` option SHALL provide a virtual relative path for path rules. When target operands are omitted, check-canons SHALL target the entire workspace (`.canons/**/*.md`).

#### Scenario: Checking explicit target paths
- **WHEN** invoking `canon-clerk check-canons .canons/pr-tests.md`
- **THEN** only the specified canon file is evaluated

#### Scenario: Checking document content from standard input
- **WHEN** piping Markdown content into `canon-clerk check-canons -`
- **THEN** the piped content is read from stdin and evaluated as a canon document

#### Scenario: Providing virtual filename for stdin content
- **WHEN** piping Markdown content into `canon-clerk check-canons - --stdin-filename .canons/pr-tests.md`
- **THEN** the piped content is evaluated using the virtual path context

#### Scenario: Defaulting to workspace root
- **WHEN** invoking `canon-clerk check-canons` with no target operands
- **THEN** all canon files in the workspace are discovered and evaluated

### Requirement: Traversal and Discovery Configuration Flags
The `check-canons` command SHALL support `-g, --glob <pattern>` to configure the canon discovery glob pattern (defaulting to `**/.canons/**/*.md`). The command SHALL support `--ignore <pattern>` (additive to defaults, repeatable) and `--default-ignores` / `--no-default-ignores` (boolean, defaulting to true) to configure workspace discovery exclusions adhering to `.gitignore` semantics.

#### Scenario: Overriding discovery glob pattern with custom pattern
- **WHEN** invoking `canon-clerk check-canons tmp -g "**/*.md"`
- **THEN** all markdown files in `tmp` matching `**/*.md` are discovered and evaluated

#### Scenario: Applying custom additive ignore patterns
- **WHEN** invoking `canon-clerk check-canons --ignore "experimental/**"`
- **THEN** matching files are excluded from discovery in addition to default ignores

#### Scenario: Disabling default directory ignores
- **WHEN** invoking `canon-clerk check-canons --no-default-ignores`
- **THEN** default noise directories are not pruned during discovery

### Requirement: Formatted Terminal Reporting
When configured with `--format stylish` (default), the `check-canons` command SHALL emit human-readable diagnostic reports grouped by relative POSIX file path with aligned coordinates, severity badges, rule identifiers, and actionable remediation hints. Results SHALL be stably sorted by file path. When zero violations occur, the command SHALL print a clean status indicator in interactive terminals and remain silent in non-interactive streams.

#### Scenario: Reporting violations with remediation hints
- **WHEN** checking a workspace with canon violations in stylish format
- **THEN** diagnostics are displayed grouped by file with coordinates, badges, rule IDs, and indented hints

#### Scenario: Formatting clean workspace run in TTY
- **WHEN** checking a clean workspace in an interactive TTY session
- **THEN** a green success indicator with total passed canons is displayed

#### Scenario: Remaining silent on clean run in non-interactive pipeline
- **WHEN** checking a clean workspace in a non-interactive piped environment
- **THEN** no output is written to stdout and the process exits with status 0

### Requirement: Canonical JSON Reporting
When configured with `--format json` or `--json`, the `check-canons` command SHALL emit a canonical JSON array of `FileLintResult` objects to stdout containing only files with diagnostics (emitting `[]` on a clean workspace). Output MUST be valid, unadorned JSON with zero ANSI escape codes.

#### Scenario: Emitting JSON results on workspace with violations
- **WHEN** invoking `canon-clerk check-canons --json` on a workspace with rule violations
- **THEN** an unadorned JSON array of file results is emitted to stdout

#### Scenario: Emitting empty JSON array on clean workspace
- **WHEN** invoking `canon-clerk check-canons --json` on a clean workspace
- **THEN** an empty JSON array `[]` is emitted to stdout and the process exits with status 0

### Requirement: Warning Thresholds and Exit Code Sensitivity
The `check-canons` command SHALL exit with status 0 when zero errors and zero warnings are present. When errors are detected, the command SHALL exit with status 1. When only warnings are present, the command SHALL exit with status 0 by default, or status 1 when warnings exceed `--max-warnings <number>`. When `--quiet` (`-q`) is passed, warning diagnostics SHALL be suppressed from terminal output.

#### Scenario: Suppressing warnings with quiet flag
- **WHEN** invoking `canon-clerk check-canons --quiet` on a workspace with warnings and no errors
- **THEN** warning diagnostics are omitted from output and the process exits with status 0

#### Scenario: Failing on warnings exceeding max-warnings threshold
- **WHEN** invoking `canon-clerk check-canons --max-warnings 0` on a workspace with 1 warning
- **THEN** warning diagnostics are printed to stdout and the process exits with status 1

#### Scenario: Passing when warnings are within max-warnings threshold
- **WHEN** invoking `canon-clerk check-canons --max-warnings 5` on a workspace with 2 warnings
- **THEN** warning diagnostics are printed to stdout and the process exits with status 0

### Requirement: Grouped Help Screen and Usage Documentation
The `check-canons` command SHALL support `--help` (`-h`). When requested, the command SHALL output categorized usage instructions grouped into Targets & Filtering, Output & Reporting, Sensitivity & Thresholds, and General sections, accompanied by runnable command examples.

#### Scenario: Displaying categorized help screen
- **WHEN** invoking `canon-clerk check-canons --help`
- **THEN** categorized help with runnable examples is printed to stdout and the process exits with status 0
