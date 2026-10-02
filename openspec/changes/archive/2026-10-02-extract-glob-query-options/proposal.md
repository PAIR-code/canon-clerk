# Proposal

## Why

Across Canon Clerk, multiple subsystems discover and filter repository files:
1. **Linter (`lintCanons`):** Discovers canon Markdown files within target paths while ignoring noise directories (`node_modules`, `dist`, `.bare`, `.git`, `.turbo`) and custom user ignore patterns.
2. **Stage 0 Primitives (`checkFileInCanonScope`):** Verifies canon file path patterns and ensures ignored canons (e.g. archived rules) are excluded.
3. **Stage 0 Streaming Engine (`queryCanons`):** Discovers canons and target codebase files across symmetrical bipartite domains.
4. **CLI Subcommands (`canon-clerk match`, `canon-clerk lint`):** Accepts target paths/globs, canon definition patterns, and tiered ignore flags.

Currently, default noise ignores (`DEFAULT_IGNORES`), glob normalization, and `.gitignore` evaluation are private internal helpers locked inside `packages/core/src/linter.ts`. Furthermore, `LintCanonsOptions` exposes flat ad-hoc properties (`targets`, `globs`, `glob`, `ignores`, `defaultIgnores`) with competing ways to describe discovery and filtering.

Additionally, `packages/core` currently falls back to `process.cwd()`. In server processes, long-running LSP daemons, and GitHub Action runners where `$GITHUB_WORKSPACE` defines the repository root, relying on ambient process working directory state is an anti-pattern. Semantically, the object of interest is `workspaceRoot`, not the OS process's current working directory.

Extracting discovery into a cohesive `glob-query` module with `GlobQueryOptions` and `QueryDomainInput` contracts unifies discovery across Canon Clerk, establishes `workspaceRoot: string` as an explicit required anchor, and expunges legacy flat options and `process.cwd()` dependencies from `@canon-clerk/core`.

## What Changes

- Create `packages/core/src/glob-query.ts` exporting:
  - `DEFAULT_IGNORES`: Frozen array of standard repository noise directories (such as `node_modules`, `dist`, `.bare`, `.git`, `.turbo`).
  - `GlobQueryOptions`, `QueryDomainInput`, and `NormalizedGlobQueryOptions` interfaces and types.
  - `normalizeGlobQueryOptions(input?, defaultGlobs?)`: Normalizes flexible input and prepends `DEFAULT_IGNORES` unless explicitly disabled.
  - `isPathIgnored(path, options, workspaceRoot)`: Evaluates ignore patterns with `.gitignore` syntax requiring an explicit `workspaceRoot`.
  - `isPathMatch(path, options, workspaceRoot)`: Evaluates whether a path matches at least one glob pattern and zero active ignore patterns.
  - `getMatchedGlobs(path, options, workspaceRoot)`: Returns all matched glob patterns for a path.
- Refactor `packages/core/src/linter.ts`:
  - Expunge legacy flat properties (`targets`, `globs`, `glob`, `ignores`, `defaultIgnores`) from `LintCanonsOptions`.
  - Require `workspaceRoot: string` on `LintCanonsOptions` to anchor all relative path resolution explicitly, eliminating all calls to `process.cwd()` from `@canon-clerk/core`.
  - Introduce `targetPaths?: string | readonly string[] | undefined` for where discovery occurs (defaulting to `['.']`).
  - Introduce `canonQuery?: QueryDomainInput | undefined` for canon patterns and ignore rules (defaulting to `{ globs: ['**/.canons/**/*.md'], defaultIgnores: true }`).
  - Use `glob-query` utilities inside `discoverCanonPaths()`, normalizing and pruning redundant descendant targets to guarantee deterministic lexicographical order.
- Update `@canon-clerk/cli`'s `lint` command to pass `workspaceRoot: cwd ?? process.cwd()`, `targetPaths`, and `canonQuery`.
- Update `@canon-clerk/core` test suites to pass `workspaceRoot`, `targetPaths`, and `canonQuery`.
- Re-export `glob-query` types and utilities from `packages/core/src/index.ts`.

## Capabilities

### New Capabilities
- `glob-query`: Defines flexible filesystem discovery options, default noise ignores, `.gitignore` parsing, and path filtering predicates for bipartite query domains anchored to an explicit workspace root.

### Modified Capabilities
- `canon-linter/workspace`: Updates `lintCanons` discovery options to require `workspaceRoot: string`, use `targetPaths` and `canonQuery?: QueryDomainInput`, and expunge legacy flat options.

## Impact

- `@canon-clerk/core`: Adds new `glob-query` module, cleans `LintCanonsOptions` interface, and removes all `process.cwd()` references.
- `@canon-clerk/cli`: Migrates `lint` command invocation of `lintCanons` to provide `workspaceRoot: process.cwd()` and new query options.
- Test suites: Migrates `linter.test.ts` to new options and adds `glob-query.test.ts`.
