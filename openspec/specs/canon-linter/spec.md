# canon-linter Specification

## Purpose

Defines the static canon linting engine, immutable evaluation context, rule interfaces, severity configuration resolution, and pure execution runner.

## Requirements

### Requirement: Pure Rule Evaluation Context
The static linting engine SHALL provide an immutable rule evaluation context (`RuleContext`) capturing raw content, optional file path, file name, file stem, lexical tokens, and optional parsed frontmatter. Token collections exposed to rules MUST be defensively frozen to prevent cross-rule mutations. Tokenization and frontmatter parsing MUST be evaluated lazily on first property access and memoized.

#### Scenario: Freezing token collections against mutation
- **WHEN** a rule attempts to mutate the token collection provided on the rule context
- **THEN** the collection is frozen and rejects modification

#### Scenario: Zero YAML parsing overhead for token-only inspection
- **WHEN** a rule inspects only raw content or token streams without accessing frontmatter properties
- **THEN** frontmatter CST and YAML document parsing is not performed

#### Scenario: Deriving file stem and name from file path
- **WHEN** a rule context is initialized with a relative path such as `.canons/pr-tests.md`
- **THEN** `fileName` resolves to `"pr-tests.md"` and `fileStem` resolves to `"pr-tests"`

### Requirement: Fault-Tolerant Rule Execution Pipeline
The system SHALL provide a pure linting entrypoint (`lintCanon`) that executes configured lint rules against a canon document without throwing unhandled exceptions. If syntax is malformed or a rule throws an unhandled error during evaluation, the runner SHALL catch the error gracefully and emit structured diagnostic records with source coordinates.

#### Scenario: Graceful recovery on malformed syntax
- **WHEN** linting a canon file with malformed frontmatter delimiters or invalid syntax
- **THEN** the runner completes execution without throwing unhandled exceptions and reports diagnostics

#### Scenario: Encapsulating rule evaluation errors
- **WHEN** a configured rule throws an exception during evaluation
- **THEN** the runner catches the error and surfaces it as a diagnostic record rather than crashing the execution pipeline

### Requirement: Rule Severity Configuration and Overrides
The linting engine SHALL support configurable rule severities (`off`, `warning`, `error`). When a rule's severity is configured as `off`, the runner SHALL omit that rule from evaluation. When a rule has an explicit severity override, the runner SHALL emit diagnostics for that rule using the configured severity level.

#### Scenario: Disabling a rule via configuration
- **WHEN** a rule is configured with severity `'off'` in the lint options
- **THEN** the rule is not evaluated and emits zero diagnostics

#### Scenario: Overriding default rule severity
- **WHEN** a rule with default severity `'warning'` is overridden to `'error'` in the lint options
- **THEN** emitted diagnostics for that rule carry `'error'` severity

### Requirement: Deterministic Diagnostic Sorting
The runner SHALL sort all emitted diagnostics deterministically before returning them. Diagnostics MUST be sorted in ascending order by 1-indexed line number, then by 1-indexed column number, and finally by machine-readable rule identifier code.

#### Scenario: Sorting diagnostics by position and code
- **WHEN** multiple rules emit diagnostics spanning different lines, columns, and codes
- **THEN** the returned diagnostics array is sorted by line number, then column number, then rule identifier code

### Requirement: Pure In-Memory Linting Execution
The static linting engine and all rule evaluation logic within `@canon-clerk/schema` SHALL be pure and free of I/O, operating solely on in-memory strings and tokens with zero Node.js filesystem or network dependencies.

#### Scenario: In-memory linting execution
- **WHEN** the linting engine executes rules on canon source text
- **THEN** no filesystem reads, writes, or network calls are performed

### Requirement: YAML Frontmatter Syntax and Delimiter Verification
The static linter SHALL provide a rule (`valid-yaml-frontmatter`, default severity `error`) that verifies canon frontmatter blocks open and close with `---` delimiters and contain valid YAML mapping syntax. Syntax errors MUST report 1-indexed source line and column numbers. Canons without frontmatter MUST produce zero diagnostics.

#### Scenario: Valid frontmatter with proper delimiters
- **WHEN** linting a canon with properly delimited valid YAML frontmatter
- **THEN** `valid-yaml-frontmatter` produces zero diagnostics

#### Scenario: Missing terminating delimiter
- **WHEN** linting a canon starting with `---` that lacks a terminating `---` delimiter
- **THEN** `valid-yaml-frontmatter` reports an error diagnostic at the unclosed block

#### Scenario: Malformed YAML syntax inside frontmatter
- **WHEN** linting a canon whose frontmatter contains invalid YAML syntax
- **THEN** `valid-yaml-frontmatter` reports error diagnostics with 1-indexed line and column coordinates

