# Design: File Artifact and Colorability Assessment Primitives

## Context

Phase 2 (Docket: #169, #170) and Phase 3 (Audit) of Canon Clerk require standardized, pure data structures to represent code modifications, persistent reference exhibits, and screening verdicts across the Three-Phase Evaluation Cascade.

In accordance with [`.canons/git-workflow/precursor-domain-refactors-must-ship-independently.md`](.canons/git-workflow/precursor-domain-refactors-must-ship-independently.md), these primitives are decoupled from downstream prompt engineering, token budgeting, and screening execution into a self-contained domain module in `@canon-clerk/core`.

## Goals / Non-Goals

**Goals:**
- Provide immutable TypeScript domain interfaces (`FileArtifact`, `ColorabilityAssessment`) and supporting enumerations.
- Implement a pure factory builder (`createFileArtifact`) enforcing POSIX path validity, non-negative line statistics, and omission rationale consistency.
- Support dual representation where a file artifact can hold both unified diff delta (`patch`) and full source text (`content`).
- Maintain zero runtime dependencies and zero environment I/O in the core data plane.

**Non-Goals:**
- Generating git diffs or reading workspace files from the filesystem (delegated to presentation adapters and loaders).
- Executing screening models or token budget truncation algorithms (handled in #169 and #170).

## Decisions

### 1. Salient Reason-First Ordering for ColorabilityAssessment
- **Decision:** Model screening assessments as `ColorabilityAssessment` featuring `colorabilitySummary: string` generated before `colorabilityScore: number`.
- **Rationale:** Choosing salient domain vocabulary drives model behavior during constrained structured decoding. Asking the model to produce `colorabilitySummary` directly answers the focal question *"How colorable is this exhibit/canon?"*, establishing prima facie jurisdiction in prose before emitting the scalar `colorabilityScore`. Emitting the summary first acts as an autoregressive chain-of-thought anchor for reproducible scoring.
- **Alternatives Considered:** Generic `applicabilityReason` / `applicabilityScore`. Rejected in favor of domain-grounded legal terminology that directly primes the screening model's attention.

### 2. Mandatory Omission Rationale Tracking
- **Decision:** When `patch` is undefined, `patchOmissionReason` must be populated. When `content` is undefined, `contentOmissionReason` must be populated.
- **Rationale:** Downstream evaluators must clearly distinguish why a file's delta or content is absent—e.g. intentionally skipped (`not_requested`), omitted to protect token budgets (`oversized`), excluded due to file encoding (`binary`), or deleted from disk (`deleted`).
- **Alternatives Considered:** Optional omission reasons. Rejected because absence of patch/content without explanation creates ambiguity in downstream audit logs and screening triage.

### 3. Pure Builder Validation (`createFileArtifact`)
- **Decision:** Implement `createFileArtifact(params)` as a pure function that validates relative POSIX path format (rejecting leading slashes and `..` traversal segments), non-negative line stats, and omission rationale invariants.
- **Rationale:** Centralizing invariant validation at the boundary prevents corrupted or malformed artifacts from reaching the evaluation pipeline.

### 4. Dual Representation for Modified Grounding Documents
- **Decision:** Allow both `patch` and `content` to be simultaneously populated on a single `FileArtifact`.
- **Rationale:** When a persistent reference document (e.g. `README.md`) is also modified in a PR, the auditor model requires both the delta focus and the full surrounding context to evaluate compliance without manual diff-patching.

## Risks / Trade-offs

- **Risk:** Callers must explicitly declare omission reasons when constructing artifacts.
- **Mitigation:** Standard string union values (`not_requested`, `unchanged`, etc.) make builder construction clear and self-documenting.
