# Tasks

## 1. Streaming Workspace Linting Orchestration
- [x] 1.1 Implement internal canon path generator `discoverCanonPaths` in `packages/core/src/discovery.ts` with default directory pruning (`DEFAULT_IGNORES`), custom ignores (`ignores`) using `ignore` package with `.gitignore` semantics, override flag (`defaultIgnores`), and explicit target precedence.
- [x] 1.2 Define options (`LintWorkspaceOptions`) and result type (`FileLintResult`) in `packages/core/src/linter.ts`.
- [x] 1.3 Implement streaming `lintWorkspace(workspaceRoot, options?)` in `packages/core/src/linter.ts` as an async generator yielding `FileLintResult` on the fly.
- [x] 1.4 Handle missing explicit target files and unreadable files by yielding `FileLintResult` with error diagnostics.
- [x] 1.5 Export `lintWorkspace`, `LintWorkspaceOptions`, `FileLintResult`, `DEFAULT_IGNORES`, and `toPosixPath` from `packages/core/src/index.ts`.

## 2. Verification & Testing
- [x] 2.1 Update unit tests for `discoverCanonPaths` testing traversal, exclusion, and target filtering.
- [x] 2.2 Update unit tests for `lintWorkspace` verifying streaming iteration, clean runs, violation reporting, and error handling.
- [x] 2.3 Verify monorepo build and test suite pass cleanly (`npm test`).