#### Scenario: Canon omitting frontmatter entirely
- **WHEN** linting a canon without frontmatter
- **THEN** `valid-yaml-frontmatter` produces zero diagnostics

### Requirement: Frontmatter Schema Key Restriction
The static linter SHALL provide a rule (`no-unrecognized-keys`, default severity `warning`) that flags any frontmatter keys outside the SPEC.md Section 4.1 schema (`id`, `title`, `triggers`, `inspect`, `tags`, `references`). Diagnostics MUST report the exact 1-indexed line and column coordinates of unrecognized keys using YAML CST node locations.

#### Scenario: Canon with recognized frontmatter keys
- **WHEN** linting frontmatter containing only standard keys (`id`, `title`, `triggers`, `inspect`, `tags`, `references`)
- **THEN** `no-unrecognized-keys` produces zero diagnostics

#### Scenario: Frontmatter containing unknown properties
- **WHEN** linting frontmatter containing an unrecognized key such as `author:` or `deprecated:`
- **THEN** `no-unrecognized-keys` reports a warning diagnostic targeting the 1-indexed position of that key

### Requirement: Frontmatter Property Type Validation
The static linter SHALL provide a rule (`valid-frontmatter-types`, default severity `error`) that verifies frontmatter properties conform to their defined schema types: `id` and `title` as strings, and `triggers`, `inspect`, `tags`, and `references` as arrays of strings.

#### Scenario: Conforming frontmatter types
- **WHEN** frontmatter fields match their expected types (`id: string`, `title: string`, array fields as string lists)
- **THEN** `valid-frontmatter-types` produces zero diagnostics

#### Scenario: Scalar supplied for array field
- **WHEN** a scalar number or object is supplied where an array of strings is required
- **THEN** `valid-frontmatter-types` reports an error diagnostic targeting the invalid property

#### Scenario: Array containing non-string items
- **WHEN** a list field like `triggers` or `tags` contains numbers or booleans
- **THEN** `valid-frontmatter-types` reports an error diagnostic targeting the invalid item

### Requirement: Canon ID and Filename Alignment
The static linter SHALL provide a rule (`id-matches-filename`, default severity `warning`) that verifies explicit frontmatter `id` values match the kebab-case file stem when `filePath` is present on the rule context.

#### Scenario: Explicit ID matches file stem
- **WHEN** frontmatter `id` matches `context.fileStem` (e.g. `id: prs-must-include-tests` for `prs-must-include-tests.md`)
- **THEN** `id-matches-filename` produces zero diagnostics

#### Scenario: Explicit ID diverges from file stem
- **WHEN** frontmatter `id` diverges from `context.fileStem`
- **THEN** `id-matches-filename` reports a warning diagnostic suggesting alignment

#### Scenario: File path omitted or frontmatter ID omitted
- **WHEN** either `filePath` is undefined or no explicit frontmatter `id` is specified
- **THEN** `id-matches-filename` produces zero diagnostics

### Requirement: Affirmative File Stem Naming
The static linter SHALL provide a rule (`no-negated-file-stems`, default severity `warning`) that flags canon file stems containing negative modal verbs such as `-must-not-`, encouraging affirmative actions or categorical prohibition.

#### Scenario: Affirmative canon file stem
- **WHEN** a canon file stem uses affirmative phrasing like `canons-must-omit-needless-rationale` or `third-party-apps-are-forbidden`
- **THEN** `no-negated-file-stems` produces zero diagnostics

#### Scenario: Negated modal in canon file stem
- **WHEN** a canon file stem contains `-must-not-` (e.g. `prs-must-not-skip-tests`)
- **THEN** `no-negated-file-stems` reports a warning diagnostic with remediation guidance

### Requirement: Affirmative Frontmatter ID Naming
The static linter SHALL provide a rule (`no-negated-ids`, default severity `warning`) that flags explicit frontmatter `id` values containing negative modal phrases like `must-not`.

#### Scenario: Affirmative frontmatter ID
- **WHEN** an explicit frontmatter `id` uses affirmative phrasing
- **THEN** `no-negated-ids` produces zero diagnostics

#### Scenario: Negated modal in frontmatter ID
- **WHEN** an explicit frontmatter `id` contains `must-not`
- **THEN** `no-negated-ids` reports a warning diagnostic with remediation guidance

### Requirement: Frontmatter Rule Catalog Registration
The static linter SHALL export all frontmatter lint rules individually and as part of a default rule catalog in `@canon-clerk/schema`.

#### Scenario: Accessing default frontmatter rules
- **WHEN** importing rules from `@canon-clerk/schema`
- **THEN** all frontmatter lint rules are accessible and executable via `lintCanon`
