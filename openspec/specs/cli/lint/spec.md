# Canon Lint CLI Specification

## Purpose

Defines the static canon linting subcommand (`canon-clerk lint`), target operand handling, standard input ingestion, traversal options, terminal formatting, canonical JSON reporting, and sensitivity controls.

## Requirements

### Requirement: Primary Target Operands and Standard Input Content
The `lint` command SHALL accept optional positional file paths, directories, or globs as primary target operands. When `-` is provided as a target operand, the command SHALL read raw Markdown document content from standard input (`stdin`) and evaluate it using schema rule evaluation. An optional `--stdin-filename <path>` option SHALL be supported to provide a virtual relative path for rules that evaluate file path and naming conventions. When target operands are omitted, linting SHALL target the entire workspace, discovering all `.canons/**/*.md` files.

#### Scenario: Linting explicit target paths
- **WHEN** invoking `canon-clerk lint .canons/pr-tests.md`
- **THEN** only the specified canon file is evaluated

#### Scenario: Linting document content from standard input
- **WHEN** piping Markdown content into `canon-clerk lint -`
- **THEN** the piped content is read from stdin and evaluated as a canon document

#### Scenario: Providing virtual filename for stdin content
- **WHEN** piping Markdown content into `canon-clerk lint - --stdin-filename .canons/pr-tests.md`
- **THEN** the piped content is evaluated against filename and frontmatter path rules using the virtual path

#### Scenario: Defaulting to workspace root
- **WHEN** invoking `canon-clerk lint` with no target operands
- **THEN** all canon files in the workspace are discovered and evaluated

### Requirement: Traversal and Discovery Configuration Flags
The `lint` command SHALL support `-g, --glob <pattern>` to configure the canon discovery glob pattern (defaulting to `**/.canons/**/*.md`). The command SHALL support `--ignore <pattern>` (additive to defaults, repeatable) and `--default-ignores` / `--no-default-ignores` (boolean, defaulting to true) to configure workspace discovery exclusions adhering to `.gitignore` semantics.

#### Scenario: Overriding discovery glob pattern with custom pattern
- **WHEN** invoking `canon-clerk lint tmp -g "**/*.md"`
- **THEN** all markdown files in `tmp` matching `**/*.md` are discovered and evaluated

#### Scenario: Applying custom additive ignore patterns
- **WHEN** invoking `canon-clerk lint --ignore "experimental/**"`
- **THEN** matching files are excluded from discovery in addition to default ignores

#### Scenario: Disabling default directory ignores
- **WHEN** invoking `canon-clerk lint --no-default-ignores`
- **THEN** default noise directories are not pruned during discovery

### Requirement: Formatted Terminal Reporting
When configured with `--format stylish` (default), the `lint` command SHALL emit human-readable diagnostic reports grouped by relative POSIX file path with aligned coordinates, severity badges, rule identifiers, and actionable remediation hints. Results SHALL be stably sorted by file path. When zero violations occur, the command SHALL print a clean status indicator in interactive terminals and remain silent in non-interactive streams.

#### Scenario: Reporting violations with remediation hints
- **WHEN** linting a workspace with canon violations in stylish format
- **THEN** diagnostics are displayed grouped by file with coordinates, badges, rule IDs, and indented hints

#### Scenario: Formatting clean workspace run in TTY
- **WHEN** linting a clean workspace in an interactive TTY session
- **THEN** a green success indicator with total passed canons is displayed

#### Scenario: Remaining silent on clean run in non-interactive pipeline
- **WHEN** linting a clean workspace in a non-interactive piped environment
- **THEN** no output is written to stdout and the process exits with status 0

### Requirement: Canonical JSON Reporting
When configured with `--format json` or `--json`, the `lint` command SHALL emit a canonical JSON array of `FileLintResult` objects to stdout containing only files with diagnostics (emitting `[]` on a clean workspace). Output MUST be valid, unadorned JSON with zero ANSI escape codes.

#### Scenario: Emitting JSON array with violations
- **WHEN** invoking `canon-clerk lint --json` on a workspace with rule violations
- **THEN** a valid JSON array of `FileLintResult` objects is emitted containing only files with diagnostics

#### Scenario: Emitting JSON array for clean workspace
- **WHEN** invoking `canon-clerk lint --json` on a clean workspace
- **THEN** `[]` is emitted to stdout and the process exits with status 0

### Requirement: Warning Sensitivity and Quiet Mode
The `lint` command SHALL support `--quiet` (`-q`) to suppress warning diagnostics from terminal and JSON output. The command SHALL support `--max-warnings <n>` to trigger exit status 1 whenever the emitted warning count exceeds `<n>`.

#### Scenario: Suppressing warnings via quiet mode
- **WHEN** invoking `canon-clerk lint --quiet` on a workspace with warnings and no errors
- **THEN** warnings are omitted from output and the process exits with status 0

#### Scenario: Failing exit threshold when warnings exceed limit
- **WHEN** invoking `canon-clerk lint --max-warnings 0` on a workspace with 1 warning
- **THEN** the process terminates with exit status 1 and reports threshold exceeded

#### Scenario: Passing when warnings do not exceed limit
- **WHEN** invoking `canon-clerk lint --max-warnings 5` on a workspace with 2 warnings
- **THEN** warnings are displayed and the process terminates with exit status 0

### Requirement: Grouped Help Screen with Runnable Examples
The `lint` command SHALL provide a help screen (`--help`, `-h`) that organizes options into categorized sections (Targets & Filtering, Output & Reporting, Sensitivity & Thresholds, General) and includes copy-pasteable runnable examples.

#### Scenario: Displaying grouped lint help screen
- **WHEN** invoking `canon-clerk lint --help`
- **THEN** option descriptions are printed grouped by category alongside runnable usage examples
