# Tasks

## 1. Schema Types Definition
- [ ] 1.1 Define `FileArtifact`, `FileChangeStatus`, `PatchOmissionReason`, `ContentOmissionReason`, `ColorabilityAssessment`, `AssessmentProvenance`, `MissingCanonPolicy`, and `DuplicateCanonPolicy` in `packages/schema/src/types/artifact.ts`
- [ ] 1.2 Define `Caseload`, `CaseloadIntake`, and `LinkedIssueContext` in `packages/schema/src/types/caseload.ts`
- [ ] 1.3 Export new types through `packages/schema/src/types/index.ts` and `packages/schema/src/index.ts`

## 2. Core Re-exports and Reconciliation
- [ ] 2.1 Update `packages/core/src/artifact.ts` to import and re-export data plane types from `@canon-clerk/schema`
- [ ] 2.2 Re-export `Caseload` and `CaseloadIntake` through `packages/core/src/index.ts`
- [ ] 2.3 Verify `packages/core/src/artifact.test.ts` and ensure all tests continue passing

## 3. Monorepo Verification
- [ ] 3.1 Run full typecheck and build across monorepo packages (`npm run build`, `npm run typecheck`)
- [ ] 3.2 Run complete test suite (`npm test`) and check pipeline (`npm run check`)
