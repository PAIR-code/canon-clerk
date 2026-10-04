# Proposal: File Artifact and Colorability Assessment Primitives

## Why

Both the Docket Phase (Phase 2: #169, #170) and Audit Phase (Phase 3) of Canon Clerk's Three-Phase Evaluation Cascade require consistent representations for:
1. **Code Changes & Grounding Documents:** Representing unified diffs, full file contents, and line modification deltas uniformly without duplicating code or creating cognitive divergence.
2. **Relevance Screening Triage:** Structuring the reason-first colorability tuples (`colorabilitySummary` + `colorabilityScore`) emitted by lightweight screening models to determine threshold docket jurisdiction.

In accordance with [`.canons/git-workflow/precursor-domain-refactors-must-ship-independently.md`](.canons/git-workflow/precursor-domain-refactors-must-ship-independently.md), this change extracts these pure data structures into a self-contained, testable precursor in `@canon-clerk/core` with zero external dependencies, zero API keys, and 100% deterministic unit tests.

## What Changes

1. **File Artifact Domain Types:**
   - Define `FileArtifact`, `FileChangeStatus`, `PatchOmissionReason`, and `ContentOmissionReason` in `packages/core/src/artifact.ts`.
2. **Factory & Invariant Validation:**
   - Implement `createFileArtifact(params)` validating POSIX relative paths, non-negative line statistics, and explicit omission rationale constraints.
3. **Colorability Assessment Representation:**
   - Define `ColorabilityAssessment` in `packages/core/src/artifact.ts` implementing the reason-first screening schema (`colorabilitySummary` and `colorabilityScore`).
4. **API Re-exports:**
   - Export artifact and colorability primitives from `packages/core/src/index.ts`.
5. **Unit Test Suite:**
   - Implement comprehensive unit tests in `packages/core/src/artifact.test.ts` covering path validation, omission tracking, status combinations, builder assertions, and colorability assessment contracts.

## Capabilities

### New Capabilities
- `cascade-data-plane`: Shared data primitives for file artifacts (diff deltas and reference content) with explicit omission tracking and reason-first colorability assessments.

### Modified Capabilities
*(None)*

## Impact

- **Core Engine:** `@canon-clerk/core` provides pure data structures and builders with zero external dependencies.
- **Downstream Consumers:** Unblocks Phase 2 cascade subroutines (`docketCanons` in #169 and `docketCanonTargets` in #170).
