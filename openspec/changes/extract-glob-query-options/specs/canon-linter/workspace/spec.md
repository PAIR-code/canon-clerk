# Spec Delta

## MODIFIED Requirements

### Requirement: Streaming Workspace Canon Linting Orchestration
The system SHALL provide a streaming canon linting generator (`lintCanons`) in `@canon-clerk/core` accepting `options: LintCanonsOptions` and yielding `FileLintResult` records in deterministic lexicographic order.
`workspaceRoot: string` SHALL be required, anchoring all path resolution without relying on ambient `process.cwd`.
Discovery scope SHALL be specified via `targetGlobs` (defaulting to `['.']`).
Canon discovery SHALL be configured via `canons?: QueryDomainInput`.

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
- **WHEN** linting a workspace with custom ignore paths or patterns specified via `canons.ignores`
- **THEN** matching directories and files are ignored during discovery and linting

#### Scenario: Explicit target precedence over default ignores
- **WHEN** linting with an explicit target in `targetGlobs` pointing to a canon file inside a default-ignored directory
- **THEN** the explicitly targeted canon file is included in linting results

#### Scenario: Linting a collection of explicit file targets
- **WHEN** invoking `lintCanons({ workspaceRoot: '.', targetGlobs: ["foo/first.md", "bar/second.md"] })`
- **THEN** targets are sorted and evaluated in ascending lexicographic order (`bar/second.md` before `foo/first.md`)

#### Scenario: Sorting resolved targets regardless of input syntax
- **WHEN** invoking `lintCanons({ workspaceRoot: '.', targetGlobs: ["././foo/first.md", "./bar/second.md"] })`
- **THEN** targets are normalized and evaluated in resolved lexicographic order (`bar/second.md` before `foo/first.md`)

#### Scenario: Discovering canons with custom glob pattern
- **WHEN** invoking `lintCanons({ workspaceRoot: '.', canons: "**/*.md" })` on a workspace with markdown files outside `.canons/`
- **THEN** all matching markdown files are discovered, evaluated, and yielded

#### Scenario: Evaluating multiple globs as union
- **WHEN** invoking `lintCanons({ workspaceRoot: '.', canons: ["**/.canons/**/*.md", "docs/canons/**/*.md"] })`
- **THEN** markdown files matching either pattern are discovered and evaluated

#### Scenario: Scoping custom glob to target directory
- **WHEN** invoking `lintCanons({ workspaceRoot: '.', targetGlobs: "tmp", canons: "**/*.md" })`
- **THEN** markdown files inside `tmp` are discovered and evaluated

#### Scenario: Defaulting discovery pattern to `.canons`
- **WHEN** invoking `lintCanons({ workspaceRoot: '.' })` without specifying a custom `canons` option
- **THEN** discovery only yields markdown files matching `**/.canons/**/*.md`
