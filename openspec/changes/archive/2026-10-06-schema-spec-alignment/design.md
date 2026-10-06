# Design

## Context

Issue #196 decoupled `SPEC.md` from runner-specific execution phase terminology ("Phase 1 Check", "Phase 3 Audit") and introduced three key normative constructs:
1. `exists:` state preconditions (SPEC.md §4.1 & §4.2.6) determining whether a canon should be evaluated based on the state of the target file tree.
2. `inspect:` optional rider syntax `?` (SPEC.md §4.1 & §4.2.4) distinguishing mandatory intake exhibits (`diff`) from aspirational ones (`pr_title?`, `pr_body?`).
3. Scope containment and subordination (SPEC.md §3.3 & §80) guaranteeing monorepo package isolation by forbidding directory traversal outside `<scope>/`.

The current `@canon-clerk/schema` package parses and derives legacy frontmatter without `exists`, treats all `inspect` tokens as bare strings without optionality, lacks scope containment AST validation, and contains runner-phase comments in JSDoc.

Per canon `schema-must-be-pure-and-free-of-io`, `@canon-clerk/schema` must remain 100% pure in-memory, without importing Node.js I/O modules (`node:fs`, `node:path`) and without external runtime dependencies beyond `yaml`.

## Goals / Non-Goals

**Goals:**
- Align AST types (`Canon`, `CanonMetadata`, `CanonFrontmatter`, `InspectPlane`) with SPEC.md.
- Implement pure metadata derivation for `exists` (with scalar coercion and scoped prefixing) and `inspect` (with optionality parsing).
- Enforce scope containment during scoped derivation and in pure AST lint rules.
- Add pure lint rules for `scope-containment` and `exists-patterns`, and update `no-unrecognized-keys` and `valid-frontmatter-types`.
- Maintain 100% compliance with existing repository canons and preserve zero runtime I/O in `@canon-clerk/schema`.

**Non-Goals:**
- Evaluating file existence on disk (filesystem evaluation of `exists:` belongs to `@canon-clerk/core` in the Caseload DAG intake/discovery stages).
- Assembling or pruning diff exhibits (intake filtering belongs to runner nodes in `@canon-clerk/core`).
- Backward-compatibility aliases for removed runner phase terminology.

## Decisions

### 1. `InspectPlane` Structured Token Modeling
- **Decision:** Model `CanonMetadata.inspect` as `readonly InspectPlane[]`, where `InspectPlane = { readonly token: InspectToken; readonly optional: boolean }`.
- **Rationale:** Strongly typing the optional rider directly on the domain entity ensures downstream Caseload intake and discovery nodes can evaluate exhibit satisfaction without repeatedly parsing trailing `?` characters from string tokens.
- **Alternatives Considered:**
  - *Keep string tokens with trailing `?` (e.g. `string[]`):* Rejected because downstream consumers would have to repeatedly parse or strip `?` to identify the underlying exhibit name.
  - *Separate arrays (`requiredInspect: InspectToken[]`, `optionalInspect: InspectToken[]`):* Rejected because declaration order in frontmatter provides useful prioritization hints and SPEC.md treats `inspect` as a single ordered collection.

### 2. Pure Path Normalization & Scope Containment Without `node:path`
- **Decision:** Implement pure POSIX path parsing and normalization routines internally within `@canon-clerk/schema`.
- **Rationale:** In accordance with project canon `schema-must-be-pure-and-free-of-io`, `@canon-clerk/schema` cannot import `node:path`. Pure string routines (`replace(/\\/g, '/')`, relative path segment inspection, and `../` escape detection) allow schema parsing and linting in edge, browser, and in-memory environments.
- **Alternatives Considered:**
  - *Importing `node:path`:* Rejected; violates core architectural canon `schema-must-be-pure-and-free-of-io`.
  - *Importing third-party path libraries (e.g. `pathe`):* Rejected; avoids introducing new runtime dependencies.

### 3. Exposing `scope` on `RuleContext`
- **Decision:** Add a lazy `get scope(): string | undefined` getter to `RuleContext` using `deriveScope(this.filePath)`.
- **Rationale:** Lint rules such as `scope-containment` need to know whether the file under inspection belongs to a scoped package (`<scope>/.canons/`) or repository root (`.canons/`). Providing this via `RuleContext` keeps rule implementations clean and decoupled from file path string manipulation.
- **Alternatives Considered:**
  - *Passing scope explicitly in every rule invocation:* Rejected because `RuleContext` is the standard encapsulation container for all rule inputs.

## Risks / Trade-offs

- **Risk:** Type change of `CanonMetadata.inspect` from `InspectToken[]` to `readonly InspectPlane[]` could break downstream consumers.
  - *Mitigation:* Audited entire monorepo; `canon.inspect` was only referenced in `@canon-clerk/schema` unit tests. `@canon-clerk/core` and presentation packages do not currently inspect this property.
- **Risk:** Scoped pattern prefixing could erroneously flag valid relative paths inside `<scope>/`.
  - *Mitigation:* Normalized path logic verifies that relative paths without `../` escapes stay within scope, and unit tests cover nested directory structures.
