# Proposal: Phase 2 Docket Canons Macro Triage

## Why
Phase 1 (`check-triggers`) deterministically filters workspace canons down to a candidate set based purely on path boundaries. However, many canons specify broad trigger globs (e.g. `packages/cli/**/*.ts` or `**/*`) that catch changes outside their semantic jurisdiction (such as documentation edits, formatting tweaks, or unrelated refactors). Naively running Phase 3 frontier reasoning audits over all candidate canons causes excessive token expenditure and latency.

`docketCanons` (Step 1 of the Docket Phase) resolves this by performing fast **Macro Triage**: evaluating all candidate canons in a single aggregate model call against the pull request metadata and unified diffs (constrained by a configurable diff budget). It scores subject-matter jurisdiction to filter out non-colorable candidates before detailed exhibit assembly in Step 2.

To provide first-class visibility into model execution without callback inversion, `docketCanons` is structured as an **async generator** (`AsyncGenerator<DocketCanonsEvent>`). It streams reasoning thoughts (`{ type: 'thought', delta }`) in real time as they arrive, and concludes with a final event (`{ type: 'finish', assessments, telemetry }`) carrying the colorability assessments and fine-grained latency telemetry (`timeToFirstThoughtMs`, `timeToFirstTokenMs`, `thoughtTokens`, and token usage). A convenience helper (`collectDocketCanons`) is provided for unary/synchronous-style callers.

## What Changes
- Introduce `docketCanons` async generator and associated types (`DocketCanonsContext`, `DocketCanonsOptions`, `DocketCanonsResult`, `DocketCanonsTelemetry`, `DocketCanonsEvent`) in `@canon-clerk/core`.
- Provide `collectDocketCanons` convenience helper draining the generator and returning `Promise<DocketCanonsResult>`.
- Ingest PR metadata (title, body, branch name, linked issues) and `FileArtifact` unified diffs.
- Enforce a configurable byte budget (`maxDiffBytes`, default 100KB) with graceful per-file truncation diagnostics when diffs exceed budget.
- Dynamically build a strict Zod schema enforcing every candidate canon path as a required key with reason-first structured output (`colorabilitySummary` followed by `colorabilityScore`).
- Stream reasoning thoughts via `yield { type: 'thought', delta }` using `client.streamStructured` when available.
- Fall back gracefully to unary `client.generateStructured` / `generateStructuredJson` when streaming is unavailable, directly yielding the `finish` event.
- Short-circuit to an immediate `finish` event (`{ type: 'finish', assessments: {}, telemetry: { durationMs: 0 } }`) when `canons` is empty without invoking `ModelClient` (0 tokens).
- Export types and functions from `@canon-clerk/core` public barrel (`packages/core/src/index.ts`).

## Capabilities
### New Capabilities
- `docket-canons`: Evaluates candidate canons in aggregate against high-level PR context and diffs to determine subject-matter jurisdiction via schema-constrained screening, streaming reasoning thoughts and collecting diagnostic and latency telemetry.

### Modified Capabilities
None.

## Impact
- Core Domain: `@canon-clerk/core` exports `docketCanons`, `collectDocketCanons`, `DocketCanonsContext`, `DocketCanonsOptions`, `DocketCanonsResult`, `DocketCanonsTelemetry`, and `DocketCanonsEvent`.
- Consumers: Enables Phase 2 runners and CLI commands (`docket-canons` and `audit` cascade) to stream live progress indicators and filter candidate canons prior to micro-exhibit assembly with rich telemetry attribution.
- Dependencies: Consumes `ModelClient` and `FileArtifact` / `ColorabilityAssessment` primitives already present in `@canon-clerk/core`. No new runtime dependencies.
