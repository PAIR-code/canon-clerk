# Spec Delta: cli/check-config

## ADDED Requirements

### Requirement: Active Model Endpoint Probing
When `--probe` is passed, the `check-config` command SHALL actively exercise the production model generation API for configured tiers to verify live inference authorization, quota availability, and service reachability. The probe SHALL NOT rely on shallow metadata queries, unauthenticated pings, or simulated health endpoints. The command SHALL enforce a bounded timeout, defaulting to 15,000ms, overridable via `--probe-timeout <ms>` or `CANON_CLERK_PROBE_TIMEOUT_MS`.

#### Scenario: Successfully probing reachable model endpoint
- **WHEN** invoking `canon-clerk check-config --probe` with valid credentials and network reachability
- **THEN** actively exercises the production generation API, reports reachability status with roundtrip latency in milliseconds, and exits 0

#### Scenario: Attributing resolved concrete model snapshot
- **WHEN** probing a model tier configured with an alias where the provider returns underlying model version metadata
- **THEN** captures the underlying model identifier in `resolvedModel` and reports it in stylish and JSON outputs

#### Scenario: Failing probe on missing credentials
- **WHEN** invoking `canon-clerk check-config --probe` without configured provider credentials
- **THEN** fails without network call, reports `missing_credentials` category, and exits with status 1

#### Scenario: Overriding probe timeout deadline via CLI flag
- **WHEN** invoking `canon-clerk check-config --probe --probe-timeout 5000`
- **THEN** enforces a 5,000ms deadline on model endpoint probing

#### Scenario: Overriding probe timeout via environment variable
- **WHEN** invoking `canon-clerk check-config --probe` with `CANON_CLERK_PROBE_TIMEOUT_MS=8000`
- **THEN** enforces an 8,000ms deadline on model endpoint probing

#### Scenario: Rejecting invalid probe timeout values
- **WHEN** invoking `canon-clerk check-config --probe --probe-timeout abc` or negative value
- **THEN** throws an invalid argument error and exits with code 2

### Requirement: Probe Failure Disambiguation
The `check-config --probe` command SHALL classify probe errors into structured failure categories (`missing_credentials`, `authentication`, `authorization`, `bad_request`, `model_not_found`, `rate_limited`, `network_error`, `timeout`, or `unknown`) and provide actionable remediation hints.

#### Scenario: Classifying authentication failure
- **WHEN** probing an endpoint with an invalid or expired API key resulting in an authentication error
- **THEN** reports failure with category `authentication` and a remediation hint directing to credential setup

#### Scenario: Classifying authorization failure
- **WHEN** probing an endpoint with a credential lacking required permissions or disabled API
- **THEN** reports failure with category `authorization` and a permission remediation hint

#### Scenario: Classifying model not found error
- **WHEN** probing a tier configured with an unrecognized or deprecated model identifier
- **THEN** reports failure with category `model_not_found` and a model configuration hint

#### Scenario: Classifying network and transport errors
- **WHEN** probing an endpoint when DNS fails or connection cannot be established
- **THEN** reports failure with category `network_error` and a network connectivity hint

#### Scenario: Enforcing probe deadline timeout
- **WHEN** probing an endpoint that hangs beyond the configured timeout deadline
- **THEN** aborts execution, reports failure with category `timeout`, and exits with status 1
