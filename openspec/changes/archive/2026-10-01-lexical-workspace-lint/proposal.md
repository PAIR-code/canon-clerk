# Proposal

## Why

Currently, `lintWorkspace()` in `@canon-clerk/core` delegates discovery to `node:fs/promises.glob()`. Because `glob()` performs depth-first descent over unsorted `fs.readdir()` results, canon files are yielded in arbitrary filesystem hash/inode order. This causes non-deterministic output ordering across operating systems (Linux vs. macOS), filesystems, and fresh git checkouts, forcing downstream consumers like the CLI to buffer the entire stream in memory to achieve stable sorting.

By sorting entries lexically at each directory level during depth-first descent, `lintWorkspace()` can naturally stream results in deterministic lexicographic order in real time with zero global buffering.

## What Changes

- Update `@canon-clerk/core`'s internal workspace traversal to sort entries alphabetically at each directory level prior to recursive descent.
- Guarantee that `lintWorkspace()` yields `FileLintResult` records in strictly deterministic lexicographic order across the entire workspace or targeted paths.
- Add unit tests asserting deterministic lexicographic yield order across nested directories.

## Capabilities

### Modified Capabilities
- `canon-linter/workspace`: Updates the streaming workspace linting orchestration contract to require deterministic lexicographic yield order during discovery and evaluation.

## Impact

- `@canon-clerk/core`: `lintWorkspace()` becomes strictly deterministic across platforms without changing its public API signature (`AsyncGenerator<FileLintResult>`).
- Downstream consumers (`@canon-clerk/cli`, CI actions): Can stream formatted results directly to the terminal in real time while guaranteeing reproducible, sorted output.
