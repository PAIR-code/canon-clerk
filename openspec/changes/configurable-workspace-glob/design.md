# Design

## Context

`@canon-clerk/core` provides the primary filesystem discovery and lint orchestration engine for Canon Clerk. Currently exported as `lintWorkspace(workspaceRoot?, options?)`, it has two structural limitations:
1. `workspaceRoot` is a positional parameter while `targets` is inside an options bag. When every parameter has a sensible default (`cwd` defaults to `process.cwd()`, `targets` defaults to `['.']`, `globs` defaults to `['**/.canons/**/*.md']`), requiring a positional root forces awkward placeholders (e.g. `lintWorkspace(undefined, { targets: [...] })`).
2. Discovery hardcodes the requirement that canons must reside inside a directory named `.canons/`, causing scans of targeted non-`.canons` directories (e.g. `tmp/`) to find zero files.

SPEC.md §3.1 states:
> *"By default, canon discovery SHOULD search for all Markdown files matching the pattern: `**/.canons/**/*.md`. Implementations MAY allow or use a different pattern, for example to allow project-specific configuration."*

## Goals / Non-Goals

**Goals:**
- Rename `lintWorkspace` to `lintCanons(options?: LintCanonsOptions)`.
- Encapsulate all parameters in `options` with sensible defaults:
  - `targets?: string | readonly string[]` (defaults to `['.']`).
  - `cwd?: string` (defaults to `process.cwd()`).
  - `globs?: string | readonly string[]` (alias `glob`, defaults to `['**/.canons/**/*.md']`).
  - `defaultIgnores?: boolean` (defaults to `true`).
  - `rules?: readonly CanonLintRule[] | CanonLintRule[] | undefined`.
  - `ruleConfig?: RuleConfig | undefined`.
- Resolve, normalize, and sort target paths lexicographically relative to `cwd` as the initial orchestration step to guarantee deterministic yield order.
- Support multiple globs evaluated as a logical OR (union).
- Enable targeting arbitrary directories with custom globs (e.g. `lintCanons({ targets: 'tmp', globs: '**/*.md' })`).
- Support zero-argument invocation: `lintCanons()` lints the current workspace with default settings out of the box.
- Preserve ascending lexicographical sorting and streaming evaluation.

**Non-Goals:**
- Changing default discovery pattern (`**/.canons/**/*.md` remains the default).
- Adding CLI argument parsing in this change (handled in `cli-lint`).

## Decisions

### Decision 1: Single Defaultable Options Bag (`lintCanons(options?: LintCanonsOptions)`)
- **Choice:** Take a single optional `options?: LintCanonsOptions` object where all parameters (`targets`, `cwd`, `globs`, `defaultIgnores`, `rules`) have sensible defaults.
- **Rationale:** Eliminates awkward positional parameters and dummy placeholders. Calling `lintCanons()` with zero arguments evaluates the current working directory using default canons discovery and standard ignores. Downstream callers like `@canon-clerk/cli` can directly forward parsed CLI flags into `lintCanons({ targets, globs, cwd })`.

### Decision 2: Resolved Target Normalization & Sorting
- **Choice:** Resolve each target relative to `cwd`, normalize it to a clean relative POSIX path (eliminating redundant `./`, `//`, trailing slashes, and dot segments), and sort the resulting paths lexicographically before traversal.
- **Rationale:** Prevents syntactic path anomalies (such as `././foo` vs `./bar`) from corrupting the yield order, guaranteeing strict ascending yield order regardless of input formatting.

### Decision 3: Multi-Glob Support (`globs?: string | readonly string[]`)
- **Choice:** Support both single string and array forms for `globs` (or `glob`), normalizing to an array evaluated as a logical OR.
- **Rationale:** Aligns with standard TypeScript API ergonomics and SPEC.md §3.1, enabling callers to specify multiple discovery patterns without multiple traversal passes.

### Decision 4: Path Matching via Standard Ignore/Wildmatch
- **Choice:** Match discovered candidate relative POSIX paths against the configured glob set using gitignore wildmatch semantics.
- **Rationale:** Leverages the existing `ignore` dependency in `@canon-clerk/core` with zero new runtime dependencies, ensuring consistent globbing semantics across platforms.

