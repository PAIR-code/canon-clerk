# Tasks

## 1. Schema Definitions & AST Types

- [x] 1.1 Codify TypeScript types and interfaces for `CanonFrontmatter`, `CanonMetadata`, `CanonSections`, and `Canon` domain entity in `packages/schema/src/types.ts` and verify types compile with `npm run typecheck -w packages/schema`
- [x] 1.2 Add `yaml` dependency to `packages/schema/package.json` and verify monorepo build and lockfile integrity with `npm run lint:lockfile`

## 2. Deterministic Metadata Derivation & Parser

- [x] 2.1 Implement frontmatter extraction and validation in `packages/schema/src/frontmatter.ts` and verify unit tests pass with `npx vitest run packages/schema/src/frontmatter.test.ts`
- [x] 2.2 Implement Section 4.2 metadata derivation rules (`id`, `title`, `triggers`, `inspect`, `tags`, `references`) in `packages/schema/src/derive.ts` and verify unit tests pass with `npx vitest run packages/schema/src/derive.test.ts`
- [x] 2.3 Implement cognitive directive and markdown section AST parser (`Exception`, `Rationale`, `Remediation`) in `packages/schema/src/sections.ts` and verify unit tests pass with `npx vitest run packages/schema/src/sections.test.ts`
- [x] 2.4 Unify pure `parseCanon(content, options)` entrypoint in `packages/schema/src/index.ts` and verify end-to-end normalization tests pass with `npx vitest run packages/schema`

## 3. Core Alignment & Integration Verification

- [x] 3.1 Update `@canon-clerk/core` to consume the updated `Canon` schema and verify `npx vitest run packages/core` passes
- [x] 3.2 Run full workspace validation (`npm run check`) to ensure type safety, spec validation, pure schema conformance, and clean builds
