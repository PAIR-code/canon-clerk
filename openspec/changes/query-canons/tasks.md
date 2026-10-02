# Tasks

## 1. Core Types & Discovery Contracts
- [x] 1.1 Define `FileMatch`, `TargetMatch`, `CanonMatch`, `QueryCanonsResult`, and `QueryCanonsOptions` in `packages/core/src/query.ts`.
- [x] 1.2 Export query interfaces and `queryCanons` function signature from `packages/core/src/index.ts`.
- [x] 1.3 Validate input parameters: enforce required `workspaceRoot` string and handle flexible `targetQuery` and `canonQuery` inputs.

## 2. Target Path Resolution & Bipartite Normalization
- [x] 2.1 Implement target query normalization using `normalizeGlobQueryOptions`.
- [x] 2.2 Support prospective target paths: resolve literal file paths directly against `workspaceRoot` without requiring filesystem existence.
- [x] 2.3 Support wildcard/directory target queries: traverse filesystem matching `isPathMatch` and pruning `isPathIgnored`.
- [x] 2.4 Lexically sort resolved targets to guarantee deterministic evaluation order.
- [x] 2.5 Enforce workspace boundary checks, rejecting target paths escaping `workspaceRoot` with `RangeError`.

## 3. Streaming Engine & Lazy Canon Evaluation
- [x] 3.1 Implement lexical depth-first directory traversal from `workspaceRoot`, pruning ignored subtrees.
- [x] 3.2 Filter candidate canon files matching `isPathMatch(file, canonQueryConfig, workspaceRoot)`.
- [x] 3.3 Apply early scope screening via `checkFileInCanonScope` to discard out-of-scope targets with zero I/O.
- [x] 3.4 Lazily read and parse candidate canon Markdown files via `parseCanon`, caching parsed results to guarantee single-pass loading per canon.
- [x] 3.5 Evaluate `matchesTriggers` against target scope-relative paths.
- [x] 3.6 Stream atomic `QueryCanonsResult` tuples immediately upon activation without buffering.

## 4. Test Suite, Verification & Living Spec Sync
- [x] 4.1 Author comprehensive unit test suite in `packages/core/src/query.test.ts` structured into topical sub-suites.
- [x] 4.2 Test prospective target resolution, scoped monorepo canons, root canons, and lazy parsing verification.
- [x] 4.3 Verify zero references to `process.cwd` in `@canon-clerk/core`.
- [x] 4.4 Run full verification pipeline (`npm run check`) ensuring 100% green status across all monorepo lanes.
- [x] 4.5 Promote delta specification to living specs via `openspec-sync-specs` or `openspec-archive-change`.
