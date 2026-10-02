# Proposal

## Why

Stage 0 of the Canon Clerk evaluation cascade determines whether a repository canon is activated by a given target file before incurring downstream screening or deep reasoning costs.

Stage 0 evaluation answers two distinct questions:
1. **Scope Containment:** *"Does this target file reside within this canon's monorepo package or repository scope boundary?"*
2. **Trigger Matching:** *"Given that the target file is in scope, does its scope-relative path match any of the canon's path triggers?"*

Currently, Canon Clerk lacks pure in-memory primitives for these operations. Without them, Stage 0 cannot screen modified files against canons without incurring filesystem I/O or full frontmatter parsing. Furthermore, evaluating scope and triggers must be host- and environment-agnostic, operating on explicit `workspaceRoot` anchors without ambient calls to `process.cwd()` in accordance with the Explicit Dependencies Principle (`library-dependencies-must-be-explicit`).

Decomposing Stage 0 into two pure, zero-I/O primitives (`checkFileInCanonScope` and `matchesTriggers`) provides the foundational building blocks for high-throughput screening and streaming queries (`queryCanons`).

## What Changes

- Implement `checkFileInCanonScope(targetPath, canonPath, options)` in `@canon-clerk/core`:
  - Evaluates whether a target file resides within a canon's hierarchical scope at zero I/O cost.
  - Enforces workspace containment for both target and canon paths relative to `workspaceRoot`.
  - Rejects ignored canons via `isPathIgnored` and validates canon discovery patterns via `isPathMatch`.
  - Performs an ancestor directory walk from the target file to the workspace root using stripped local canon globs to identify the owning scope boundary.
  - Returns `scopePath`, `scopeRelativePath`, and `targetScopeRelativePath` on match, or descriptive failure reasons (`CANON_OUTSIDE_WORKSPACE`, `TARGET_OUTSIDE_WORKSPACE`, `CANON_IGNORED`, `CANON_GLOB_MISMATCH`, `OUT_OF_SCOPE`).
- Implement `matchesTriggers(targetScopeRelativePath, triggers?)` in `@canon-clerk/core`:
  - Evaluates whether a scope-relative target path satisfies a canon's declared path triggers.
  - Treats omitted, empty, or universal wildcard triggers (`'**/*'`, `'**'`) as active (`triggered: true`, `matchedTriggers: ['**/*']`).
  - Evaluates pattern lists with dotfile awareness (`dot: true`) and returns all matching trigger patterns.
- Export primitives and related types from `packages/core/src/index.ts`.
- Deliver comprehensive topical unit test suites in `packages/core/src/scope.test.ts` and `packages/core/src/triggers.test.ts`.

## Capabilities

### New Capabilities
- `path-filter/scope`: Evaluates whether a target file resides within a canon's monorepo package or repository scope boundary through pure hierarchical ancestor traversal anchored to an explicit workspace root.
- `path-filter/triggers`: Evaluates whether a scope-relative target file path matches a canon's declared trigger patterns using glob matching with dotfile awareness.

### Modified Capabilities
*(None)*

## Impact

- `@canon-clerk/core`: Adds `checkFileInCanonScope`, `matchesTriggers`, and associated types. Pure string manipulation with zero I/O and zero ambient `process.cwd()` dependencies.
- Downstream systems: Establishes primitives required by Issue #156 (`queryCanons` streaming engine) and subsequent CLI filtering commands (`canon-clerk match`).
