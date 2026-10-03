# Canon Check-Triggers CLI Specification

## Purpose

Defines the Phase 1 canon trigger checking subcommand (`canon-clerk check-triggers`), target and canon operand handling, standard input ingestion, traversal options, terminal formatting, canonical JSON reporting, and predicate exit codes.

## Requirements

### Requirement: Primary Target Operands and Naked Invocation Guardrails
The `check-triggers` command SHALL accept optional positional target file paths, directories, or globs, an explicit `-` operand to read line-delimited target paths from standard input, or `--all` to evaluate the entire workspace. Invocations omitting all target operands, stdin `-`, `--all`, and `--canon` SHALL print an error with remediation hint to stderr and exit with status 2.

#### Scenario: Specifying explicit target paths
- **WHEN** invoking `canon-clerk check-triggers packages/core/src/scope.ts packages/cli/src/app.ts`
- **THEN** only the specified target files are evaluated against candidate canons

#### Scenario: Reading target paths from standard input
- **WHEN** piping paths into `git diff --name-only | canon-clerk check-triggers -`
- **THEN** line-delimited paths are read from stdin and evaluated as target files

#### Scenario: Evaluating entire workspace via `--all`
- **WHEN** invoking `canon-clerk check-triggers --all`
- **THEN** all non-ignored workspace files are evaluated against discovered canons

#### Scenario: Rejecting naked invocation without targets or canons
- **WHEN** invoking `canon-clerk check-triggers` with no arguments or flags
- **THEN** a usage error and remediation hint are printed to stderr and the process exits with status 2

### Requirement: Symmetrical Canon Selection and Stdin Ingestion
The `check-triggers` command SHALL support `--canon <path>` and `--canons <path>` (repeatable, or `-` for stdin) and `--canon-glob <pattern>` (repeatable, default: `**/.canons/**/*.md`). Supplying `-` for both targets and `--canon` SHALL exit with status 2. Supplying `--canon` without targets SHALL default targets to all in-scope workspace files (`**/*`).

#### Scenario: Restricting evaluation to explicit canon file
- **WHEN** invoking `canon-clerk check-triggers src/Button.tsx --canon .canons/ui-tests.md`
- **THEN** only the specified canon is evaluated against the target file

#### Scenario: Reading canon paths from standard input
- **WHEN** piping canon paths into `canon-clerk check-triggers src/Button.tsx --canon -`
- **THEN** canon paths are read from stdin and evaluated against the target file

#### Scenario: Inverting query by specifying canon without targets
- **WHEN** invoking `canon-clerk check-triggers --canon packages/ui/.canons/prs-must-have-tests.md`
- **THEN** targets default to `**/*` and all matching workspace files in scope are returned

#### Scenario: Rejecting simultaneous standard input on targets and canons
- **WHEN** invoking `canon-clerk check-triggers - --canon -`
- **THEN** an error message is printed to stderr and the process exits with status 2

### Requirement: Traversal Ignore Hierarchy and Explicit Target Precedence
The command SHALL support `--ignore <pattern>` (repeatable) and `--default-ignores` / `--no-default-ignores` (default: true) applied globally, plus granular overrides `--target-ignore`, `--canon-ignore`, `--no-default-target-ignores`, and `--no-default-canon-ignores`. Explicit concrete target or canon paths SHALL bypass default noise ignores. Targets escaping workspace root SHALL exit with status 2.

#### Scenario: Bypassing default ignores for explicit concrete targets
- **WHEN** invoking `canon-clerk check-triggers node_modules/pkg/index.ts`
- **THEN** the concrete path is evaluated despite matching default noise ignores

#### Scenario: Applying granular target ignore patterns
- **WHEN** invoking `canon-clerk check-triggers src/**/*.ts --target-ignore "src/**/*.test.ts"`
- **THEN** test files are excluded from target evaluation while canon discovery remains unaffected

#### Scenario: Rejecting target paths escaping workspace root
- **WHEN** invoking `canon-clerk check-triggers ../outside.ts`
- **THEN** an error is printed to stderr and the process exits with status 2

### Requirement: Formatted Terminal Reporting
When configured with `--format stylish` (default), the command SHALL emit a human-readable tree grouped by canon, listing canon scope and triggering target files with matched trigger patterns. Scoped canons SHALL display their scope-relative identifier (`<scope>/<id>`), and global canons SHALL display `<id>`. Results SHALL be stably sorted. When zero canons match, the command SHALL exit 0.

#### Scenario: Displaying formatted tree with matched trigger attribution
- **WHEN** check-triggers finds active canon matches in stylish format
- **THEN** output groups targets under their governing canon, displaying scope and matched triggers

#### Scenario: Reporting global vs scoped canon identifiers
- **WHEN** displaying a scoped canon and a root global canon
- **THEN** the scoped canon displays `<scope>/<id>` while the global canon displays `<id>`

#### Scenario: Exiting cleanly with code 0 on zero matches
- **WHEN** check-triggers finds zero matching canons for targets in stylish format
- **THEN** a zero-match notice is printed to stdout and the process exits with status 0

### Requirement: Canonical JSON Reporting
When configured with `--format json` or `--json`, the command SHALL emit a canonical JSON array of match records containing `canonId`, `canonPath`, `title`, `scopePath`, `scopeRelativePath`, `matchedCanonPatterns`, and `matchedTargets` (including `targetPath`, `targetScopeRelativePath`, `targetRelativePath`, `matchedTargetPatterns`, and `matchedTriggers`). Clean runs with zero matches SHALL emit `[]`.

#### Scenario: Emitting canonical JSON array with three-plane attribution
- **WHEN** invoking `canon-clerk check-triggers --json src/Button.tsx` with matching canons
- **THEN** a valid JSON array of match records is printed to stdout containing three-plane pattern attribution

#### Scenario: Emitting empty JSON array on zero matches
- **WHEN** invoking `canon-clerk check-triggers --json src/Button.tsx` with zero matching canons
- **THEN** `[]` is emitted to stdout and the process exits with status 0

### Requirement: Predicate Mode
When configured with `--quiet` (`-q`), the command SHALL suppress all standard output and exit immediately with status 0 upon encountering the first canon activation tuple, or exit with status 1 if zero canons are activated. Predicate mode SHALL short-circuit evaluation without traversing remaining candidates.

#### Scenario: Exiting 0 on first triggered match in predicate mode
- **WHEN** invoking `canon-clerk check-triggers -q src/Button.tsx` where at least one canon matches
- **THEN** evaluation halts on the first match, nothing is printed to stdout, and the process exits with status 0

#### Scenario: Exiting 1 when zero canons trigger in predicate mode
- **WHEN** invoking `canon-clerk check-triggers -q src/Button.tsx` where zero canons match
- **THEN** nothing is printed to stdout and the process exits with status 1

### Requirement: Grouped Help Screen and Subcommand Routing
The command SHALL provide a help screen (`--help`, `-h`) that organizes options into categorized sections (Targets & Filtering, Canon Selection, Ignore Controls, Output & Presentation, Sensitivity, General) and includes runnable usage examples. The subcommand SHALL be registered in the root CLI router without aliases.

#### Scenario: Displaying categorized check-triggers help screen
- **WHEN** invoking `canon-clerk check-triggers --help`
- **THEN** option descriptions are printed grouped by concern alongside runnable usage examples

#### Scenario: Routing check-triggers without aliases
- **WHEN** invoking `canon-clerk check-triggers` with valid options
- **THEN** execution dispatches to the check-triggers command handler with no alias routing
