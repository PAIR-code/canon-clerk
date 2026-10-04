# Design: Dedicated Integration Testing Workspace & Live Service Verification

## Context

Canon Clerk's evaluation cascade relies on deterministic, fast local analysis during Phase 1 (Check), followed by semantic evaluation in subsequent phases that interact with language models. While unit tests and CLI subprocess integration tests must run in isolated, hermetic, and offline environments during routine development and commit verification, verifying end-to-end connectivity across packages against remote service endpoints requires an ambient environment (API keys, network reachability, quota). Housing live service tests directly inside domain packages pollutes hermetic test runs, while placing them into CLI packages conflates command interface routing with core evaluation mechanics.

## Goals / Non-Goals

**Goals:**
- Establish a segregated workspace package for cross-package and live service testing adhering to the internal `-private` convention.
- Establish two distinct execution tiers: hermetic routine verification (`npm test` and `npm run check`) and on-demand live service verification (`npm run test:integration`).
- Implement an end-to-end smoke test probe exercising configuration resolution and model client communication against live endpoints.
- Enforce the 4-pillar integration standard: standard configuration resolution, fail-by-default behavior on missing credentials, actionable remediation diagnostics, and explicit bypass flags.
- Author an isolated sibling CI workflow running live integration tests on changes to core evaluation paths while safely guarding repository secrets for fork pull requests.
- Codify unstated testing and workspace governance invariants into project canons.

**Non-Goals:**
- Outsource or relocate package-scoped hermetic CLI subprocess tests away from their colocated packages.
- Require live credentials or external network connectivity for standard pre-commit verification or routine pull request CI.
- Publish `@canon-clerk/integration-tests-private` to the public npm package registry.

## Decisions

### 1. Dedicated Private Integration Package (`packages/integration-tests-private`)
- **Decision:** Establish cross-package integration tests in an internal monorepo package marked `"private": true`, omitting publication fields and type declaration build steps.
- **Rationale:** Separating live and multi-package testing from core domain libraries preserves package boundary cleanliness, minimizes dependency pollution, and prevents ambient environment requirements from creeping into hermetic unit test suites.
- **Alternatives Considered:**
  - *Colocating live tests in `@canon-clerk/core`:* Rejected because it introduces network dependencies and conditional skip complexity into foundational engine tests.
  - *Housing live tests in `@canon-clerk/cli`:* Rejected because it conflates CLI command parsing with backend model client communication.

### 2. Two-Tier Test Execution Stratification
- **Decision:** Configure standard test runs to exclude the private integration workspace, reserving its execution for a dedicated integration script.
- **Rationale:** Preserves fast, deterministic, offline execution for routine developer workflows (`npm run check`) while providing an intentional entrypoint for end-to-end live testing.
- **Alternatives Considered:**
  - *Running all tests in default test command with automatic silent skipping when credentials are absent:* Rejected because silent skipping hides configuration errors and causes false positive confidence in CI.

### 3. Four-Pillar Integration Testing Standard
- **Decision:** Mandate that live tests resolve credentials through the standard configuration system, fail explicitly with remediation guidance when credentials are unavailable, and provide a standardized bypass environment flag.
- **Rationale:** Ensures developers and continuous integration environments receive explicit feedback rather than silent passes, with clear diagnostic paths to resolve missing credentials.
- **Alternatives Considered:**
  - *Ad-hoc environment variable loading per test file:* Rejected because it duplicates resolution logic and diverges from production credential precedence.

### 4. Sibling CI Workflow with Secret Guards and Concurrency Cancellation
- **Decision:** Execute live integration tests in a dedicated GitHub Actions workflow triggered only on relevant path changes, guarded by repository secret presence, with automatic in-progress cancellation on newer commits.
- **Rationale:** Isolates live service flakiness or provider downtime from core pull request checks, cleanly accommodates forks lacking repository secrets, and conserves API quotas.
- **Alternatives Considered:**
  - *Embedding live tests as an optional step in the main CI workflow:* Rejected because transient upstream API latency or rate limits would disrupt core build status.

## Risks / Trade-offs

- **Risk:** Upstream model provider rate limits or latency fluctuations may cause intermittent failures in integration workflows.
  - *Mitigation:* Live integration workflows are decoupled from required hermetic gates, use lightweight probe queries, and employ concurrency cancellation.
- **Risk:** Developers without API keys might miss integration test failures during local development.
  - *Mitigation:* The live suite fails with actionable instructions on how to configure keys, and hermetic mock doubles verify complete logic coverage offline.
