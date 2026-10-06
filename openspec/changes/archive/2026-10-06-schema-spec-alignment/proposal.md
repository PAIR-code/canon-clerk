# Proposal

## Why

[SPEC.md](SPEC.md) (formalized in #196 / PR #197) decouples the canon document format from execution runner terminology and introduces several critical grammar and derivation updates:
1. `exists:` state preconditions (SPEC.md §4.1 & §4.2.6) specifying files that must exist in the target file tree for evaluation to proceed.
2. `inspect:` optional rider syntax `?` (SPEC.md §4.1 & §4.2.4) distinguishing required context exhibits from aspirational ones.
3. Scope containment and subordination (SPEC.md §3.3 & §80) requiring all paths in `triggers:`, `exists:`, and `references:` to resolve strictly within `<scope>/`, rejecting `../` directory traversals.
4. Runner decoupling, purging legacy runner-phase comments ("Phase 1 Check", "Phase 3 Audit") from canon definitions.

Currently, `@canon-clerk/schema` does not parse or derive `exists:`, does not support `?` riders on `inspect` tokens, lacks scope containment AST linting, and retains legacy runner comments. Aligning the AST, frontmatter derivation, and static linters with SPEC.md establishes a conforming document model prior to constructing the Caseload DAG state container.

## What Changes

1. **AST & Frontmatter Types:**
   - Define `InspectToken` and `InspectPlane` (`{ token: InspectToken, optional: boolean }`) in `@canon-clerk/schema`.
   - Update `CanonMetadata` with `readonly exists: readonly string[];` and `readonly inspect: readonly InspectPlane[];`.
   - Define typed `CanonFrontmatter` supporting optional `exists` and `inspect` tokens with trailing `?`.
   - Purge runner-phase terminology from JSDoc docstrings.

2. **Metadata Derivation & Scope Containment:**
   - Derive `exists` with scalar string coercion, defaulting to `[]`.
   - Update `inspect` default to `['diff', 'pr_title?', 'pr_body?']`, parsing `?` riders into `InspectPlane`.
   - Automatically scope patterns for scoped canons (`<scope>/.canons/`) and reject `../` scope escapes during derivation.

3. **Schema Parser Integration:**
   - Wire derived `exists` and `inspect` into `parseCanon()`, preserving pure, in-memory evaluation and 0-byte/nominal fallbacks.

4. **AST Lint Rules:**
   - Update `no-unrecognized-keys` to recognize `exists`.
   - Update `valid-frontmatter-types` to validate `exists` (string or array of strings) and `inspect` tokens with optional trailing `?`.
   - Add `scope-containment` rule rejecting path traversal outside `<scope>/`.
   - Add `exists-patterns` rule verifying syntactic validity of `exists` globs.
   - Add `scope` getter to `RuleContext`.

## Capabilities

### Modified Capabilities
- `schema`: Parses `exists` preconditions, resolves `inspect` optional riders (`?`), enforces scope containment, and models `InspectPlane`.
- `canon-linter/frontmatter-rules`: Adds `scope-containment` and `exists-patterns` rules, and updates `valid-frontmatter-types` and `no-unrecognized-keys`.

## Impact
- **Affected Packages:** `@canon-clerk/schema` (primary), `@canon-clerk/core` (type consumers).
- **Dependencies:** Maintains zero external runtime dependencies beyond `yaml`.
- **Breaking Changes:** Internal AST type `CanonMetadata.inspect` transitions from `InspectToken[]` to `readonly InspectPlane[]`. Repository canons in `.canons/` remain 100% compliant.
