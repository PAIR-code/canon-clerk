# Design

## Context
Canon Clerk's architecture (specified in `docs/architecture/overview.md`) establishes a clean separation of concerns: `@canon-clerk/schema` provides zero-dependency canonical domain models and ASTs, whereas `@canon-clerk/core` implements functional evaluation logic and DAG scheduling.

Currently, data plane primitives (`FileArtifact`, `ColorabilityAssessment`, and omission rationale types) reside in `packages/core/src/artifact.ts`. When designing the upcoming `intake` stage (and downstream DAG nodes `discover`, `validate`, `docket`, `admit`, `audit`), every stage enriches a shared `Caseload` record. If `Caseload` is defined in `@canon-clerk/schema`, it needs `FileArtifact` for `CaseloadIntake.diffs`. Furthermore, third-party consumers, reporting tools, or test harnesses should be able to inspect and parse `Caseload` records without pulling in `@canon-clerk/core` runtime execution logic.

## Goals / Non-Goals
**Goals:**
- Move pure data plane types (`FileArtifact`, `ColorabilityAssessment`, and related enums/types) from `@canon-clerk/core` to `@canon-clerk/schema`.
- Codify the canonical `Caseload` and `CaseloadIntake` (and `LinkedIssueContext`) interfaces in `@canon-clerk/schema`.
- Maintain strict backward compatibility for existing callers of `@canon-clerk/core` by re-exporting relocated types.
- Keep validation factories (`createFileArtifact`, `createColorabilityAssessment`) in `@canon-clerk/core`.

**Non-Goals:**
- Implementing runtime diff parsing or `executeIntake` (reserved for the subsequent `feat(core,cli): intake` change).
- Implementing CLI commands or presentations.
- Modifying other package dependencies.

## Decisions

### 1. Colocate Data Plane Types in `@canon-clerk/schema`
- **Decision:** Place `FileArtifact`, `FileChangeStatus`, `PatchOmissionReason`, `ContentOmissionReason`, `ColorabilityAssessment`, `AssessmentProvenance`, `MissingCanonPolicy`, and `DuplicateCanonPolicy` in `packages/schema/src/types/artifact.ts`.
- **Rationale:** These interfaces represent pure immutable data structures without external runtime dependencies. Housing them in `@canon-clerk/schema` enables `Caseload` to reference them directly and adheres to the hexagonal architecture where `@canon-clerk/schema` is the universal data contract.
- **Alternatives Considered:**
  - *Keep FileArtifact in core and define Caseload in core:* Breaks the architectural principle that `@canon-clerk/schema` defines all cumulative data models across the DAG.
  - *Keep FileArtifact in core and have schema depend on core:* Circular or inverted dependency, violating package hierarchy (`schema` has 0 dependencies).

### 2. Codify `Caseload` and Stage Delta Interfaces in `@canon-clerk/schema`
- **Decision:** Introduce `packages/schema/src/types/caseload.ts` defining `Caseload`, `CaseloadIntake`, and `LinkedIssueContext`, with optional fields for downstream stages (`discovery`, `validation`, `config`, `probe`, `docket`, `evidence`, `verdict`).
- **Rationale:** Establishes the cumulative spine of the DAG in TypeScript before implementing individual stages. Allows each subsequent stage to attach its typed delta without altering schema definitions.

### 3. Retain Validation Factories in `@canon-clerk/core`
- **Decision:** Retain `createFileArtifact` and `createColorabilityAssessment` in `packages/core/src/artifact.ts`.
- **Rationale:** `@canon-clerk/schema` focuses strictly on type contracts, parsing, and AST derivation. Runtime constraint checking, sanitization, and entity factory routines belong in the core domain engine.

## Risks / Trade-offs
- **Import churn risk:** Existing internal code importing `FileArtifact` from `@canon-clerk/core` could break if exports are shifted.
  - *Mitigation:* `@canon-clerk/core` explicitly re-exports all types from `@canon-clerk/schema`, ensuring 100% backward compatibility for all existing imports.
