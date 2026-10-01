# Design

## Context

`@canon-clerk/core` provides `lintWorkspace()` as an `AsyncGenerator<FileLintResult>` for real-time pipeline evaluation. Currently, discovery delegates to `node:fs/promises.glob()`, which visits directories using raw `fs.readdir()` results. Because directory entries are returned in arbitrary filesystem hash/inode order on modern filesystems (such as Linux ext4 and tmpfs), yielded results arrive in non-deterministic order across operating systems and fresh checkouts.

To satisfy reproducible execution requirements without violating streaming invariants, discovery must produce a deterministically ordered stream.

## Goals / Non-Goals

**Goals:**
- Guarantee strictly deterministic, ascending lexicographic yield order for `lintWorkspace()`.
- Preserve true real-time async generator streaming with $O(\text{depth})$ memory overhead rather than buffering all workspace results globally.
- Preserve existing `.gitignore` ignore filtering semantics, default noise pruning, and explicit target precedence.

**Non-Goals:**
- Introducing configurable sort strategies or opt-out flags for non-lexical traversal, as non-deterministic ordering provides no architectural or operational advantage.

## Decisions

### Decision 1: Per-Directory Sorted Depth-First Descent
- **Choice:** Sort directory entries lexicographically at each individual directory level before recursing into subdirectories or yielding files, rather than accumulating all workspace paths into a global array.
- **Rationale:** In depth-first traversal, sorting entries at each directory level guarantees that sibling directories and files are evaluated in canonical hierarchical lexicographic order. This achieves 100% deterministic streaming with zero global buffering overhead.
- **Alternatives considered:**
  - *Global array buffering:* Collect all matching paths, sort globally, and yield. Rejected because it breaks real-time streaming pipeline overlap and increases peak memory usage on massive workspaces.
  - *Post-hoc sorting in consumers:* Require CLI and other consumers to buffer results. Rejected because it shifts a core determinism concern to every presentation adapter and leaves the domain service non-deterministic.

### Decision 2: Unconditional Lexicographic Invariant
- **Choice:** Enforce lexicographic ordering unconditionally across all workspace runs.
- **Rationale:** Deterministic order is essential for snapshot reproducibility, consistent CI logs, and stable diff evaluation. There is no valid use case where non-deterministic traversal order is advantageous.
- **Alternatives considered:** An opt-out configuration flag (e.g. `sort: false`). Rejected as premature complexity without a driving use case.

## Risks / Trade-offs

- **[Risk: Local sorting latency on large directories]** → **Mitigation:** Sorting entries within a single directory operates on small arrays (typically tens or hundreds of strings), introducing sub-millisecond CPU overhead that is completely negligible compared to filesystem I/O and schema CST parsing.
