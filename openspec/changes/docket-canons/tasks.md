# Tasks: Phase 2 Docket Canons Macro Triage

## 1. Core Interfaces & Event Contracts
- [x] 1.1 Define `DocketCanonsContext`, `DocketCanonsOptions`, `DocketCanonsTelemetry`, `DocketCanonsResult`, and `DocketCanonsEvent` in `packages/core/src/docket-canons.ts`.
- [x] 1.2 Re-export new types and functions (`docketCanons`, `collectDocketCanons`) from `packages/core/src/index.ts`.

## 2. Prompt Formatting & Diff Budgeting
- [x] 2.1 Implement PR context and diff formatter enforcing `maxDiffBytes` (default 100KB).
- [x] 2.2 Add graceful per-file diff truncation with diagnostic omission notices.
- [x] 2.3 Implement candidate canon roster formatter presenting path, title, invariant, and rationale.

## 3. Schema Construction, Async Generator & Triage Execution
- [x] 3.1 Implement dynamic Zod schema builder keyed by candidate canon paths with reason-first properties.
- [x] 3.2 Implement `docketCanons` async generator with empty-canons short-circuit yielding an immediate `finish` event (0 tokens).
- [x] 3.3 Implement streaming execution via `client.streamStructured`, yielding `{ type: 'thought', delta }` events and recording latency metrics (`timeToFirstThoughtMs`, `timeToFirstTokenMs`, `thoughtChunks`).
- [x] 3.4 Implement graceful unary fallback via `client.generateStructured` / `generateStructuredJson`, yielding a single `finish` event.
- [x] 3.5 Implement `collectDocketCanons` helper function draining the generator to return `Promise<DocketCanonsResult>`.

## 4. Hermetic Testing & Verification
- [x] 4.1 Unit tests for empty canons array short-circuit yielding immediate `finish` without model invocation.
- [x] 4.2 Unit tests for streaming execution: iterating thoughts, chunk counting, `timeToFirstThoughtMs`, and `timeToFirstTokenMs` with `createMockModelClient`.
- [x] 4.3 Unit tests for graceful fallback when client lacks `streamStructured`.
- [x] 4.4 Unit tests for `collectDocketCanons` helper resolving final result.
- [x] 4.5 Unit tests for diff truncation when exceeding `maxDiffBytes`.
- [x] 4.6 Unit tests for `AbortSignal` cancellation propagation across streaming and unary flows.
- [x] 4.7 Verify repository health with `npm run check`.

## 5. Responsible Aggregation & Schema Constraint Refinement
- [x] 5.1 Enforce linear FST enum trie constraint in `createDocketCanonsSchema` preventing hallucinated keys.
- [x] 5.2 Implement `normalizeAssessments` and `DocketAnomaliesManifest` tracking dropped, duplicate, and unrecognized canons.
- [x] 5.3 Implement `ColorabilityAssessment` discriminated union with `provenance` ('result' | 'missing' | 'duplicate') and `policy`.
- [x] 5.4 Support `missingCanonPolicy` ('escalate' default vs 'exclude') and `duplicateCanonPolicy` ('highest' default vs 'first' | 'last').
- [x] 5.5 Unit and integration tests verifying anomalies manifest, policy resolutions, and provenance tracking.
