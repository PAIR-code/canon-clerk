# Proposal

## Why
Per `docs/architecture/overview.md`, `@canon-clerk/schema` serves as the zero-dependency canonical home for all AST and cumulative `Caseload` state types, while `@canon-clerk/core` acts as the execution engine. Currently, data plane interfaces (`FileArtifact`, `ColorabilityAssessment`, and associated status enums) reside in `@canon-clerk/core`, and the root `Caseload` interface is not yet defined in TypeScript.

Per `.canons/git-workflow/precursor-domain-refactors-must-ship-independently.md`, foundational domain model adjustments must ship independently of presentation or pipeline node implementations to preserve atomic review units and bisectability. Relocating these interfaces to `@canon-clerk/schema` and codifying the baseline `Caseload` and `CaseloadIntake` types establishes a solid schema foundation ahead of the `intake` engine and CLI implementation.

## What Changes
- Relocate pure data plane type definitions (`FileArtifact`, `FileChangeStatus`, `PatchOmissionReason`, `ContentOmissionReason`, `ColorabilityAssessment`, `AssessmentProvenance`, `MissingCanonPolicy`, `DuplicateCanonPolicy`) from `@canon-clerk/core` to `@canon-clerk/schema`.
- Define canonical `Caseload`, `CaseloadIntake`, and `LinkedIssueContext` interfaces in `@canon-clerk/schema`.
- Re-export all relocated data plane and Caseload types from `@canon-clerk/core` to ensure zero breaking changes for existing consumers and test suites.
- Retain runtime validation factory functions (`createFileArtifact`, `createColorabilityAssessment`) in `@canon-clerk/core`.
- Update the living specification for `cascade-data-plane` to record the schema package location and Caseload state container contracts.

## Capabilities
### Modified Capabilities
- `cascade-data-plane`: Expands the data plane specification to encompass the canonical `Caseload` state container, `CaseloadIntake` contract, and defines `@canon-clerk/schema` as the zero-dependency home for data plane interfaces.

## Impact
- **Affected Packages:** `@canon-clerk/schema`, `@canon-clerk/core`.
- **Dependencies:** Zero new runtime dependencies introduced.
- **Consumers:** Existing consumers of `FileArtifact` and `createFileArtifact` from `@canon-clerk/core` remain unaffected due to backward-compatible re-exports.
