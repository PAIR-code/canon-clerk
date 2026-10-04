# Tasks: Establish @canon-clerk/integration-tests-private

## 1. Package Scaffolding & Configuration
- [ ] 1.1 Scaffold `packages/integration-tests-private` directory structure, `package.json`, and scoped `tsconfig.json` adhering to private package canons.
- [ ] 1.2 Verify package manifest omits `types` and `main` fields and contains `"private": true`.

## 2. Test Execution Stratification
- [ ] 2.1 Update root `package.json` `test` script to exclude `packages/integration-tests-private/**`.
- [ ] 2.2 Add `test:integration` script in root `package.json` targeting `packages/integration-tests-private`.
- [ ] 2.3 Verify `npm test` and `npm run check` run cleanly offline and hermetically.

## 3. Exemplar Live Connectivity Smoke Test
- [ ] 3.1 Implement live model probe smoke test in `packages/integration-tests-private/src/live-probe.integration.test.ts` consuming `@canon-clerk/configuration` and `@canon-clerk/core`.
- [ ] 3.2 Verify test fails by default with actionable remediation instructions when API credentials are unset.
- [ ] 3.3 Verify test cleanly skips when `CANON_CLERK_SKIP_LIVE_TESTS=1` is passed.
- [ ] 3.4 Verify test passes end-to-end with live model connectivity when valid credentials are provided.

## 4. Sibling CI Workflow
- [ ] 4.1 Create `.github/workflows/integration.yml` with path filtering, secret guarding for forks, and concurrency cancellation.
- [ ] 4.2 Verify CI verification parity with local checks per governance canons.

## 5. Sanctioned Canon Codification
- [ ] 5.1 Author `.canons/testing/live-service-tests-must-be-segregated.md`.
- [ ] 5.2 Author `.canons/testing/live-service-tests-must-fail-by-default-with-actionable-remediation.md`.
- [ ] 5.3 Author `.canons/testing/package-entrypoint-tests-must-colocate-within-package.md`.
- [ ] 5.4 Author `.canons/repo-governance/internal-workspace-packages-must-suffix-private.md`.
- [ ] 5.5 Verify canons pass `npm run check-canons`.
