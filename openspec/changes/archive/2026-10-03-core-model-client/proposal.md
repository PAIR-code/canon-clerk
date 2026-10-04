# Proposal: Core Model Client and Gemini Provider Adapter

## Why

Phase 2 (Docket) and Phase 3 (Audit) of Canon Clerk's Three-Phase Evaluation Cascade require structured LLM interactions with schema-constrained JSON output:
- **Phase 2 (Docket):** Requires fast, reason-first screening of candidate canons against PR diff exhibits.
- **Phase 3 (Audit):** Requires frontier reasoning models to adjudicate substantive rule violations.

Adopting the Vercel AI SDK (`ai` + `@ai-sdk/google`) within `@canon-clerk/core` provides a standardized multi-provider foundation with schema-constrained decoding while maintaining clean architectural boundaries.

## What Changes

1. **ModelClient Abstraction:**
   - Define `ModelClient` interface and `StructuredGenerationRequest<T>` in `packages/core/src/client.ts`.
2. **Production Provider Adapter:**
   - Implement `GoogleModelClient` wrapping Vercel AI SDK (`generateText` + `Output.object`) with dynamic imports to protect Phase 1 CLI startup latency.
3. **Provider Factory:**
   - Implement `createModelClient` mapping `ModelConfig` to provider adapters.
4. **Offline Test Fixture:**
   - Implement `createMockModelClient` for deterministic offline testing (<100ms, zero network/credentials).
5. **Living Documentation:**
   - Establish living specifications under `openspec/specs/model-client/spec.md`.

## Capabilities

### New Capabilities
- `model-client`: Structured JSON generation contract, provider adapters, and deterministic offline mock fixture.

### Modified Capabilities
- None.

## Impact

- **Core Engine:** `@canon-clerk/core` provides the foundational model invocation layer.
- **Consumers:** Unblocks downstream cascade subroutines (`docketCanons`, `docketCanonTargets`, and semantic auditor).
