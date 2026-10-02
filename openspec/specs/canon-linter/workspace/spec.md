# canon-linter/workspace Specification

## Purpose

Defines streaming workspace canon linting orchestration in `@canon-clerk/core`, filesystem traversal, ignore resolution adhering to `.gitignore` semantics, and presentation-agnostic file lint results.

## Requirements

### Requirement: Streaming Workspace Canon Linting Orchestration
The system SHALL provide a streaming canon linting generator (`lintCanons`) in `@canon-clerk/core` accepting `options: LintCanonsOptions` and yielding `FileLintResult` records in deterministic lexicographic order.
`workspaceRoot: string` SHALL be required, anchoring all path resolution without relying on ambient `process.cwd`.
Discovery scope SHALL be specified via `targetPaths` (defaulting to `['.']`).
Canon discovery SHALL be configured via `canonQuery?: QueryDomainInput`.

#### Scenario: Linting a clean workspace
- **WHEN** invoking `lintCanons({ workspaceRoot: '.' })` on a workspace where all discovered canons conform to active rules
- **THEN** each yielded file result contains zero diagnostics and error/warning counts of zero

#### Scenario: Deterministic lexicographic yield order
- **WHEN** linting a workspace containing canons across multiple directories
- **THEN** file results are yielded in strictly ascending lexicographic order by relative POSIX path

#### Scenario: Yielding multi-file diagnostics
- **WHEN** linting a workspace containing rule violations across multiple canon files
- **THEN** file results retain source coordinates and messages for each evaluated file

#### Scenario: Handling unreadable or missing target files
- **WHEN** a specified target canon file does not exist or cannot be read from disk
- **THEN** the system yields an error diagnostic associated with the file path without crashing the runner

#### Scenario: Custom directory and path ignores
- **WHEN** linting a workspace with custom ignore paths or patterns specified via `canonQuery.ignores`
- **THEN** matching directories and files are ignored during discovery and linting

#### Scenario: Explicit target precedence over default ignores
- **WHEN** linting with an explicit target in `targetPaths` pointing to a canon file inside a default-ignored directory
- **THEN** the explicitly targeted canon file is included in linting results

#### Scenario: Linting a collection of explicit file targets
- **WHEN** invoking `lintCanons({ workspaceRoot: '.', targetPaths: ["foo/first.md", "bar/second.md"] })`
- **THEN** targets are sorted and evaluated in ascending lexicographic order (`bar/second.md` before `foo/first.md`)

#### Scenario: Sorting resolved targets regardless of input syntax
- **WHEN** invoking `lintCanons({ workspaceRoot: '.', targetPaths: ["././foo/first.md", "./bar/second.md"] })`
- **THEN** targets are normalized and evaluated in resolved lexicographic order (`bar/second.md` before `foo/first.md`)

#### Scenario: Discovering canons with custom glob pattern
- **WHEN** invoking `lintCanons({ workspaceRoot: '.', canonQuery: "**/*.md" })` on a workspace with markdown files outside `.canons/`
- **THEN** all matching markdown files are discovered, evaluated, and yielded

#### Scenario: Evaluating multiple globs as union
- **WHEN** invoking `lintCanons({ workspaceRoot: '.', canonQuery: ["**/.canons/**/*.md", "docs/canons/**/*.md"] })`
- **THEN** markdown files matching either pattern are discovered and evaluated

#### Scenario: Scoping custom glob to target directory
- **WHEN** invoking `lintCanons({ workspaceRoot: '.', targetPaths: "tmp", canonQuery: "**/*.md" })`
- **THEN** markdown files inside `tmp` are discovered and evaluated

#### Scenario: Defaulting discovery pattern to `.canons`
- **WHEN** invoking `lintCanons({ workspaceRoot: '.' })` without specifying a custom `canonQuery` option
- **THEN** discovery only yields markdown files matching `**/.canons/**/*.md`

### Requirement: Structured Presentation-Agnostic File Lint Results
The system SHALL yield linting results as structured data objects (`FileLintResult`) containing the relative POSIX `filePath`, `diagnostics` array, `errorCount`, and `warningCount`. Output MUST remain strictly presentation-agnostic with zero terminal styling or ANSI codes.

#### Scenario: Presentation agnosticism in file results
- **WHEN** inspecting messages or file paths in `FileLintResult`
- **THEN** no ANSI color escape sequences or terminal formatting codes are present
