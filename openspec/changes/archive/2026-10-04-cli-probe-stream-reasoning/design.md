# Design: Streaming Reasoning Thoughts and Latency Telemetry in check-config Probing

## Context

Phase 1 active endpoint probing (`canon-clerk check-config --probe`) tests live model connectivity, authentication, and schema decoding. Frontier models like Gemini 2.5/3.x generate internal reasoning thoughts prior to producing output.

Previously, `probeTier` used unary `generateStructured`, resulting in a frozen terminal for several seconds and no breakdown of roundtrip latency. With `streamStructured` now available in `@canon-clerk/core`, `probeTier` can stream reasoning thoughts as they arrive and measure fine-grained latency benchmarks.

## Goals / Non-Goals

**Goals:**
- Stream reasoning thoughts to the user in real time during `check-config --probe`.
- Direct thought streams to `stderr` so `stdout` remains pristine and parseable (especially under `--json`).
- Provide an independent opt-out flag (`--no-thoughts`) and environment variable (`CANON_CLERK_PROBE_THOUGHTS=false`) to silence thoughts while retaining final diagnostic output.
- Capture granular latency telemetry (`timeToFirstThoughtMs`, `timeToFirstTokenMs`, `thoughtTokens`).
- Surface latency metrics in both stylish terminal output (dimmed `Latency:` line) and canonical JSON.
- Respect `--quiet` (`-q`) by suppressing thought output.
- Gracefully fall back to unary generation if `streamStructured` is not implemented on the client.
- Maintain full testability offline with `createMockModelClient`.

**Non-Goals:**
- Interactive full-screen TUI: A simple stream to `stderr` provides immediate feedback without heavy terminal dashboard dependencies.

## Decisions

### 1. Route Real-Time Thoughts to Stderr
- **Decision:** Stream live thought tokens to `stderr` rather than `stdout`.
- **Rationale:** Standard UNIX practice. Keeps `stdout` dedicated to the primary output format (stylish diagnostic tree or canonical JSON document). Allows piping `canon-clerk check-config --probe --json | jq .` without parse failures while still showing live progress on stderr.

### 2. Stream Consumer in probeTier
- **Decision:** In `probeTier`, if `client.streamStructured` is available, iterate the generator. Call `onThought` for each `{ type: 'thought', delta }` and extract metadata from `{ type: 'finish' }`.
- **Rationale:** Leverages the newly built `streamStructured` API natively and cleanly decouples stream consumption from CLI rendering.

### 3. Opt-out Control via --no-thoughts
- **Decision:** Enable thought streaming by default when `--probe` is active, with `--no-thoughts` (or `CANON_CLERK_PROBE_THOUGHTS=false`) available to silence the stream.
- **Rationale:** Solves the 10-second interactive blindness problem out-of-the-box for interactive users, while allowing CI and automation to suppress intermediate tokens cleanly without sacrificing final diagnostic reports.

### 4. Streaming Latency Telemetry Attribution
- **Decision:** Measure `timeToFirstThoughtMs` (elapsed ms from dispatch to first thought chunk) and `timeToFirstTokenMs` (elapsed ms from dispatch to first payload token), and retrieve `thoughtTokens` from `usage`.
- **Rationale:**
  - Allows distinguishing network/handshake delay from reasoning duration.
  - In JSON, adds optional fields for programmatic performance tracking.
  - In Stylish tree, displays a clean `Latency:` line only when reasoning occurred (e.g. `280ms to first thought · 820ms reasoning (42 tokens)`).

### 5. Graceful Fallback
- **Decision:** If `streamStructured` is not a function on the client instance, fall back to `generateStructured` or `generateStructuredJson`.
- **Rationale:** Ensures custom or future `ModelClient` implementations that only provide unary generation continue to work without error.

## Risks / Trade-offs

- **Risk:** Fast endpoints without reasoning emit no thoughts, causing no stderr output before the final report.
  - **Mitigation:** Expected behavior; fast responses (<500ms) do not need progress indication or reasoning latency lines.
