# Tasks

## 1. Specification & Core API
- [x] 1.1 Rename `lintWorkspace` to `lintCanons(options?: LintCanonsOptions)` and rename `LintWorkspaceOptions` to `LintCanonsOptions` in `packages/core/src/linter.ts`.
- [x] 1.2 Implement `cwd` (defaulting to `process.cwd()`), `targets` (accepting string or array, defaulting to `['.']`), relative resolution, canonical POSIX normalization, and lexicographical target sorting in `lintCanons`.
- [x] 1.3 Add `globs?: string | readonly string[]` and `glob?: string | readonly string[]` to `LintCanonsOptions`, defaulting to `['**/.canons/**/*.md']`.
- [x] 1.4 Update `discoverCanonPaths` in `packages/core/src/linter.ts` to evaluate candidates against the configured glob pattern(s) using union matching.
- [x] 1.5 Update exports in `packages/core/src/index.ts`.

## 2. Testing & Verification
- [x] 2.1 Update existing unit tests in `packages/core/src/linter.test.ts` to call `lintCanons(options)`.
- [x] 2.2 Add unit tests in `packages/core/src/linter.test.ts` for zero arguments, multiple explicit file targets (verifying sorted order), custom single globs, multiple globs (union), directory-scoped custom globs, and resolved target normalization (`././foo` vs `./bar`).
- [x] 2.3 Verify full test suite passes with `npm run check`.
