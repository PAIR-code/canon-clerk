# Tasks: Stream Reasoning Thoughts and Structured Events

## 1. Type Definitions & Core Contracts
- [x] 1.1 Define `ModelStreamEvent<T>` and `ModelUsage` interfaces in `packages/core/src/client.ts`
- [x] 1.2 Add `streamStructured?` method signature to `ModelClient` interface in `packages/core/src/client.ts`
- [x] 1.3 Export streaming types from `packages/core/src/index.ts`

## 2. GoogleModelClient Streaming Adapter
- [x] 2.1 Implement `streamStructured` in `GoogleModelClient` using `streamText` and `fullStream`
- [x] 2.2 Wire reasoning chunks to `type: 'thought'` and text deltas to `type: 'text-delta'`
- [x] 2.3 Extract model version and token usage metadata on finish
- [x] 2.4 Validate accumulated JSON payload against request Zod schema before yielding `type: 'finish'`
- [x] 2.5 Propagate `AbortSignal` and generator termination cleanly

## 3. Mock Model Client & Test Fixtures
- [x] 3.1 Extend `createMockModelClient` to support simulated streaming with `mockThoughts` and async delays
- [x] 3.2 Add unit tests for `createMockModelClient.streamStructured` verifying thought order, finish event, and abort handling
- [x] 3.3 Add unit tests for `GoogleModelClient.streamStructured` credential checks and abort handling

## 4. OpenSpec Specifications & Verification
- [x] 4.1 Update living specification in `openspec/specs/model-client/spec.md`
- [x] 4.2 Validate OpenSpec change with `npm run opsx -- validate core-stream-reasoning --strict`
- [x] 4.3 Run full repository build and test suite (`npm run check`)
