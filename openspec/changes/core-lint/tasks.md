# Tasks

## 1. Core Discovery Engine
- [x] 1.1 Define discovery options (`DiscoverCanonsOptions`) in `packages/core/src/discovery.ts`.
- [x] 1.2 Implement `discoverCanonFiles(workspaceRoot, options?)` in `packages/core/src/discovery.ts` with default directory pruning (`DEFAULT_IGNORES`), custom ignores (`ignores`) with `.gitignore` semantics, override flag (`defaultIgnores`), and explicit target precedence.
- [x] 1.3 Add path and target filtering support for specific files and directories.

## 2. Workspace Lint Orchestration
- [x] 2.1 Define options (`LintWorkspaceOptions`) and result types (`FileLintResult`, `WorkspaceLintResult`) in `packages/core/src/linter.ts`.
- [x] 2.2 Implement `lintWorkspace(workspaceRoot, options?)` in `packages/core/src/linter.ts` reading files, running `lintCanon()`, and aggregating counts.
- [x] 2.3 Implement graceful I/O error handling reporting file-level error diagnostics on read failures.
- [x] 2.4 Export discovery and linting functions and types from `packages/core/src/index.ts`.

## 3. Verification & Testing
- [x] 3.1 Create test fixtures for clean canons, rule violations, and nested package directories.
- [x] 3.2 Add unit tests for `discoverCanonFiles` testing traversal, exclusion, and target filtering.
- [x] 3.3 Add unit tests for `lintWorkspace` verifying clean runs, violation aggregation, and error handling.
- [x] 3.4 Verify monorepo build and test suite pass cleanly (`npm test`).
