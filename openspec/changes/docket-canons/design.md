# Design: Phase 2 Docket Canons Macro Triage

## Context
In Canon Clerk's Three-Phase Evaluation Cascade, Phase 1 (`check-triggers`) performs fast, deterministic path filtering based on glob patterns in canon frontmatter. This reduces 100+ canons down to ~25 candidate canons. However, path matching often catches changes outside a canon's semantic scope. Running Phase 3 (substantive audit with frontier reasoning models) across all candidate canons is slow and expensive.

Step 1 of Phase 2 (`docketCanons`) bridges this gap by performing Macro Triage in a single batched call with a fast model tier (`gemini-flash-lite-latest`). It determines subject-matter jurisdiction ("colorable claim to be heard") rather than pass/fail compliance, narrowing the docket to ~3–5 canons before detailed exhibit inspection in Step 2 (`docketCanonTargets`).

With `ModelClient` providing both unary (`generateStructured`) and streaming (`streamStructured`) capabilities, structuring `docketCanons` as an **async generator** mirrors the underlying stream, yields live reasoning thoughts for terminal/UI progress, and terminates with the final object of inquiry (`finish` carrying assessments and latency telemetry).

## Goals / Non-Goals
**Goals:**
- Provide `docketCanons(canons, context, options)` as an async generator (`AsyncGenerator<DocketCanonsEvent>`) in `@canon-clerk/core`.
- Provide `collectDocketCanons(canons, context, options)` convenience helper resolving directly to `DocketCanonsResult`.
- Batch all candidate canons into a single prompt to amortize baseline tokens and roundtrip latency.
- Enforce strict structured decoding via dynamically generated Zod schemas so models cannot omit, rename, or hallucinate canon keys.
- Preserve 0-token efficiency by short-circuiting on empty candidate arrays.
- Bound prompt token bloat via configurable diff budgeting (`maxDiffBytes`, default 100KB) with graceful per-file truncation notices.
- Stream reasoning thoughts in real time as `{ type: 'thought', delta }` events without callback inversion.
- Conclude the stream with `{ type: 'finish', assessments, telemetry }` carrying fine-grained diagnostic and latency telemetry (`durationMs`, `timeToFirstThoughtMs`, `timeToFirstTokenMs`, `thoughtChunks`, `thoughtTokens`, `resolvedModel`, `usage`).
- Gracefully fall back to unary execution if `streamStructured` is not implemented on the client.
- Support hermetic, offline unit testing using `createMockModelClient`.

**Non-Goals:**
- Evaluating substantive rule compliance or pass/fail verdicts (strictly reserved for Phase 3 `audit`).
- Inspecting granular line hunks or individual AST nodes per canon (reserved for Step 2 `docketCanonTargets`).
- Loading canon definitions from disk (canons are passed as pre-parsed `Canon` objects from Phase 1).
- Direct Git diff generation or filesystem I/O (diffs are passed in `context.diffs` as `Record<string, FileArtifact>`).

## Decisions

### 1. Single-Pass Aggregate Prompting
- **Decision:** Present PR metadata and all candidate canon invariants in one prompt, requesting a unified JSON object keyed by canon path.
- **Rationale:** Evaluating 25 candidate canons serially or concurrently in 25 separate requests multiplies network overhead, TTFT, and token costs by 25x. Aggregating into a single call amortizes PR metadata and completes in ~400–600ms using `gemini-flash-lite-latest`.
- **Alternatives Considered:** 
  - *Per-canon individual calls:* Rejected due to 25x HTTP roundtrip latency and multiplied prompt overhead.
  - *Chunked batches (e.g. 5 at a time):* Adds orchestration complexity with little gain given Flash-Lite's large context window.

### 2. Dynamic Strict Zod Schema Construction
- **Decision:** Build a runtime Zod schema (`z.object({...}).strict()`) using each `canon.path` as an exact required key, with fields `colorabilitySummary` (string) and `colorabilityScore` (number in `[0, 1]`).
- **Rationale:** Guarantees that every candidate canon receives an assessment without skips or extraneous keys. Structuring `colorabilitySummary` before `colorabilityScore` forces the model to articulate reasoning before scoring, improving calibration.
- **Alternatives Considered:**
  - *Array output (`[{ path, score, reason }]`):* Vulnerable to missing canons or duplicates. Object-key mapping ensures 1:1 parity with candidate inputs.

### 3. Graceful Diff Budgeting
- **Decision:** Introduce `maxDiffBytes` (default 100,000 bytes). If the aggregate diff size exceeds this budget, truncate individual file patches with an inline diagnostic marker (`[Diff truncated: file exceeds per-file budget; see diff stats]`), while preserving file paths and line delta counts.
- **Rationale:** Large PRs (e.g. package lockfiles, generated code, vendored assets) can overwhelm context windows or inflate latency. Preserving file paths and line counts ensures the model still knows the file was modified without wasting tokens on non-semantic deltas.
- **Alternatives Considered:**
  - *Omitting diffs entirely on overflow:* Deprives the model of all diff context for smaller files in the same PR.
  - *No diff budget:* Risks out-of-context errors and excessive latency on massive commits.

### 4. Async Generator Contract with Final Finish Event
- **Decision:** Structure `docketCanons` as an `AsyncGenerator<DocketCanonsEvent, void, unknown>`. Yield `{ type: 'thought', delta }` events as reasoning tokens arrive from `client.streamStructured`, and yield a single `{ type: 'finish', assessments, telemetry }` as the terminal event.
- **Rationale:** 
  - Eliminates callback inversion (`options.onThought` callback parameter): callers naturally use `for await (const event of docketCanons(...))`.
  - Native lifecycle & cancellation: breaking out of the loop triggers generator cleanups and aborts the stream cleanly.
  - Mirrors the pattern established by `ModelClient.streamStructured` and `check-config --probe`.
  - Captures `timeToFirstThoughtMs` (first thought event) and `timeToFirstTokenMs` (first payload delta or finish) while counting `thoughtChunks`.
  - For unary clients (or test mocks without streaming), falls back cleanly by executing unary generation and yielding only the `finish` event.
- **Alternatives Considered:**
  - *Callback-based streaming (`options.onThought`):* Creates awkward callback lifecycles, makes error propagation clunky, and separates thought handling from the final return.
  - *Returning a stream handle or promise tuple:* Non-idiomatic in modern TypeScript compared to standard async generators.

### 5. Convenience Collector Helper (`collectDocketCanons`)
- **Decision:** Provide `collectDocketCanons(canons, context, options)` which drains the async generator and returns `Promise<DocketCanonsResult>`.
- **Rationale:** Allows non-streaming callers (e.g., standard unit tests or batch scripts) to obtain `{ assessments, telemetry }` in a single `await` without writing boilerplate `for await` loops.

### 6. Zero-Canons Short-Circuit
- **Decision:** When `canons.length === 0`, immediately yield `{ type: 'finish', assessments: {}, telemetry: { durationMs: 0 } }` without calling `ModelClient`.
- **Rationale:** Preserves 0-token efficiency when `check-triggers` yields no candidates.

## Risks / Trade-offs
- **Model hallucination on borderline relevance:** Flash-Lite may occasionally score a marginal canon above 0.5. *Mitigation:* Step 2 (`docketCanonTargets`) and Phase 3 (`audit`) serve as downstream validation layers; a false positive in triage simply proceeds to exhibit check, whereas a false negative is prevented by prompt instructions encouraging colorability when ambiguous.
- **Diff truncation obscuring salient changes:** Very large diffs may have important changes truncated. *Mitigation:* Truncation prioritizes preserving file paths, line statistics, and PR title/body.
