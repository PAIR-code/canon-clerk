# Design: Model Client & Structured Generation

## Context

Phase 2 and Phase 3 require structured LLM evaluation. Domain logic in `@canon-clerk/core` must remain decoupled from specific provider SDKs and host filesystem credentials.

## Architectural Decisions

1. **Inversion of Control via ModelClient:**
   `@canon-clerk/core` defines the `ModelClient` interface. Cascade evaluators depend only on this abstraction, never concrete SDKs.

2. **Dynamic Imports for Latency Hygiene:**
   `GoogleModelClient` loads `ai` and `@ai-sdk/google` dynamically on first generation call. CLI commands for Phase 1 (`check-canons`, `check-triggers`) never load model SDKs into Node memory.

3. **Deterministic Offline Mock Double:**
   `createMockModelClient` enables comprehensive unit tests of cascade logic offline in <10ms without API keys or network I/O.
