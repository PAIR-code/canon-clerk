# Tasks

## 1. Extract Glob Query Module
- [ ] 1.1 Implement `packages/core/src/glob-query.ts` with `DEFAULT_IGNORES`, `GlobQueryOptions`, `QueryDomainInput`, `NormalizedGlobQueryOptions`, and `normalizeGlobQueryOptions`.
- [ ] 1.2 Implement path evaluation predicates in `glob-query.ts`: `isPathIgnored`, `isPathMatch`, and `getMatchedGlobs` requiring `workspaceRoot: string` without calling `process.cwd()`.
- [ ] 1.3 Export `glob-query.ts` from `packages/core/src/index.ts`.
- [ ] 1.4 Add comprehensive unit tests in `packages/core/src/glob-query.test.ts` covering normalization, noise defaults, custom ignores, and matching predicates.

## 2. Refactor Linter Options and Traversal
- [ ] 2.1 Refactor `LintCanonsOptions` in `packages/core/src/linter.ts` to require `workspaceRoot: string`, replace legacy flat options (`targets`, `globs`, `glob`, `ignores`, `defaultIgnores`) with `targetPaths?: string | readonly string[]` and `canonQuery?: QueryDomainInput`, and eliminate all `process.cwd()` calls.
- [ ] 2.2 Update `discoverCanonPaths()` in `packages/core/src/linter.ts` to normalize and prune redundant descendant target paths, sort disjoint targets, and traverse entries in sorted tree order anchored to `workspaceRoot`.
- [ ] 2.3 Update CLI lint command in `packages/cli/src/commands/lint.ts` to pass `workspaceRoot: cwd ?? process.cwd()`, `targetPaths`, and `canonQuery`.
- [ ] 2.4 Update all tests in `packages/core/src/linter.test.ts` to pass `workspaceRoot` and conform to `targetPaths` and `canonQuery`.

## 3. Verification & Hygiene
- [ ] 3.1 Verify `@canon-clerk/core` source contains zero references to `process.cwd`.
- [ ] 3.2 Run unit test suites for `@canon-clerk/core` ensuring 100% pass rate.
- [ ] 3.3 Run typecheck across the monorepo to ensure zero type errors.
