# docket-canons Specification

## Purpose
Defines the Step 1 Macro Triage screening capability (`docketCanons`) in `@canon-clerk/core`, evaluating candidate canons in aggregate against pull request context and file diffs using an async generator yielding reasoning thought events and concluding with a finish event carrying colorability assessments, an anomalies manifest, and telemetry.

## Requirements

### Requirement: Aggregate Canon Screening Stream Contract
The system SHALL provide an async generator `docketCanons` evaluating candidate canons against PR context and diffs, yielding `DocketCanonsEvent` stream items ending with a `finish` event containing `assessments` and `telemetry`.

#### Scenario: Streaming candidate canon evaluation
- **WHEN** invoking `docketCanons` with candidate canons and PR diff context
- **THEN** returns an async generator whose final yielded event is `{ type: 'finish' }` containing assessments and telemetry

### Requirement: Zero-Canons Short-Circuit
The system SHALL immediately yield a `finish` event with an empty assessment map and zero duration without calling `ModelClient` when candidate canons array is empty.

#### Scenario: Empty candidate canons list
- **WHEN** invoking `docketCanons` with an empty array of canons
- **THEN** immediately yields `{ type: 'finish', assessments: {}, telemetry: { durationMs: 0 } }`

### Requirement: Dynamic Strict Schema and Trie Constraint Enforcement
The system SHALL dynamically construct a structured schema utilizing an array of assessment items with an enum trie constraint restricting `canonPath` to candidate canon identifiers, preserving linear FST complexity and preventing hallucinated keys during constrained decoding.

#### Scenario: Schema guarantees candidate path integrity
- **WHEN** constructing the screening schema for candidate canons
- **THEN** generates an array schema where `canonPath` is constrained by a string enum (or literal) of the candidate canon keys

### Requirement: Responsible Aggregation and Anomalies Manifest
The system SHALL provide an anomalies manifest (`DocketAnomaliesManifest`) tracking omitted, duplicate, or unrecognized entries, and SHALL record `provenance` ('result' | 'missing' | 'duplicate') and `policy` on each `ColorabilityAssessment`.

#### Scenario: Missing canon escalation policy
- **WHEN** the screener model omits a candidate canon from its output
- **THEN** applies `missingCanonPolicy` (defaulting to 'escalate' with score 1.0 and a path-woven summary) and records the omitted path in `anomalies.missing`

#### Scenario: Duplicate canon resolution policy
- **WHEN** the screener model emits multiple assessments for the same canon
- **THEN** applies `duplicateCanonPolicy` (defaulting to 'highest' score) and records all duplicate occurrences in `anomalies.duplicates`

### Requirement: Diff Budgeting and Graceful Truncation
The system SHALL enforce a configurable diff budget (`maxDiffBytes`, defaulting to 100KB), gracefully truncating diff content that exceeds the budget while preserving file paths and line delta statistics.

#### Scenario: Diff within budget
- **WHEN** combined diffs are within `maxDiffBytes`
- **THEN** includes complete patch contents in prompt context

#### Scenario: Diff exceeding budget
- **WHEN** diffs exceed `maxDiffBytes`
- **THEN** truncates patch content with a diagnostic notice while retaining line stats and file paths

### Requirement: Thought Streaming and Metric Collection
The system SHALL invoke `client.streamStructured` when available, yielding `{ type: 'thought', delta }` events as thoughts arrive and recording `timeToFirstThoughtMs`, `timeToFirstTokenMs`, and `thoughtChunks` in the final `finish` event's `telemetry`.

#### Scenario: Streaming model execution with thoughts
- **WHEN** `docketCanons` runs with a client supporting `streamStructured`
- **THEN** yields `thought` events and records thought timing and chunk count in `telemetry`

### Requirement: Unary Fallback Execution
The system SHALL fall back to unary model generation (`generateStructured` or `generateStructuredJson`) when `streamStructured` is not supported on the client, directly yielding the `finish` event with duration and model metadata.

#### Scenario: Client lacking streaming capability
- **WHEN** `docketCanons` runs with a client that does not provide `streamStructured`
- **THEN** executes unary generation and yields a single `finish` event with available telemetry

### Requirement: Convenience Drain Helper
The system SHALL provide a `collectDocketCanons` helper function that drains the `docketCanons` async generator and resolves to the final `DocketCanonsResult`.

#### Scenario: Draining generator to final result
- **WHEN** invoking `collectDocketCanons`
- **THEN** returns a Promise resolving to `{ assessments, telemetry }` from the final finish event

### Requirement: Abort Signal Propagation
The system SHALL accept an optional `AbortSignal` in options and propagate cancellation to the underlying `ModelClient`.

#### Scenario: Aborting screening operation
- **WHEN** the supplied `AbortSignal` is aborted prior to or during evaluation
- **THEN** propagates the abort and rejects with an AbortError
