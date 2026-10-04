# Spec Delta: model-client

## ADDED Requirements

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
