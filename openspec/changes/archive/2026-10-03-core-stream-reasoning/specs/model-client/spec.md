# Spec Delta: model-client

## ADDED Requirements

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
