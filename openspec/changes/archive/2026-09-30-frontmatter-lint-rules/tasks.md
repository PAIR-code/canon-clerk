# Tasks: Frontmatter Schema & Metadata Lint Rules

## 1. Rule Implementations
- [x] 1.1 Implement `valid-yaml-frontmatter` rule verifying delimiters and reporting YAML parse errors with 1-indexed coordinates.
- [x] 1.2 Implement `no-unrecognized-keys` rule with CST key location tracking against SPEC.md Section 4.1 schema.
- [x] 1.3 Implement `valid-frontmatter-types` rule validating scalar string and array types for recognized keys.
- [x] 1.4 Implement `id-matches-filename` rule checking explicit frontmatter `id` against `context.fileStem`.
- [x] 1.5 Implement `no-negated-file-stems` rule flagging negative modal verbs in canon file stems.
- [x] 1.6 Implement `no-negated-ids` rule flagging negative modal phrases in frontmatter IDs.

## 2. Rule Registration & Exports
- [x] 2.1 Create rule catalog exports (`FRONTMATTER_RULES`, `DEFAULT_RULES`) in `packages/schema/src/rules/`.
- [x] 2.2 Re-export rules and catalogs from `packages/schema/src/index.ts`.

## 3. Test Suites & Verification
- [x] 3.1 Author comprehensive unit tests in `packages/schema/src/rules/frontmatter/` covering positive and negative cases for all 6 rules.
- [x] 3.2 Verify integration with `lintCanon` runner, line/col coordinate accuracy, and severity overrides.
- [x] 3.3 Run `npm run check` across the monorepo to guarantee clean typecheck, build, and zero I/O purity.
