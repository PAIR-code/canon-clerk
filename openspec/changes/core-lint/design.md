# Design

## Context

`@canon-clerk/schema` exports pure in-memory linting (`lintCanon`) and the default rule catalog (`DEFAULT_RULES`), but has zero filesystem access. `@canon-clerk/core` is the domain layer responsible for canon discovery, path filtering, and orchestration.

Per [`cli-must-delegate-domain-queries-to-core.md`](packages/cli/.canons/cli-must-delegate-domain-queries-to-core.md), presentation adapters such as the CLI must delegate all domain queries and discovery to `@canon-clerk/core`. Per [`core-must-be-presentation-agnostic.md`](packages/core/.canons/core-must-be-presentation-agnostic.md), `@canon-clerk/core` must not produce terminal formatting or import styling libraries.

## Goals / Non-Goals

**Goals:**
- Implement `discoverCanonFiles(workspaceRoot, options?)` to locate all `.canons/**/*.md` across monorepo packages and workspace root.
- Support target filtering by file path, subdirectory path, or globs.
- Implement `lintWorkspace(workspaceRoot, options?)` that reads files asynchronously, evaluates `lintCanon()`, and aggregates results.
- Define strongly typed `FileLintResult` and `WorkspaceLintResult` structures.
- Normalize all reported file paths to POSIX relative paths from `workspaceRoot`.
- Provide comprehensive unit tests using isolated test fixtures.

**Non-Goals:**
- Hierarchical `.canonlintrc.json` configuration cascading (deferred to follow-on issues).
- CLI command execution, options parsing, and terminal formatting (handled in Issue #140).
- Exit code process termination (handled in Issue #140).

## Decisions

### Decision 1: Recursive filesystem traversal using `node:fs/promises`
- **Choice**: Use recursive directory traversal (`readdir` with `{ withFileTypes: true }`) in `node:fs/promises` to discover `.canons` directories.
- **Rationale**: Built into Node 24 with zero external dependencies. Efficiently ignores standard noise directories (`node_modules`, `.git`, `.bare`, `dist`, `.turbo`) at the top level without walking unnecessary subtree hierarchies.
- **Alternatives considered**: External glob libraries like `glob` or `fast-glob` (rejected to maintain zero additional runtime dependencies in `@canon-clerk/core`).

### Decision 2: Configurable exclusions and explicit target precedence
- **Choice**: Provide sensible default directory exclusions with a positive polarity override flag (`defaultExcludes: boolean`, defaulting to true), a custom exclusion list (`exclude: string[]`), and ensure explicit target paths bypass default exclusions.
- **Rationale**: Gives callers full control over traversal while keeping standard runs fast and quiet. Explicit targets indicate intentional caller focus and must not be silently skipped.
- **Alternatives considered**: Hard-coded exclusions without override capability (rejected as too inflexible for monorepo edge cases).

### Decision 3: POSIX path normalization
- **Choice**: Normalize all discovered relative file paths using POSIX forward slashes (`/`), even on Windows.
- **Rationale**: Ensures deterministic diagnostic output, stable cross-platform testing, and consistent ID mapping.

### Decision 4: Presentation-agnostic aggregation contracts
- **Choice**: Return pure domain data entities capturing relative file paths, diagnostic collections, and summary counters (total files, errors, warnings, error presence) with zero presentation fields.
- **Rationale**: Keeps `@canon-clerk/core` completely divorced from terminal output, allowing CLI, GitHub Actions, and Web UI adapters to consume the identical payload and format it according to their own presentation mediums.
- **Alternatives considered**: Formatting result strings or terminal banners directly in core (rejected per `core-must-be-presentation-agnostic.md`).

### Decision 5: Graceful I/O fault tolerance
- **Choice**: If a target file is missing or unreadable, `lintWorkspace` records an error diagnostic for that file path rather than throwing an unhandled exception that aborts the entire workspace run.
- **Rationale**: CI workflows and developers need to see all valid diagnostics and failures in a single run rather than crashing on the first missing file.

## Risks / Trade-offs

- **[Risk: Large monorepos with deep directory trees]** → **Mitigation**: Prune traversal at known ignore boundaries (`node_modules`, `.git`, `.bare`, `dist`, `.turbo`) so subtrees are never traversed.
- **[Risk: Path separator discrepancies across platforms]** → **Mitigation**: Convert all backslashes (`\`) to forward slashes (`/`) when computing relative paths.
