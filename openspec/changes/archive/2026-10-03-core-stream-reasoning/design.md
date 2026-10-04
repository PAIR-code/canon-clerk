# Design: Streaming Reasoning Thoughts and Structured Events

## Context

Canon Clerk's Phase 1 pipeline uses `AsyncGenerator` streams to achieve high performance and streaming output. Phase 2 (Docket) and Phase 3 (Audit) require structured LLM evaluation where frontier reasoning models generate reasoning traces before emitting schema-constrained JSON verdicts.

Currently, `ModelClient` only provides unary Promise APIs. To enable live UX feedback and fine-grained latency telemetry while keeping Canon Clerk deterministic and decoupled, we extend `ModelClient` to support streaming structured events.

## Goals / Non-Goals

**Goals:**
- Provide a dual-mode client interface supporting both unary Promise resolution and asynchronous generator streaming.
- Route model reasoning chunks (`thought`) separately from structured verdict output (`finish`).
- Guarantee that `finish` output is strictly validated against the request's Zod schema before the stream completes.
- Support request cancellation and early loop termination via `AbortSignal`.
- Enable offline, deterministic unit testing via `createMockModelClient` without network calls.
- Preserve Phase 1 CLI startup latency (<10ms) via dynamic SDK imports.

**Non-Goals:**
- Partial JSON stream parsing: consumers do not need or parse incomplete JSON objects mid-flight; only reasoning thought strings and the final validated verdict object are required.
- Modifying CLI audit presentation in this change: downstream CLI spinners and progress formatters will be integrated in subsequent issues.

## Decisions

### 1. Unified AsyncGenerator Event Stream
- **Decision:** Expose `streamStructured<T>(request)` returning `AsyncGenerator<ModelStreamEvent<T>, void, unknown>`.
- **Rationale:** Async generators are native TypeScript/JavaScript primitives that fit seamlessly into Node.js, support standard `for await (const event of ...)` syntax, and automatically handle cleanup/breakage via `finally` blocks and `AbortSignal`.
- **Alternatives Considered:**
  - Callback-based hooks (`onThought(chunk)`): Rejects idiomatic async iteration and makes composition/cancellation harder.
  - Exposing raw Vercel AI SDK streams directly: Leaks external library types across package boundaries and complicates offline testing.

### 2. Event Discriminated Union
- **Decision:** Define `ModelStreamEvent<T>` with distinct event types:
  - `{ type: 'thought', delta: string }`
  - `{ type: 'text-delta', delta: string }`
  - `{ type: 'finish', output: T, resolvedModel?: string, usage?: ModelUsage, durationMs: number }`
- **Rationale:** Clear discrimination enables consumers to easily handle reasoning progress while maintaining complete type safety on the final validated payload.

### 3. Separation of Concerns in Consumption
- **Decision:** Consumers receive reasoning thoughts as plain string deltas while structured evaluation results are schema-validated on finish.
- **Rationale:** Audit rules must be grounded in strictly typed outputs, while reasoning visibility is strictly for human observation and latency telemetry.

### 4. Vercel AI SDK streamText with fullStream
- **Decision:** In `GoogleModelClient`, invoke `streamText` from `ai` and iterate its `fullStream` property.
- **Rationale:** `streamText({ model, prompt, ... }).fullStream` yields granular stream parts including `reasoning` chunks, `text-delta` chunks, and final `finish` step metadata, mapping directly to our domain events.

### 5. Error Signaling via Native Async Generator Rejection (`throw`)
- **Decision:** When schema validation fails or unexpected transport errors occur, the generator throws a native exception (rejecting the current `.next()` Promise) rather than yielding a `{ type: 'error' }` event object.
- **Rationale:**
  - **API Parity:** Preserves exact semantic symmetry with unary `generateStructuredJson` and `generateStructured`, which reject on schema violation or network failure.
  - **Consumer Ergonomics:** Allows callers to wrap standard `try / catch` around the `for await (const event of ...)` stream, keeping failure handling separated from event processing.
  - **Guaranteed Invariant:** The stream closes immediately with `done: true`, executing `finally` blocks (cleaning up underlying HTTP resources) and guaranteeing that an invalid or half-formed `finish` event can never be reached if validation fails.
- **Alternatives Considered:**
  - *Yielding `{ type: 'error', error }`:* Evaluated and rejected because it forces consumers to branch on error conditions inside the hot streaming loop and weakens the semantic guarantee of stream completion.

## Risks / Trade-offs

- **Risk:** Provider reasoning chunk structure or naming changes in future SDK releases.
  - **Mitigation:** Abstract provider details behind `@canon-clerk/core`'s `ModelStreamEvent`, and test with mock fixtures and adapter tests.
- **Risk:** Uncaught exceptions when stream is aborted mid-generation.
  - **Mitigation:** Wrap iteration in `try ... finally` blocks and ensure `AbortSignal` cleanly aborts the underlying provider call without throwing uncaught rejections.
