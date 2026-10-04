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
