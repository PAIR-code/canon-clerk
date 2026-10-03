# query-canons Specification

## Purpose

Defines the Phase 1: Check streaming evaluation engine in `@canon-clerk/core`, orchestrating bipartite discovery across target codebase files and canon definition rules to stream relational activation tuples with zero memory buffering, explicit workspace scoping, and lazy filesystem I/O.

## Requirements

### Requirement: Symmetrical Bipartite Discovery Configuration
The system SHALL accept independent discovery configurations for target files and canon definitions, supporting literal paths, wildcard globs, custom ignore patterns, and default noise directory exclusions across both domains.

#### Scenario: Flexible target and canon inputs
- **WHEN** configuring a query with literal paths, wildcard globs, or structured discovery options
- **THEN** normalizes both domains into concrete discovery options applying independent active ignore patterns

### Requirement: Prospective and Existing Target Resolution
The system SHALL resolve literal target paths directly against the workspace root without requiring filesystem existence to evaluate prospective files, while evaluating existing codebase files when target queries specify wildcards or directory paths.

#### Scenario: Prospective target file evaluation
- **WHEN** querying a non-existent target path passed as an explicit literal path
- **THEN** constructs target provenance directly without requiring the file to exist on disk

#### Scenario: Wildcard target file traversal
- **WHEN** querying targets using glob wildcards or directory targets
- **THEN** traverses the filesystem and collects matching existing files while pruning ignored directories

### Requirement: Zero-I/O Early Scope Screening
The system SHALL evaluate hierarchical scope containment prior to reading or parsing canon files from disk, immediately skipping out-of-scope target files without initiating filesystem I/O.

#### Scenario: Sibling canon scope rejection
- **WHEN** a candidate canon resides in a sibling package outside the target file's ancestor directory tree
- **THEN** discards the target evaluation without reading or parsing the canon Markdown document

### Requirement: Single-Pass Lazy Canon Loading
The system SHALL lazily read and parse candidate canon Markdown files only after at least one candidate target file is proven to reside within scope, caching parsed canon models so each file is read and parsed at most once per execution.

#### Scenario: Canon parser caching across multiple targets
- **WHEN** multiple target files activate the same in-scope canon
- **THEN** loads and parses the canon file from disk exactly once across the entire query run

### Requirement: Relational Tuple Streaming
The system SHALL stream relational match tuples bundling target provenance, canon provenance, relative scope path coordinates, and matched trigger patterns as atomic activations without buffering complete query results in memory.

#### Scenario: Streaming relational match output
- **WHEN** a target file matches the trigger patterns of an in-scope canon
- **THEN** immediately yields an atomic relational result tuple with complete file provenance and coordinates

### Requirement: Deterministic Lexical Ordering
The system SHALL traverse directory trees and process candidate targets in deterministic lexicographical order, guaranteeing consistent output sequences across identical filesystem configurations.

#### Scenario: Lexical emission stability
- **WHEN** evaluating multiple targets and discovering multiple candidate canons
- **THEN** processes targets in sorted order and traverses directories in sorted entry order

### Requirement: Workspace Containment Boundary
The system SHALL require an explicit workspace root and reject target or canon paths resolving outside the workspace boundary, operating without ambient process working directory coupling.

#### Scenario: Missing workspace root
- **WHEN** invoking query execution without an explicit workspace root directory
- **THEN** rejects the query invocation with a descriptive validation error

#### Scenario: Escaping target path
- **WHEN** a target path resolves outside the declared workspace root
- **THEN** throws an out-of-bounds error rejecting the escaping path
