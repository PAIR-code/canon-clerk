# Proposal

## Why

All evaluation stages (Stage 0 Path Filtering, Stage 1 Screening, Stage 2 Deep Audit, and the CLI query suite) require a validated, normalized in-memory representation of a canon. Currently, the codebase contains only an unaligned placeholder interface in `@canon-clerk/schema`, with no standard API to convert raw canon markdown documents into canonical domain entities adhering to `SPEC.md`.

## What Changes

- Define comprehensive TypeScript types, frontmatter schemas, and AST interfaces for canons in `@canon-clerk/schema`.
- Implement pure, zero-I/O parsing and normalization functions to transform raw markdown and YAML frontmatter into normalized `Canon` domain entities.
- Implement Section 4.2 deterministic metadata derivation rules (`id`, `title`, `triggers`, `inspect`, `tags`, `references`).
- Parse cognitive directives (`Exception`, `Rationale`, `Remediation`) and the invariant statement into structured section ASTs.
- Enforce the `@canon-clerk/schema` pure architectural boundary (zero Node I/O or filesystem dependencies).

## Capabilities

### New Capabilities
- `core`: Core evaluation engine contracts, canon domain entity schemas, AST interfaces, and pure normalization per SPEC.md.

### Modified Capabilities

## Impact

- `@canon-clerk/schema`: Provides canonical types, AST interfaces, and pure normalization APIs.
- `@canon-clerk/core`: Updates consumers and type references to use the normalized `Canon` entity.
- Downstream packages (`@canon-clerk/cli`, `@canon-clerk/action`): Establishes the foundational entity contracts required for list queries and audit gates.
