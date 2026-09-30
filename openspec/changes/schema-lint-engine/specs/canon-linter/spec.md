# Spec Delta

## Purpose

Defines the static canon linting engine, immutable evaluation context, rule interfaces, severity configuration resolution, and pure execution runner.

## ADDED Requirements

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
