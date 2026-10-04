# Tasks

## 1. Data Plane Types and Builder
- [x] 1.1 Define `FileArtifact`, `FileChangeStatus`, `PatchOmissionReason`, and `ContentOmissionReason` in `packages/core/src/artifact.ts`
- [x] 1.2 Implement `createFileArtifact(params)` builder with constraint validation in `packages/core/src/artifact.ts`
- [x] 1.3 Define `ColorabilityAssessment` in `packages/core/src/artifact.ts`
- [x] 1.4 Re-export data primitives and builder from `packages/core/src/index.ts`

## 2. Testing and Monorepo Verification
- [x] 2.1 Implement comprehensive unit tests in `packages/core/src/artifact.test.ts` verifying path validation, omission tracking, line counts, and colorability assessment
- [x] 2.2 Verify full monorepo typecheck, lint, and test suites via `npm run check`
