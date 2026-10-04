# Proposal: Echo Reasoning Thoughts and Capture Streaming Telemetry during check-config Probing

## Why

When running `canon-clerk check-config --probe` against frontier reasoning models (such as Gemini 2.5/3.x Flash or Pro), active probing can take 3–15 seconds while the model generates internal reasoning thoughts before emitting the structured `{ ok: true }` connectivity payload.

Under the previous unary generation implementation:
1. **Interactive Blindness:** Developers and operators face a silent freeze with no feedback indicating progress or whether remote connection has stalled.
2. **Missing Latency Attribution:** Total roundtrip latency cannot be decomposed to determine whether bottlenecks are caused by network/queueing delays, model reasoning duration, or output token decoding.

With `@canon-clerk/core` now supporting `streamStructured`, `check-config --probe` can consume streaming thought deltas and echo them in real time to `stderr` while awaiting the verdict body, while capturing granular latency telemetry (Time-to-First-Thought, Time-to-First-Token, thought token count). Additionally, developers and CI environments need a dedicated control (`--no-thoughts` / `CANON_CLERK_PROBE_THOUGHTS=false`) to silence live thoughts without silencing the entire final diagnostic report (`--quiet`).

## What Changes

1. **Streaming Probe Runner & Telemetry Extraction:**
   - Update `probeTier` in `packages/cli/src/commands/probe-runner.ts` to invoke `client.streamStructured` when available, falling back to unary `generateStructured` or `generateStructuredJson`.
   - Record `timeToFirstThoughtMs` (time from dispatch to first thought delta) and `timeToFirstTokenMs` (time from dispatch to first content token).
   - Extract `thoughtTokens` from `finish` event usage metrics.
   - Accept an optional `onThought?: (delta: string, tier: ModelTier) => void` callback on `ProbeTierOptions` and `ExecuteCascadeProbesOptions`.
2. **Real-time Thought Echoing & Control:**
   - In `runCheckConfigCommand` (`packages/cli/src/commands/check-config.ts`), wire `onThought` to write streamed reasoning chunks to `stderr` in real time when probing.
   - Add `--no-thoughts` CLI flag and `CANON_CLERK_PROBE_THOUGHTS` environment variable (defaults to `true`). When disabled or when `--quiet` is passed, thought echoing is suppressed.
   - Writing to `stderr` ensures stdout remains pure, unadorned JSON when `--json` is requested (preserving machine-readability for pipelines and `jq`).
   - In interactive TTY mode, style thoughts with dim styling for clean visual hierarchy.
3. **Diagnostic Reporting & Formatter Updates:**
   - In `ModelTierProbeResult` (`@canon-clerk/configuration`), add optional `timeToFirstThoughtMs`, `timeToFirstTokenMs`, and `thoughtTokens`.
   - In Stylish format, render a dimmed `Latency:` line under `Probe:` when thoughts occurred (e.g. `280ms to first thought · 820ms reasoning (42 tokens)`).
   - In JSON format (`--json`), include telemetry fields in the `probe` object.
4. **Unit Tests & Living Specs:**
   - Add unit tests for `probeTier` streaming thoughts, telemetry attribution, callback invocation, and fallback handling.
   - Add tests verifying stderr receives thought stream, `--no-thoughts` silences them, and stdout remains intact with telemetry.
   - Update living specification in `openspec/specs/cli/check-config/spec.md`.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `cli/check-config`: Update endpoint probing to stream reasoning thoughts via `streamStructured`, echo them to `stderr` during active probe execution, capture streaming latency telemetry (`timeToFirstThoughtMs`, `timeToFirstTokenMs`, `thoughtTokens`), and support `--no-thoughts` / `CANON_CLERK_PROBE_THOUGHTS` silencing.

## Impact

- **CLI (`@canon-clerk/cli`):** `check-config --probe` becomes responsive, transparent, and diagnostic during model reasoning.
- **Configuration (`@canon-clerk/configuration`):** Extends `ModelTierProbeResult` with optional telemetry fields.
- **Pipelines:** Stdout JSON format gains rich latency breakdown while remaining 100% valid and parseable.
