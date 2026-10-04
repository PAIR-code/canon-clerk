# model-client Specification

## Purpose

Defines the core model client abstraction, structured JSON generation contract, provider adapters, and deterministic offline test fixtures for Canon Clerk's evaluation cascade.

## Requirements

### Requirement: Structured JSON Generation Contract
The system SHALL define a `ModelClient` interface providing schema-constrained JSON generation (`generateStructuredJson`) accepting a prompt, optional system instruction, target schema, sampling temperature, and cancellation signal.

#### Scenario: Submitting structured generation request
- **WHEN** invoking `generateStructuredJson` with a prompt and schema
- **THEN** returns a typed, parsed payload conforming to the provided schema

#### Scenario: Propagating cancellation signal
- **WHEN** a structured generation request includes an aborted signal
- **THEN** execution terminates immediately and rejects with an abort error

### Requirement: Production Google Provider Adapter
The system SHALL provide a `GoogleModelClient` adapter wrapping the Vercel AI SDK and Google Generative AI provider, loading SDK modules dynamically to protect Phase 1 CLI startup latency.

#### Scenario: Enforcing explicit credentials
- **WHEN** initializing `GoogleModelClient` without an API key and executing a generation request
- **THEN** rejects with an actionable error directing the developer to configuration options

#### Scenario: Dynamic SDK module loading
- **WHEN** instantiating `GoogleModelClient`
- **THEN** heavy SDK dependencies are deferred until generation execution

### Requirement: Model Client Factory
The system SHALL provide a `createModelClient` factory creating the appropriate provider adapter from a resolved `ModelConfig`.

#### Scenario: Instantiating Google provider client
- **WHEN** invoking `createModelClient` with a configuration specifying provider `"google"`
- **THEN** returns an instance of `GoogleModelClient` configured with the specified parameters

#### Scenario: Rejecting unsupported providers
- **WHEN** invoking `createModelClient` with an unrecognized provider identifier
- **THEN** throws an error identifying the unsupported provider

### Requirement: Deterministic Offline Mock Double
The system SHALL provide a `createMockModelClient` test fixture allowing offline unit tests to simulate model responses synchronously or asynchronously with schema validation and request inspection.

#### Scenario: Capturing requests and validating schema
- **WHEN** a mock client processes a generation request
- **THEN** records the request details and validates the mock payload against the requested schema

### Requirement: Streaming Structured Events Contract
The system SHALL define a `streamStructured` asynchronous generator method on `ModelClient` returning a stream of typed `ModelStreamEvent` items (`thought`, `text-delta`, `finish`). The `finish` event SHALL yield a schema-validated output payload, resolved model identifier, and token usage metrics.

#### Scenario: Streaming reasoning thoughts and final structured payload
- **WHEN** invoking `streamStructured` with a valid prompt, schema, and reasoning model
- **THEN** yields reasoning thought events followed by a finish event containing the validated payload and duration

#### Scenario: Enforcing schema validation on stream finish
- **WHEN** the model produces a completion that does not satisfy the request's Zod schema
- **THEN** the stream throws a validation error before yielding a finish event

#### Scenario: Aborting stream via AbortSignal
- **WHEN** an active stream's cancellation signal is aborted or the consumer breaks out of iteration
- **THEN** the underlying generation stream is aborted immediately without orphaned requests

### Requirement: Streaming Mock Double Fixture
The system SHALL enhance `createMockModelClient` to support streaming generation with configurable synthetic thoughts and async chunking for deterministic offline testing.

#### Scenario: Emitting simulated thoughts in mock client
- **WHEN** invoking `streamStructured` on a mock client configured with mock thoughts
- **THEN** yields the mock thought events followed by the mock schema-validated finish payload

#### Scenario: Aborting mock stream on cancelled signal
- **WHEN** `streamStructured` is invoked with an aborted signal or cancelled mid-stream
- **THEN** the mock stream terminates immediately with an abort error

### Requirement: Live Connectivity Smoke Test Harness
The system SHALL provide a dedicated cross-package live integration test harness that resolves ambient credentials via configuration, instantiates the model client, and issues a minimal structured smoke query (`{ ok: boolean }`) against the live endpoint. The harness SHALL fail by default with actionable remediation if credentials are missing, support explicit bypass via `CANON_CLERK_SKIP_LIVE_TESTS=1`, and execute outside routine hermetic CI checks.

#### Scenario: End-to-end smoke verification with live endpoint
- **WHEN** running the live integration harness with valid credentials and network reachability
- **THEN** returns a validated `{ ok: true }` structured response from the live model

#### Scenario: Failing by default with actionable remediation on missing credentials
- **WHEN** running the live integration harness without ambient credentials or API key
- **THEN** fails immediately with diagnostic instructions referencing configuration paths and key setup

#### Scenario: Bypassing live integration execution
- **WHEN** running the test harness with `CANON_CLERK_SKIP_LIVE_TESTS=1`
- **THEN** the live integration suite cleanly skips execution without failing


