# Design

## Context
Canon Clerk's Caseload pipeline begins at the `intake` stage, where pull request changes, commit diffs, or standard input streams are transformed into the cumulative `Caseload` container. The data plane models changes as `FileArtifact` entities recording path, status, line counts, unified patches, and omission rationales.

To populate these artifacts from standard Git diff outputs (such as `git diff`, GitHub pull request diff payloads, or piped `stdin` streams), the system requires a high-performance diff parser. Per `.canons/api-design/multi-resource-operations-must-stream-results.md`, asynchronous domain operations transforming collections of resources must yield results as an asynchronous generator rather than buffering entire collections in memory. Introducing a streaming parser alongside the synchronous in-memory parser enables both incremental CLI pipeline reporting and lightweight in-memory evaluations.

## Goals / Non-Goals
**Goals:**
- Provide an asynchronous generator yielding `FileArtifact` entities incrementally from chunked input streams (`AsyncIterable`).
- Provide a synchronous in-memory parser translating unified diff strings into a frozen `FileArtifact` map.
- Fully support Git extended headers, tracking file renames and copies with historical source paths.
- Decouple lexical tokenization from semantic artifact assembly in alignment with repository compiler canons.
- Enforce strict omission rationales for unpopulated patches or content via domain validation factories.
- Accurately parse hunk headers, single-line coordinates, and calculate line change counts.

**Non-Goals:**
- Executing Git CLI commands or sub-processes (the parser operates exclusively on in-memory strings and streaming text chunks).
- Synthesizing full file contents from patches (content retrieval is performed during subsequent discovery or intake stages).
- Full syntax AST parsing of individual file contents.

## Decisions

### 1. Two-Stage Lexical Scanner and Streaming Assembler Architecture
- **Decision:** Separate parsing into a lexical scanner that normalizes incoming chunks into discrete line tokens and a semantic assembler that yields `FileArtifact` instances at file demarcation boundaries.
- **Rationale:** Adheres to compiler architecture principles and repository parser canons. Streaming by file demarcation boundaries (`diff --git` or stream termination) ensures O(1) file-buffering memory overhead and enables immediate downstream processing before upstream network or pipe streams complete.
- **Alternatives Considered:**
  - *Full diff buffering before parsing:* Simpler initial implementation, but violates `.canons/api-design/multi-resource-operations-must-stream-results.md` and prevents progressive terminal reporting on large diffs.

### 2. Dual Streaming and Synchronous Materialization Entrypoints
- **Decision:** Provide two distinct, complementary entrypoints in `@canon-clerk/core`:
  - `parseUnifiedDiffStream`: Asynchronous generator yielding `FileArtifact` items one by one from any chunked stream.
  - `parseUnifiedDiff`: Pure synchronous parser converting an in-memory diff string directly into a frozen `Record<string, FileArtifact>`.
- **Rationale:** Satisfies the multi-resource streaming canon for I/O-bound stream consumers (CLI `stdin`, HTTP response streams) while providing a zero-overhead synchronous API for unit tests, GitHub Action webhook payloads, and already-materialized string contexts.

### 3. Domain Factory Delegated Validation
- **Decision:** Route all artifact generation through the existing domain factory `createFileArtifact` in `@canon-clerk/core`.
- **Rationale:** Leverages established validation for POSIX relative paths, non-negative line statistics, and mandatory omission rationale invariants. Prevents duplicate validation logic and ensures that parser output guarantees data plane integrity.
- **Alternatives Considered:**
  - *Direct object literal instantiation in the parser:* Risks divergence from schema invariants if validation constraints evolve.

### 4. Git Extended Header and Omission Semantics Mapping
- **Decision:** Map Git extended headers directly to data plane statuses and omission reasons:
  - 100% similarity renames without hunks set status to renamed with unchanged patch omission rationale.
  - Binary file markers set binary patch and content omission rationales with zero line statistics.
  - Deleted files set status to deleted with deleted content omission rationale.
  - Added, modified, and copied files set not-requested content omission rationale when full content is not provided in the diff.
- **Rationale:** Faithfully represents change semantics while strictly honoring schema omission requirements.

## Risks / Trade-offs
- **Chunk Boundary Slicing:** Arbitrary chunk boundaries from streaming inputs can split across newline characters or header lines.
  - *Mitigation:* The stream tokenizer buffers trailing incomplete lines across chunk boundaries, ensuring the line token stream always delivers complete, clean lines to the semantic assembler.
- **Path Quoting Complexity:** Git quotes file paths containing spaces or special characters using C-style escaping.
  - *Mitigation:* Implement an unquoting and unescaping routine that strips surrounding double quotes and resolves standard escape sequences and octals into clean POSIX paths.
