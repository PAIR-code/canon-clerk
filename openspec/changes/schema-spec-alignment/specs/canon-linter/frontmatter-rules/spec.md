# Spec Delta: canon-linter/frontmatter-rules

## MODIFIED Requirements

### Requirement: Frontmatter Schema Key Restriction
The static linter SHALL provide a rule (`no-unrecognized-keys`, default severity `warning`) that flags any frontmatter keys outside the SPEC.md Section 4.1 schema (`id`, `title`, `triggers`, `exists`, `inspect`, `tags`, `references`). Diagnostics MUST report the exact 1-indexed line and column coordinates of unrecognized keys using YAML CST node locations.

#### Scenario: Canon with recognized frontmatter keys
- **WHEN** linting frontmatter containing only standard keys (`id`, `title`, `triggers`, `exists`, `inspect`, `tags`, `references`)
- **THEN** `no-unrecognized-keys` produces zero diagnostics

#### Scenario: Frontmatter containing unknown properties
- **WHEN** linting frontmatter containing an unrecognized key such as `author:` or `deprecated:`
- **THEN** `no-unrecognized-keys` reports a warning diagnostic targeting the 1-indexed position of that key

#### Scenario: Canon with recognized exists key
- **WHEN** linting frontmatter containing `exists:` along with standard keys
- **THEN** `no-unrecognized-keys` produces zero diagnostics

### Requirement: Frontmatter Property Type Validation
The static linter SHALL provide a rule (`valid-frontmatter-types`, default severity `error`) that verifies frontmatter properties conform to their schema types: `id` and `title` as strings, `exists` as a string or array of strings, `triggers`, `tags`, and `references` as arrays of strings, and `inspect` as an array of recognized tokens with optional trailing `?`.

#### Scenario: Conforming frontmatter types
- **WHEN** frontmatter fields match their expected types (`id: string`, `title: string`, array fields as string lists)
- **THEN** `valid-frontmatter-types` produces zero diagnostics

#### Scenario: Scalar supplied for array field
- **WHEN** a scalar number or object is supplied where an array of strings is required
- **THEN** `valid-frontmatter-types` reports an error diagnostic targeting the invalid property

#### Scenario: Array containing non-string items
- **WHEN** a list field like `triggers` or `tags` contains numbers or booleans
- **THEN** `valid-frontmatter-types` reports an error diagnostic targeting the invalid item

#### Scenario: Valid exists property formats
- **WHEN** frontmatter specifies `exists` as a scalar string or an array of strings
- **THEN** `valid-frontmatter-types` produces zero diagnostics

#### Scenario: Valid inspect tokens with optional riders
- **WHEN** frontmatter specifies `inspect: ['diff', 'pr_title?', 'pr_body?']`
- **THEN** `valid-frontmatter-types` produces zero diagnostics

#### Scenario: Invalid inspect tokens with trailing question marks
- **WHEN** frontmatter specifies an unrecognized token like `unknown_token?`
- **THEN** `valid-frontmatter-types` reports an error diagnostic targeting that item

## ADDED Requirements

### Requirement: Scope Containment Lint Rule
The static linter SHALL provide a rule (`scope-containment`, default severity `error`) checking all declared patterns in `triggers:`, `exists:`, and `references:` within scoped canons. Any pattern attempting directory traversal outside `<scope>/` (e.g. `../`) MUST report an error diagnostic targeting the invalid CST item location.

#### Scenario: Scoped canon with contained patterns
- **WHEN** a scoped canon declares patterns strictly within its package directory (e.g. `src/**/*.ts`)
- **THEN** `scope-containment` produces zero diagnostics

#### Scenario: Scoped canon attempting directory escape
- **WHEN** a scoped canon declares a pattern containing `../` escaping its scope directory
- **THEN** `scope-containment` reports an error diagnostic with line and column coordinates

### Requirement: Exists Pattern Syntax Lint Rule
The static linter SHALL provide a rule (`exists-patterns`, default severity `error`) validating glob pattern syntax in `exists:`. Any malformed glob pattern containing unbalanced braces or unclosed brackets MUST report an error diagnostic targeting the invalid pattern location.

#### Scenario: Syntactically valid exists globs
- **WHEN** `exists` contains valid glob patterns such as `package.json` or `packages/*/{schema,core}.json`
- **THEN** `exists-patterns` produces zero diagnostics

#### Scenario: Malformed exists glob syntax
- **WHEN** `exists` contains an invalid glob pattern with unbalanced braces like `packages/{unclosed`
- **THEN** `exists-patterns` reports an error diagnostic targeting the invalid pattern
