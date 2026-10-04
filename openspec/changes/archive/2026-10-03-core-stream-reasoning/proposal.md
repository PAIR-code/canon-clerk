# Proposal: Stream Reasoning Thoughts and Structured Events in @canon-clerk/core

## Why

Phase 1 evaluation commands (`check-canons`, `check-triggers`) use `AsyncGenerator` streams to deliver deterministic, memory-efficient, and pipeline-friendly diagnostics. However, the model client layer in `@canon-clerk/core` (`ModelClient`, `GoogleModelClient`) currently exposes only unary Promise-based APIs (`generateStructuredJson`, `generateStructured`).

When evaluating complex engineering canons against diff exhibits in Phase 2 (Docket) and Phase 3 (Audit), reasoning models (such as Gemini 2.5/3.x Flash and Pro with reasoning effort) take 5–15 seconds to reason before producing a structured verdict. Under the unary contract:
1. **Interactive Blindness:** The CLI user stares at a frozen prompt with zero visibility into model reasoning.
2. **Missing Telemetry:** Latencies cannot be decomposed into Time-to-First-Thought (TTFTT), Time-to-First-Token (TTFT), reasoning duration, and payload generation duration.
3. **Architectural Divergence:** The LLM client remains a blocking black box while the rest of Canon Clerk is stream-native.

## What Changes

1. **Stream Event Domain Types (`@canon-clerk/core`):**
   - Introduce `ModelStreamEvent<T>` union (`thought`, `text-delta`, `finish`).
   - Introduce `ModelUsage` interface (`promptTokens`, `completionTokens`, `thoughtTokens`, `totalTokens`).
2. **Dual-Mode ModelClient Interface:**
   - Preserve `generateStructuredJson` and `generateStructured` for unary, probe, and headless workflows.
   - Add optional `streamStructured?<T>(request: StructuredGenerationRequest<T>): AsyncGenerator<ModelStreamEvent<T>, void, unknown>` to `ModelClient`.
3. **Google Provider Streaming Adapter:**
   - Implement `streamStructured` in `GoogleModelClient` using Vercel AI SDK's `streamText()`.
   - Consume `fullStream` to yield reasoning chunks as `{ type: 'thought', delta }` and text deltas as `{ type: 'text-delta', delta }`.
   - Maintain dynamic imports of `ai` and `@ai-sdk/google` to preserve Phase 1 startup latency (<10ms).
   - Enforce Zod schema validation on the accumulated JSON payload before yielding `{ type: 'finish', output, resolvedModel, usage, durationMs }`.
   - Propagate `AbortSignal` and consumer iteration cancellations cleanly to abort HTTP streams.
4. **Offline Mock Fixture Streaming:**
   - Update `createMockModelClient` to support `mockThoughts?: string[]` or custom generator simulation for deterministic offline testing.
5. **Living Specifications & Unit Tests:**
   - Extend `openspec/specs/model-client/spec.md` with streaming specifications and scenarios.
   - Author comprehensive unit tests covering thought streaming, cancellation, schema enforcement on invalid payloads, and usage attribution.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `model-client`: Added `streamStructured` async generator contract, `ModelStreamEvent`, `ModelUsage`, `GoogleModelClient` streamText adapter with reasoning thought routing, and streaming mock fixtures.

## Impact

- **Core Engine:** `@canon-clerk/core` provides streaming domain events alongside existing unary generation.
- **Downstream Consumers:** Unblocks Phase 2 and Phase 3 CLI progress bars, interactive spinners, and telemetry logging without altering existing unary callers.
