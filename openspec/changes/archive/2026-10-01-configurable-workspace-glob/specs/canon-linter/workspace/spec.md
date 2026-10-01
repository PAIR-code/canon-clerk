# Spec Delta

## MODIFIED Requirements

### Requirement: Streaming Workspace Canon Linting Orchestration
The system SHALL provide a streaming canon linting generator (`lintCanons`) in `@canon-clerk/core` accepting an optional `options` configuration object (`LintCanonsOptions`) and yielding individual `FileLintResult` records in deterministic lexicographic order by relative POSIX file path as canon files are discovered, read from disk, and evaluated against static lint rules via `lintCanon()`.
When `targets` is omitted, linting SHALL default to `['.']` relative to `cwd` (which defaults to `process.cwd()`).
Target paths SHALL be resolved relative to `cwd`, normalized to canonical POSIX paths, and sorted lexicographically before traversal and evaluation.
Discovery SHALL default to the pattern `**/.canons/**/*.md` and SHALL support custom discovery glob pattern(s) via the `globs` option (supporting single or multiple glob patterns evaluated as a logical OR).
Discovery MUST ignore default noise directories (`node_modules`, `dist`, `.bare`, `.git`, `.turbo`) unless disabled, support custom ignore patterns adhering to `.gitignore` semantics, and support target path filtering with explicit target precedence.
Files with zero diagnostics SHALL be yielded with empty diagnostic arrays.

#### Scenario: Linting a clean workspace
- **WHEN** invoking `lintCanons()` with no arguments on a workspace where all discovered canons conform to active rules
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
- **WHEN** linting a workspace with custom ignore paths or patterns specified in options adhering to `.gitignore` semantics
- **THEN** matching directories and files are ignored during discovery and linting

#### Scenario: Explicit target precedence over default ignores
- **WHEN** linting with an explicit target pointing to a canon file inside a default-ignored directory
- **THEN** the explicitly targeted canon file is included in linting results

#### Scenario: Linting a collection of explicit file targets
- **WHEN** invoking `lintCanons({ targets: ["foo/first.md", "bar/second.md"] })`
- **THEN** targets are sorted and evaluated in ascending lexicographic order (`bar/second.md` before `foo/first.md`)

#### Scenario: Sorting resolved targets regardless of input syntax
- **WHEN** invoking `lintCanons({ targets: ["././foo/first.md", "./bar/second.md"] })`
- **THEN** targets are normalized and evaluated in resolved lexicographic order (`bar/second.md` before `foo/first.md`)

#### Scenario: Discovering canons with custom glob pattern
- **WHEN** invoking `lintCanons({ globs: "**/*.md" })` on a workspace with markdown files outside `.canons/`
- **THEN** all matching markdown files are discovered, evaluated, and yielded

#### Scenario: Evaluating multiple globs as union
- **WHEN** invoking `lintCanons({ globs: ["**/.canons/**/*.md", "docs/canons/**/*.md"] })`
- **THEN** markdown files matching either pattern are discovered and evaluated

#### Scenario: Scoping custom glob to target directory
- **WHEN** invoking `lintCanons({ targets: "tmp", globs: "**/*.md" })`
- **THEN** markdown files inside `tmp` are discovered and evaluated

#### Scenario: Defaulting discovery pattern to `.canons`
- **WHEN** invoking `lintCanons` without specifying a custom `globs` option
- **THEN** discovery only yields markdown files matching `**/.canons/**/*.md`
