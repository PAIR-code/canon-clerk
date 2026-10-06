# Proposal

## Why
Downstream pipeline nodes (`intake`, `discover`, `docket`) require inspecting unified diff streams from pull requests, local Git commits, or standard input pipelines to catalog code changes into immutable `FileArtifact` maps. Standard third-party diff parsers either fail on Git extended headers (100% similarity renames without hunks, copy tracking, binary markers, file mode changes) or impose bulky, mismatched ASTs and external dependencies.

Furthermore, per `.canons/api-design/multi-resource-operations-must-stream-results.md`, asynchronous domain operations transforming resource collections must stream results rather than buffering entire collections in memory. Introducing both an asynchronous streaming generator (`parseUnifiedDiffStream`) for real-time pipeline ingestion and a synchronous in-memory parser (`parseUnifiedDiff`) provides optimal ergonomics for both streaming CLI pipelines and in-memory evaluation.

Per `.canons/git-workflow/precursor-domain-refactors-must-ship-independently.md` and `.canons/architecture/reinventing-standard-solutions-is-forbidden.md`, foundational domain primitives must ship independently of presentation or pipeline node orchestrations. Providing a lightweight, pure TypeScript, zero-dependency in-memory and streaming unified diff parser in `@canon-clerk/core` enables clean intake orchestration and strict data plane typing.

## What Changes
- Implement `parseUnifiedDiffStream` in `@canon-clerk/core` yielding an asynchronous generator of `FileArtifact` domain entities from chunked stream inputs (`AsyncIterable`).
- Implement `parseUnifiedDiff` in `@canon-clerk/core` to parse in-memory unified diff strings into frozen `Record<string, FileArtifact>` maps.
- Support Git lifecycle statuses (`'added'`, `'modified'`, `'deleted'`, `'renamed'`, `'copied'`).
- Extract Git extended headers (`rename from`/`to`, `copy from`/`to`, `new file mode`, `deleted file mode`).
- Extract hunk headers (`@@ -l,s +l,s @@`, including single-line coordinates) and calculate line statistics.
- Enforce explicit omission rationale semantics for unchanged renames and binary files via `createFileArtifact`.
- Unquote and unescape quoted paths containing whitespace or special characters.
- Separate tokenization from semantic parsing per `.canons/parser-design/parsers-must-separate-tokenization-from-semantic-ast-construction.md`.
- Export `parseUnifiedDiff` and `parseUnifiedDiffStream` from `@canon-clerk/core`.

## Capabilities
### Modified Capabilities
- `cascade-data-plane`: Expands the data plane specification with requirements for both synchronous in-memory and asynchronous streaming unified diff parsing into `FileArtifact` representations.

## Impact
- **Affected Packages:** `@canon-clerk/core`.
- **Dependencies:** Zero new runtime dependencies; zero child processes.
- **Consumers:** CLI commands, GitHub Actions, and intake cascade nodes can consume diff streams incrementally via async generators or batch-parse diff strings directly into typed `FileArtifact` collections.
