# Proposal

## Why

Currently, `@canon-clerk/core` exports `lintWorkspace(workspaceRoot?, options?)`. This design has several limitations:
1. **Awkward Positional Root:** `workspaceRoot` is a positional parameter while `targets` is inside an options bag. When every configuration option has a sensible default (`cwd` defaults to `process.cwd()`, `targets` defaults to `['.']`, `globs` defaults to `['**/.canons/**/*.md']`), requiring or expecting a positional root forces awkward placeholders (e.g. `lintWorkspace(undefined, { targets: [...] })`).
2. **Narrow Scope Connotation:** The name `lintWorkspace()` suggests operations are constrained to an entire workspace root, rather than serving as a general canon discovery and linting engine for arbitrary files, directories, or globs on disk.
3. **Hardcoded `.canons` Path Filter:** Discovery hardcodes the requirement that canon files must reside inside a directory named `.canons/`. When targeting arbitrary directories (e.g. `tmp`, test fixtures, or custom documentation folders), discovery fails to find any canon files.

Following [SPEC.md §3.1](SPEC.md#L46-L51), canon discovery defaults to `**/.canons/**/*.md`, but implementations may allow alternative patterns. Renaming the function to `lintCanons(options?: LintCanonsOptions)`—where all parameters are cleanly encapsulated in an options object with sensible defaults—and supporting configurable discovery globs (`globs?: string | readonly string[]`) establishes domain symmetry with `@canon-clerk/schema`'s `lintCanon()` and delivers a clean, zero-argument-capable programmatic API.

## What Changes

- Rename `lintWorkspace` to `lintCanons` in `@canon-clerk/core`, taking a single optional options parameter: `lintCanons(options?: LintCanonsOptions)`.
- Rename `LintWorkspaceOptions` to `LintCanonsOptions`.
- Add `targets?: string | readonly string[] | undefined` to `LintCanonsOptions` (defaulting to `['.']`).
- Add `cwd?: string | undefined` to `LintCanonsOptions` (defaulting to `process.cwd()`).
- Add `globs?: string | readonly string[] | undefined` (with `glob` alias) to `LintCanonsOptions`, defaulting to `['**/.canons/**/*.md']`. Multiple globs evaluate as a logical OR (union).
- Resolve, normalize, and sort target paths lexicographically relative to `cwd` as the initial orchestration step, guaranteeing deterministic yield order across arbitrary combinations of file and directory targets regardless of input syntax.
- Match discovered files against configured globs rather than hardcoding a `.canons` directory check.
- When directory targets are specified alongside custom globs (e.g. `lintCanons({ targets: 'tmp', globs: '**/*.md' })`), discover and evaluate all matching markdown files within those directories.
- Maintain deterministic ascending lexicographical sorting and default directory pruning (`node_modules`, `dist`, `.bare`, `.git`, `.turbo`).

## Capabilities

### Modified Capabilities
- `canon-linter/workspace`: Renames streaming generator to `lintCanons(options?)`, supporting encapsulated options with defaults for `targets`, `cwd`, and configurable multi-glob pattern matching (`globs`).

## Impact

- `@canon-clerk/core`: Updates public exports (`lintCanons`, `LintCanonsOptions`) and traversal orchestration.
- Downstream consumers (such as `@canon-clerk/cli`'s `lint` command) can invoke `lintCanons({ targets, globs, cwd: process.cwd() })` or invoke `lintCanons()` with zero arguments for workspace defaults.
