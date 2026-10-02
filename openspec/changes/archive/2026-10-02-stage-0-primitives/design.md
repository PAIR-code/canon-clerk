# Design

## Context

Stage 0 of the Canon Clerk evaluation cascade acts as the front-line gatekeeper, filtering out canons whose scope boundaries or triggers do not intersect with target files modified in a pull request. Because Stage 0 executes ahead of expensive screening models or deep reasoning auditors, it must execute with minimal latency, zero token cost, and high determinism.

Previously, canon discovery and filtering logic was entwined with filesystem traversal. To support streaming queries across bipartite domains (target files vs. canons), Stage 0 requires pure mathematical predicates capable of evaluating scope containment and trigger matching in memory, completely decoupled from filesystem reads.

## Goals / Non-Goals

**Goals:**
- Provide pure, deterministic, zero-I/O primitives for Stage 0 filtering: scope containment evaluation and trigger pattern matching.
- Adhere to the Explicit Dependencies Principle by requiring callers to inject `workspaceRoot`, expunging ambient environment dependencies.
- Honor hierarchical monorepo boundaries: root canons apply across the entire workspace, while nested package canons apply strictly to their subtree.
- Support transparent path normalization across relative, absolute, and platform-specific path formats.
- Provide comprehensive, topical unit test suites without mocks, temporary directories, or filesystem handles.

**Non-Goals:**
- Performing filesystem traversal or streaming orchestration (deferred to `queryCanons` in Issue #156).
- Parsing canon frontmatter or Markdown bodies (handled prior to or after Stage 0 evaluation).
- Implementing CLI commands or presentation adapters (deferred to Issue #158).

## Decisions

### 1. Pure In-Memory Primitives
- **Decision:** Decompose Stage 0 evaluation into two pure functions operating entirely on in-memory path strings: scope containment evaluation and trigger matching.
- **Rationale:** By eliminating filesystem I/O, these primitives can screen hundreds of file-canon pairs in milliseconds and can be unit-tested exhaustively without disk state.
- **Alternatives Considered:** Coupling scope checks with filesystem directory traversal (creates redundant I/O during batch evaluation and couples pure path logic to disk state).

### 2. Hierarchical Scope Resolution via Ancestor Walking
- **Decision:** To determine whether a target file resides within a canon's scope, ascend directory ancestors starting from the target file's parent directory up to the workspace root. At each ancestor directory, test if the canon path matches stripped local canon glob patterns.
- **Rationale:** Stripping recursive prefixes (e.g. converting `**/.canons/**/*.md` to `.canons/**/*.md`) ensures that only direct ancestors can claim ownership, preventing a canon in a sibling package from accidentally scoping target files.
- **Alternatives Considered:** Top-down directory search (requires enumerating all intermediate directories down to the target); simple string prefix matching (fails when canons reside within nested hidden directories like `.canons/`).

### 3. Explicit Workspace Anchoring
- **Decision:** Require callers to provide an explicit workspace root path, against which all relative and absolute paths are normalized and bounded.
- **Rationale:** Enforces clean architecture and ensures identical behavior across local CLI runs, GitHub Actions runners, and long-running language server daemons.
- **Alternatives Considered:** Defaulting to the process working directory (introduces ambient environment coupling and breaks in multi-workspace or daemon processes).

### 4. Dotfile-Aware Trigger Matching
- **Decision:** Evaluate scope-relative target paths against declared trigger patterns using glob matching with dotfile matching enabled by default.
- **Rationale:** Configuration and CI files (e.g. `.github/workflows/ci.yml`, `.eslintrc.json`) frequently reside in hidden or dot-prefixed paths; omitting dotfile support causes silent false negatives.
- **Alternatives Considered:** Standard glob matching requiring explicit leading dot syntax (burdens canon authors with redundant patterns).

### 5. Universal Wildcard Fallback
- **Decision:** If a canon specifies no triggers, an empty trigger list, or a universal glob (`'**/*'`, `'**'`), treat the canon as universally triggered across its scope.
- **Rationale:** Aligns with `SPEC.md` and repository standards where canons without explicit file restrictions apply unconditionally across their scope.

## Risks / Trade-offs

- **Risk:** Recursive glob pattern stripping may behave unexpectedly with non-standard custom glob configurations.
  - *Mitigation:* Normalize all query domains through standard query normalization and validate stripped patterns against the canon path at each ancestor level.
- **Risk:** High-frequency path normalization across large diffs could incur string allocation overhead.
  - *Mitigation:* Normalization uses lightweight string operations and early-terminates on workspace boundary violations or ignore matches before ancestor traversal begins.
