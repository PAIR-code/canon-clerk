# Proposal: Frontmatter Schema & Metadata Lint Rules

## Why
Building on the pure static linting engine introduced in #116, canon authors need automated, deterministic validation of YAML frontmatter blocks, metadata schema conformity, and naming conventions prior to LLM evaluation cascades. Without static validation, syntactic defects (unclosed delimiters, malformed YAML), schema violations (unrecognized keys, type mismatches), and naming anti-patterns (divergent IDs, negated modal verbs) pass unchecked into downstream processing.

## What Changes
Implement pure frontmatter and metadata lint rules in `@canon-clerk/schema` adhering to `schema-must-be-pure-and-free-of-io.md`:
- `valid-yaml-frontmatter`: Validates opening/closing `---` delimiters and YAML syntax, reporting 1-indexed coordinates.
- `no-unrecognized-keys`: Flags frontmatter keys outside SPEC.md Section 4.1 schema with CST key location tracking.
- `valid-frontmatter-types`: Validates key value types against schema (`id: string`, `title: string`, `triggers: string[]`, `inspect: string[]`, `tags: string[]`, `references: string[]`).
- `id-matches-filename`: Flags explicit frontmatter `id` values that diverge from kebab-case file stem when `filePath` is provided.
- `no-negated-file-stems`: Flags file stems containing negative modal verbs (such as `-must-not-`), encouraging affirmative actions (`must-omit`) or categorical prohibition (`...-are-forbidden`).
- `no-negated-ids`: Flags explicit frontmatter `id` values containing negative modal phrases (`must-not`).
- Register all frontmatter rules in a default rules export catalog and expose them from `@canon-clerk/schema`.

## Capabilities
### New Capabilities
- None (extends existing `canon-linter` capability).

### Modified Capabilities
- `canon-linter`: Added requirements and test scenarios for pure frontmatter syntax verification, schema key restriction, property type validation, filename-id alignment, affirmative naming, and rule catalog registration.

## Impact
- `@canon-clerk/schema`: New rule modules under `src/rules/frontmatter/`, rule registration, and unit test suites.
- Consumers of `@canon-clerk/schema`: Access to standard frontmatter lint rules and default catalog for static canon verification.
