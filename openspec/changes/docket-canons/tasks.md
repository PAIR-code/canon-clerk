# Tasks: Phase 2 Docket Canons Macro Triage

## 1. Core Interfaces & Event Contracts
- [ ] 1.1 Define `DocketCanonsContext`, `DocketCanonsOptions`, `DocketCanonsTelemetry`, `DocketCanonsResult`, and `DocketCanonsEvent` in `packages/core/src/docket-canons.ts`.
- [ ] 1.2 Re-export new types and functions (`docketCanons`, `collectDocketCanons`) from `packages/core/src/index.ts`.

## 2. Prompt Formatting & Diff Budgeting
- [ ] 2.1 Implement PR context and diff formatter enforcing `maxDiffBytes` (default 100KB).
- [ ] 2.2 Add graceful per-file diff truncation with diagnostic omission notices.
- [ ] 2.3 Implement candidate canon roster formatter presenting path, title, invariant, and rationale.

## 3. Schema Construction, Async Generator & Triage Execution
- [ ] 3.1 Implement dynamic Zod schema builder keyed by candidate canon paths with reason-first properties.
- [ ] 3.2 Implement `docketCanons` async generator with empty-canons short-circuit yielding an immediate `finish` event (0 tokens).
- [ ] 3.3 Implement streaming execution via `client.streamStructured`, yielding `{ type: 'thought', delta }` events and recording latency metrics (`timeToFirstThoughtMs`, `timeToFirstTokenMs`, `thoughtChunks`).
- [ ] 3.4 Implement graceful unary fallback via `client.generateStructured` / `generateStructuredJson`, yielding a single `finish` event.
- [ ] 3.5 Implement `collectDocketCanons` helper function draining the generator to return `Promise<DocketCanonsResult>`.

## 4. Hermetic Testing & Verification
- [ ] 4.1 Unit tests for empty canons array short-circuit yielding immediate `finish` without model invocation.
- [ ] 4.2 Unit tests for streaming execution: iterating thoughts, chunk counting, `timeToFirstThoughtMs`, and `timeToFirstTokenMs` with `createMockModelClient`.
- [ ] 4.3 Unit tests for graceful fallback when client lacks `streamStructured`.
- [ ] 4.4 Unit tests for `collectDocketCanons` helper resolving final result.
- [ ] 4.5 Unit tests for diff truncation when exceeding `maxDiffBytes`.
- [ ] 4.6 Unit tests for `AbortSignal` cancellation propagation across streaming and unary flows.
- [ ] 4.7 Verify repository health with `npm run check`.
