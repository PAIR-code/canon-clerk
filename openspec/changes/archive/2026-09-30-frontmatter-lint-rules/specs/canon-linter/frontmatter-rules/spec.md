# Spec Delta: Frontmatter Schema & Metadata Lint Rules

## Purpose

Defines frontmatter schema verification, delimiter integrity, recognized key restrictions, property types, and naming convention lint rules for Canon documents.

## ADDED Requirements

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
