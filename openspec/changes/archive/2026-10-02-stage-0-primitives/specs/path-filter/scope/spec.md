# path-filter/scope Specification

## Purpose

Defines hierarchical scope containment evaluation in `@canon-clerk/core`, determining whether a target file resides within a canon's monorepo package or repository scope boundary at zero I/O cost without ambient environment dependencies.

## ADDED Requirements

### Requirement: Workspace Boundary Verification
The system SHALL verify that both the target file path and the canon file path reside within the declared workspace root directory, rejecting any escaping paths with a descriptive mismatch reason.

#### Scenario: Target path escapes workspace root
- **WHEN** evaluating a target path that resolves outside the workspace root
- **THEN** returns inScope false with reason TARGET_OUTSIDE_WORKSPACE

#### Scenario: Canon path escapes workspace root
- **WHEN** evaluating a canon path that resolves outside the workspace root
- **THEN** returns inScope false with reason CANON_OUTSIDE_WORKSPACE

### Requirement: Discovery Glob and Ignore Validation
The system SHALL validate that the canon path matches active canon discovery globs and is not suppressed by active ignore patterns relative to the workspace root.

#### Scenario: Canon path matches active ignore patterns
- **WHEN** evaluating a canon path matching active ignore patterns
- **THEN** returns inScope false with reason CANON_IGNORED

#### Scenario: Canon path fails discovery globs
- **WHEN** evaluating a canon path that does not match configured discovery globs
- **THEN** returns inScope false with reason CANON_GLOB_MISMATCH

### Requirement: Hierarchical Scope Containment
The system SHALL evaluate scope containment by ascending directory ancestors from the target file toward the workspace root, matching against stripped local canon globs to locate the owning scope root.

#### Scenario: Target file inside scoped package
- **WHEN** evaluating a target file under a package directory owning a scoped canon
- **THEN** returns inScope true with the package scope path and relative coordinates

#### Scenario: Target file outside scoped package
- **WHEN** evaluating a target file in a sibling package or outside the scoped canon
- **THEN** returns inScope false with reason OUT_OF_SCOPE

#### Scenario: Target file matching root canon
- **WHEN** evaluating any target file under the workspace against a root canon
- **THEN** returns inScope true with scope root at the workspace root

### Requirement: Pure Environment-Agnostic Evaluation
The system SHALL evaluate scope containment as a pure function requiring an explicit workspace root without filesystem I/O or ambient environment dependencies.

#### Scenario: Evaluation with relative and absolute path variants
- **WHEN** evaluating targets and canons using relative or absolute path strings
- **THEN** normalizes paths against the explicit workspace root without reading the filesystem
