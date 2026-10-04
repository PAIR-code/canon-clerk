# Proposal: Establish @canon-clerk/integration-tests-private for Cross-Package and Live Service Testing

## Why

As Canon Clerk implements its Three-Phase Evaluation Cascade (Check → Docket → Audit), a clean division of testing horizons is required. Unit tests within packages must remain fast (<100ms), offline, and hermetic. At the same time, end-to-end integration tests that cross package boundaries (such as configuration resolution combined with core inference clients) and connect to external remote model providers need an isolated workspace. Housing live service tests inside public packages like `@canon-clerk/core` pollutes the engine's test suite with ambient environment dependencies, while sliding them into `@canon-clerk/cli` conflates command line routing with core evaluation cascade verification.

Establishing a dedicated internal workspace, `packages/integration-tests-private` (`@canon-clerk/integration-tests-private`), creates a dedicated home for cross-package and live service integration tests without tangling public package boundaries.

## What Changes

1. **Workspace Package Scaffolding (`packages/integration-tests-private`)**:
   - Establish `@canon-clerk/integration-tests-private` adhering to the `-private` suffix naming convention for internal tooling and test runners.
   - Configure manifest with `"private": true`, omitting `types` and `main` fields per package governance canons.
   - Configure scoped TypeScript configuration extending the workspace root base.

2. **Two-Tier Test Scripting**:
   - Stratify test execution into Tier 1 (hermetic unit and CLI subprocess tests via `npm test`) and Tier 2 (cross-package and live service integration tests via `npm run test:integration`).
   - Exclude `packages/integration-tests-private/**` from `npm test` so that routine verification and pre-commit checks (`npm run check`) remain 100% hermetic, fast, and offline.

3. **4-Pillar Integration Testing Standard & Probe Smoke Test**:
   - Author an exemplar live connectivity smoke test that resolves host credentials via `@canon-clerk/configuration` and invokes `@canon-clerk/core` to verify live endpoint reachability using a lightweight probe query (`{ ok: boolean }`), mirroring the diagnostic probe pattern.
   - Enforce fail-by-default behavior when credentials are missing, emitting actionable diagnostic remediation instructions.
   - Support explicit bypass via `CANON_CLERK_SKIP_LIVE_TESTS=1`.

4. **Dedicated Sibling CI Workflow (`.github/workflows/integration.yml`)**:
   - Introduce an isolated GitHub Actions workflow executing live service tests.
   - Guard against missing repository secrets on fork pull requests, cleanly skipping without failing the verification gate.
   - Apply path filtering and concurrency cancellation to optimize API quota consumption.

5. **Sanctioned Canon Codification**:
   - Codify previously unstated governance invariants into project canons covering live test segregation, fail-by-default integration test diagnostics, CLI entrypoint test colocation, and internal workspace `-private` naming.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `model-client`: Added requirements for cross-package live connectivity smoke testing adhering to the 4-pillar integration testing standard.

## Impact

- **Packages**: Adds internal workspace package `packages/integration-tests-private`.
- **Scripts**: Updates root `package.json` with two-tier test scripts (`test` and `test:integration`).
- **CI**: Adds `.github/workflows/integration.yml` sibling workflow.
- **Canons**: Adds project canons under `.canons/testing/` and `.canons/repo-governance/`.
