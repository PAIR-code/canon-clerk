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

### Requirement: Pure In-Memory and Streaming Unified Diff Parsing
The system SHALL provide unified diff parsers that convert Git patch streams into `FileArtifact` domain entities, supporting both synchronous in-memory collection and asynchronous stream generation.

#### Scenario: Streaming file artifacts across chunk boundaries
- **WHEN** consuming an asynchronous chunked stream containing multiple file diffs
- **THEN** yields each `FileArtifact` incrementally as file demarcation boundaries or stream end are encountered

#### Scenario: Parsing added file in unified diff
- **WHEN** parsing a diff stream containing a new file creation with hunk lines
- **THEN** produces an artifact with status `'added'`, computed added line statistics, patch content, and default content omission reason

#### Scenario: Parsing deleted file in unified diff
- **WHEN** parsing a diff stream containing a deleted file with hunk lines
- **THEN** produces an artifact with status `'deleted'`, computed deleted line statistics, patch content, and deleted content omission reason

#### Scenario: Parsing modified file in unified diff
- **WHEN** parsing a diff stream containing edits to an existing file
- **THEN** produces an artifact with status `'modified'`, accurate added and deleted line counts, and hunk patch text

#### Scenario: Parsing renamed file without hunks
- **WHEN** parsing a diff stream containing a 100% similarity rename without hunks
- **THEN** produces an artifact with status `'renamed'`, previous path populated, undefined patch, and patch omission reason set to `'unchanged'`

#### Scenario: Parsing renamed file with modifications
- **WHEN** parsing a diff stream containing a rename with hunk modifications
- **THEN** produces an artifact with status `'renamed'`, previous path populated, and hunk patch text populated

#### Scenario: Parsing copied file in unified diff
- **WHEN** parsing a diff stream containing a copy directive
- **THEN** produces an artifact with status `'copied'`, original source path recorded in previous path, and patch text

#### Scenario: Parsing binary file modification
- **WHEN** parsing a diff stream containing binary file markers
- **THEN** produces an artifact with zero line counts and both patch and content omission reasons set to `'binary'`

#### Scenario: Parsing single-line hunk coordinates
- **WHEN** parsing hunk headers omitting line count coordinates
- **THEN** accurately parses coordinates and counts hunk line changes

#### Scenario: Parsing quoted file paths with whitespace
- **WHEN** parsing Git diff headers containing quoted paths with spaces or escaped characters
- **THEN** unquotes and unescapes the file paths into standard relative POSIX paths

#### Scenario: Parsing empty or whitespace-only diff streams
- **WHEN** parsing a diff stream that is empty or consists solely of whitespace
- **THEN** returns an empty file artifact map or terminates stream without emissions

