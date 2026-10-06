# cascade-data-plane Specification

## Purpose

Defines the shared data plane abstractions (`FileArtifact`, `ColorabilityAssessment`, and `Caseload` state container) used across Canon Clerk's evaluation cascade to represent code modifications, persistent reference files, screening colorability verdicts, and cumulative evaluation state.

## Requirements

### Requirement: File Artifact Domain Abstraction
The system SHALL define the `FileArtifact` interface in `@canon-clerk/schema` representing unified diffs, full file contents, line change counts, and file status across active changes and reference documents.

#### Scenario: Populating unified diff for modified change
- **WHEN** creating a `FileArtifact` representing a modified source file with patch content
- **THEN** the artifact records the path, status `'modified'`, line statistics, and patch text with undefined patch omission reason

#### Scenario: Populating full content for persistent reference document
- **WHEN** creating a `FileArtifact` representing a persistent reference document
- **THEN** the artifact records full UTF-8 content with undefined content omission reason

#### Scenario: Dual representation for modified reference document
- **WHEN** creating a `FileArtifact` for a reference document that was also modified in the change
- **THEN** the artifact simultaneously populates both patch and content

### Requirement: Explicit Omission Rationale Tracking
The system SHALL enforce that if `patch` is undefined, an explicit `patchOmissionReason` (`'unchanged'`, `'binary'`, `'oversized'`, `'not_requested'`) is provided, and if `content` is undefined, an explicit `contentOmissionReason` (`'not_requested'`, `'binary'`, `'oversized'`, `'deleted'`) is provided.

#### Scenario: Tracking patch omission rationale
- **WHEN** creating a file artifact where patch is not provided
- **THEN** the artifact includes an explicit `patchOmissionReason`

#### Scenario: Tracking content omission rationale
- **WHEN** creating a file artifact where full content is not provided
- **THEN** the artifact includes an explicit `contentOmissionReason`

### Requirement: File Artifact Factory and Constraint Validation
The system SHALL provide a `createFileArtifact` builder that validates relative POSIX path formatting, non-negative line statistics, and omission rationale consistency, throwing descriptive validation errors on invalid inputs.

#### Scenario: Validating POSIX relative path
- **WHEN** invoking `createFileArtifact` with an absolute path or path traversal segment
- **THEN** throws a validation error identifying invalid path formatting

#### Scenario: Enforcing omission rationale consistency
- **WHEN** invoking `createFileArtifact` without a patch and without a `patchOmissionReason`
- **THEN** throws a validation error requiring an omission reason

#### Scenario: Enforcing non-negative line statistics
- **WHEN** invoking `createFileArtifact` with negative line statistics
- **THEN** throws a validation error rejecting negative counts

### Requirement: Reason-First Colorability Assessment Contract
The system SHALL define a `ColorabilityAssessment` interface structuring screening triage verdicts with a `colorabilitySummary` justification generated prior to a normalized `colorabilityScore` bounded in `[0.0, 1.0]`.

#### Scenario: Representing active docket jurisdiction
- **WHEN** a screening model evaluates an exhibit against a canon and determines jurisdiction
- **THEN** the assessment contains a non-empty `colorabilitySummary` and a `colorabilityScore` between 0.0 and 1.0

### Requirement: Baseline Caseload State Container Abstraction
The system SHALL define the canonical `Caseload` state container in `@canon-clerk/schema` with required schema specification version `'1.0'` and optional stage properties (`intake`, `discovery`, `validation`, `config`, `probe`, `docket`, `evidence`, `verdict`).

#### Scenario: Instantiating a baseline Caseload
- **WHEN** initializing a new evaluation run state container
- **THEN** the Caseload has version `'1.0'` and undefined evaluation stage properties

#### Scenario: Retaining completed stage properties
- **WHEN** a Caseload enriches state across pipeline stages
- **THEN** existing stage properties are preserved immutably

### Requirement: Filing Intake Caseload State Contract
The system SHALL define the `CaseloadIntake` interface in `@canon-clerk/schema` recording `diffs` (keyed by relative POSIX paths to `FileArtifact`), optional target discovery paths (`targetPaths`), scope (`'targeted' | 'all-targets'`), PR metadata (`pr_title`, `pr_body`), and optional linked issue contexts.

#### Scenario: Representing targeted diff intake state
- **WHEN** populating intake state from a pull request diff
- **THEN** the intake state records keyed FileArtifact diffs, PR title, body, and target scope

#### Scenario: Representing explicit target path directives
- **WHEN** populating intake state from explicit path directives without diffs
- **THEN** the intake state records target paths and empty diffs map

### Requirement: Schema Re-export from Core Engine
The system SHALL re-export all data plane and Caseload interfaces from `@canon-clerk/core`, preserving backward-compatible import surfaces while maintaining runtime validation factories within the core engine.

#### Scenario: Importing FileArtifact from core
- **WHEN** consumer code imports `FileArtifact` or `ColorabilityAssessment` from `@canon-clerk/core`
- **THEN** the types resolve identically to their `@canon-clerk/schema` definitions
