# @canon-clerk/integration-tests-private

Dedicated cross-package and live service integration test harness for Canon Clerk.

## Purpose

This private internal workspace houses end-to-end tests that:
1. Cross multiple workspace package boundaries (e.g., `@canon-clerk/configuration` resolving host OS credentials + `@canon-clerk/core` executing inference).
2. Connect to live external model providers (such as Gemini).

By isolating these non-hermetic, credential- and quota-dependent tests into a dedicated private package, Canon Clerk ensures that:
- Routine unit tests (`npm test`) and repository checks (`npm run check`) remain 100% hermetic, fast (<3s), and executable offline.
- Public workspace packages (`@canon-clerk/core`, `@canon-clerk/cli`) remain uncluttered by host credential prerequisites and skip gymnastics.

## Execution

Run all live service and cross-package integration tests:

```bash
npm run test:integration
```

### Bypass

To skip live service network tests in offline or credential-free environments:

```bash
CANON_CLERK_SKIP_LIVE_TESTS=1 npm run test:integration
```
