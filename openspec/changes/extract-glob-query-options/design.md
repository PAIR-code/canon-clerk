# Design

## Context

Canon Clerk needs consistent filesystem traversal and filtering across both canon rules and codebase target files. In `packages/core/src/linter.ts`, discovery logic, `.gitignore` filtering (via the `ignore` package), and hardcoded noise directories were coupled directly to the canon linter. Furthermore, options for discovery were expressed as flat, ad-hoc properties on `LintCanonsOptions` (`targets`, `globs`, `glob`, `ignores`, `defaultIgnores`), and path resolution fell back to ambient `process.cwd()`.

In headless environments (such as GitHub Actions where `$GITHUB_WORKSPACE` is the project root) or long-running daemon processes (such as LSP servers managing multi-root workspaces), ambient `process.cwd()` is fragile and incorrect. Semantically, the object of interest is the **workspace root**.

Issue #157 establishes `GlobQueryOptions` as the canonical abstraction for query domains, paving the way for Stage 0 primitives (`checkFileInCanonScope`, `matchesTriggers`) and the streaming engine (`queryCanons`), while cleanly decoupling `@canon-clerk/core` from ambient process state.

## Goals / Non-Goals

**Goals:**
- Extract noise directory defaults, glob normalization, and `.gitignore` matching into `packages/core/src/glob-query.ts`.
- Require `workspaceRoot: string` on `LintCanonsOptions`, eliminating all `process.cwd()` calls in `@canon-clerk/core`.
- Provide pure, deterministic predicates: `isPathIgnored`, `isPathMatch`, and `getMatchedGlobs`.
- Clean `LintCanonsOptions` by separating where to search (`targetGlobs`) from what constitutes a canon (`canons?: QueryDomainInput`).
- Expunge legacy flat options (`targets`, `globs`, `glob`, `ignores`, `defaultIgnores`) completely.
- Migrate CLI callers (`workspaceRoot: process.cwd()`) and core test suites cleanly.

**Non-Goals:**
- Backward-compatibility shims, deprecated property fallbacks, or transitional adapters (pre-alpha expunge directive).
- Implementation of Stage 0 `queryCanons` or `canon-clerk match` (covered in separate issues #155, #156, #158).

## Decisions

### 1. Unified `QueryDomainInput` and `GlobQueryOptions`
- **Decision:** Allow query domains to be specified as a single string (shorthand glob), string array (shorthand globs), or full `GlobQueryOptions` object with `globs`, `ignores`, and `defaultIgnores`.
- **Rationale:** Minimizes ceremony for common invocations while preserving full control over noise ignores and custom ignore rules.
- **Alternatives Considered:** Require full object syntax everywhere (too verbose for simple globs or paths); keep flat options on the outer function (creates ambiguity when orchestrating multiple domains).

### 2. Concrete `NormalizedGlobQueryOptions`
- **Decision:** `normalizeGlobQueryOptions` returns a frozen-compatible object containing readonly arrays `globs` and `ignores`. Active ignores automatically prepend `DEFAULT_IGNORES` unless `defaultIgnores === false`.
- **Rationale:** Decouples normalization from execution, ensuring traversal and matching loops operate on resolved, non-null pattern lists without redundant normalization checks.
- **Alternatives Considered:** Lazy resolution during traversal (higher per-path overhead and risk of inconsistent ignore rules).

### 3. Separation of `targetGlobs` and `canons` in `LintCanonsOptions`
- **Decision:** `LintCanonsOptions` separates traversal boundaries (`targetGlobs`) from canon pattern definition (`canons`).
- **Rationale:** `targetGlobs` answers *where* in the filesystem to look (directories or specific files), while `canons` answers *what* file patterns constitute canons and which directories to prune during recursion.
- **Alternatives Considered:** Merging targets and canons into a single glob list (breaks directory-scoped traversal and explicit file overrides).

### 4. Required `workspaceRoot` and Ban on `process.cwd()` in `core`
- **Decision:** `workspaceRoot: string` is a required parameter on `LintCanonsOptions`. No method or helper inside `@canon-clerk/core` shall invoke `process.cwd()`.
- **Rationale:** Decouples core business logic from OS process ambient state. The boundary adapters (`@canon-clerk/cli`, `@canon-clerk/action`, `@canon-clerk/lsp`) supply their environment-specific root (`process.cwd()`, `GITHUB_WORKSPACE`, `rootUri`), while unit tests pass explicit fixture paths without mutating process state.
- **Alternatives Considered:** Defaulting `workspaceRoot` to `process.cwd()` inside core (retains ambient coupling and breaks in multi-root daemon environments).

### 5. Pure Traversal and Pruning Semantics
- **Decision:** Directory subtrees matching `isPathIgnored(dir, options, workspaceRoot)` are pruned depth-first during discovery. Files matching `isPathMatch(file, options, workspaceRoot)` are yielded.
- **Rationale:** Avoids unnecessary I/O into large directories like `node_modules` and `.git` while ensuring only legitimate canon files reach the linter.

## Risks / Trade-offs

- **Risk:** Making `workspaceRoot` required breaks existing callers expecting zero-argument `lintCanons()`.
  - *Mitigation:* Boundary adapters (such as CLI `lint` command) already have access to `process.cwd()` and pass it explicitly. In pre-alpha, explicit domain modeling takes precedence over zero-argument shims.
- **Risk:** Expunging legacy properties from `LintCanonsOptions` will break any callers using `targets` or `globs`.
  - *Mitigation:* All internal consumers in `@canon-clerk/cli` and `packages/core/src/linter.test.ts` will be migrated in the same change.
