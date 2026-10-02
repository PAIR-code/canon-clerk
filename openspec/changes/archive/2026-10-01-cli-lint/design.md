# Design

## Context

Static canon evaluation (`@canon-clerk/schema`) and streaming workspace orchestration (`@canon-clerk/core`) provide pure evaluation and discovery services. To expose these capabilities to developers and CI pipelines, `@canon-clerk/cli` acts as an external presentation adapter.

Per repository architectural canons, presentation adapters must delegate all domain logic to the core package, isolate output data from operational diagnostics, support machine-readable JSON envelopes, and provide predictable exit codes.

## Goals / Non-Goals

**Goals:**
- Provide a robust CLI entrypoint with subcommand routing (`canon-clerk lint`).
- Implement presentation formatters: a human-oriented terminal reporter (`stylish`) with actionable remediation hints and a machine-readable JSON document envelope (`json`).
- Support flexible target specification via positional operands and standard input (`-`).
- Expose core traversal and exclusion controls (`-g, --glob`, `--ignore`, `--default-ignores`).
- Provide exit code enforcement with warning ratcheting (`--max-warnings`) and quiet filtering (`--quiet`).
- Adhere to the zero-external-dependency canon by utilizing standard runtime platform capabilities.

**Non-Goals:**
- Project-level configuration file discovery (e.g. `.canonclerkrc.json`), which is deferred to follow-on issues.
- Rule auto-fixing or interactive terminal prompts.
- Subcommands other than `lint` (e.g. `audit`, `sync`), though the routing architecture must cleanly accommodate them.

## Decisions

### Decision 1: Industry-Standard CLI Framework (`commander`)
- **Choice:** Adopt `commander` as the established industry-standard CLI framework for subcommand routing, option parsing, and help formatting, paired with Node 24 native `node:util.styleText` for terminal colors and formatting.
- **Rationale:** Grounded directly in `.canons/architecture/reinventing-standard-solutions-is-forbidden.md`, `commander` provides a battle-tested, zero-transitive-dependency (~40KB) solution for command hierarchies, grouped help screens, negative option flags (`--no-default-ignores`), environment variable fallbacks, and usage errors out of the box, avoiding the need to write bespoke routing and ASCII-formatting plumbing.
- **Alternatives considered:** Low-level `node:util.parseArgs` (rejected because it lacks subcommand dispatch, help formatting, and error formatting, requiring hundreds of lines of bespoke scaffolding).

### Decision 2: Subcommand Router Architecture
- **Choice:** Structure the CLI entrypoint around a command router mapping the first positional argument to registered command handlers, with root fallback displaying general usage.
- **Rationale:** Separates routing and global lifecycle concerns (signals, help, version) from command-specific presentation logic, allowing future commands (`audit`, `init`) to be added without modifying the entrypoint harness.
- **Alternatives considered:** Monolithic single-command CLI (rejected as it blocks multi-command expansion).

### Decision 3: Streaming Determinism Inherited from Core
- **Choice:** Stream terminal diagnostic outputs in real time as `FileLintResult` records are yielded by `@canon-clerk/core`'s `lintCanons()`. For JSON output (`--json`), collect yielded results in memory to assemble the final JSON array.
- **Rationale:** Because `@canon-clerk/core` guarantees deterministic ascending lexicographical yield order during per-directory depth-first descent, the CLI presentation layer does not need to buffer results for terminal display, satisfying both `multi-resource-operations-must-stream-results.md` and `cli-collection-outputs-must-be-stably-sorted.md` simultaneously.
- **Alternatives considered:** Buffering all terminal outputs before printing (rejected as unnecessary latency now that the domain generator streams in strictly sorted order).

### Decision 4: Actionable Canonical Array of File Results
- **Choice:** Emit a flat array of `FileLintResult[]` containing only files with diagnostics directly to `stdout` (yielding `[]` on a clean run).
- **Precedent & Inspiration:** Follows the established industry standard set by ESLint (`eslint -f json`), which emits a root array of per-file diagnostic records rather than a bespoke envelope object.
- **Rationale:** Aligns with standard ecosystem conventions while optimizing for downstream AI agents and Unix pipes (`jq`): by omitting clean files, payload size and token consumption remain minimal with maximum signal-to-noise ratio. An agent or script receiving the output immediately obtains actionable issues without filtering through hundreds of clean files or unwrapping synthetic envelopes.
- **Alternatives considered:** Top-level envelope object (rejected as unnecessary indirection and non-standard); unconditional emission of all clean files (rejected as excessively verbose noise for large workspaces).

### Decision 5: Standard Input for Document Content via POSIX Hyphen Operand
- **Choice:** Support `-` as a positional target operand to read raw Markdown document content from `stdin` (evaluated via `lintCanon()`), with an optional `--stdin-filename` option to provide a virtual relative path for filename-dependent frontmatter rules. Positional targets default to `.`, and when specified, can be literal files, directories, or globs. For passing collections of file paths, users compose with standard Unix `xargs` (e.g. `git diff --name-only | xargs canon-clerk lint`).
- **Rationale:** Strictly complies with POSIX.1-2017 Utility Syntax Guideline 10 and `cli-file-operands-must-accept-hyphen-for-stdin.md`. Treating `-` as document content aligns with developer expectations for linters (such as `eslint --stdin`), while avoiding implicit stdin hijacking without an explicit `-` operand.
- **Alternatives considered:** Implicitly reading `stdin` without operands (rejected as it hangs when no input is piped); parsing `stdin` as newline-delimited file paths (rejected as counter-intuitive for linters where standard input represents document content, and redundant with standard Unix `xargs`).

### Decision 6: Configurable Discovery Glob Pattern (`-g, --glob`)
- **Choice:** Support `-g, --glob <pattern>` (defaulting to `**/.canons/**/*.md`) to configure the discovery pattern used when traversing workspace roots or targeted directories.
- **Rationale:** Adheres to SPEC.md §3.1 and standard Unix conventions established by tools like `ripgrep` (`rg -g`), allowing developers to lint arbitrary folders (e.g. `canon-clerk lint tmp -g "**/*.md"`) or custom canon directory hierarchies without hardcoding `.canons/`.
- **Alternatives considered:** Auto-guessing whether a directory is a canon container vs project workspace (rejected as ambiguous and heuristic); hardcoding `**/.canons/**/*.md` with no override (rejected as overly rigid).

### Decision 7: Tiered Configuration Precedence
- **Choice:** Resolve options following standard hierarchy: explicit CLI flags override environment variables (`CANON_CLERK_*`), which override defaults.
- **Rationale:** Follows Twelve-Factor App principles and repository canons, allowing CI workflows to configure defaults via environment variables while permitting local overrides via flags.

## Risks / Trade-offs

- **[Risk: Large workspace memory usage during buffered sorting]** → **Mitigation:** Only `FileLintResult` lightweight summary metadata is stored in memory, not raw file contents.
- **[Risk: Stdin pipe hangs if no input provided]** → **Mitigation:** Only read `stdin` when `-` is explicitly supplied as a target operand.
