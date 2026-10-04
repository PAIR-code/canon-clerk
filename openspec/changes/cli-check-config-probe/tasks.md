# Tasks

## 1. Type Definitions & Diagnostics Contract
- [ ] 1.1 Define `ProbeFailureCategory` enum/union in `@canon-clerk/configuration/src/diagnostics.ts` (`missing_credentials`, `authentication`, `authorization`, `bad_request`, `model_not_found`, `rate_limited`, `network_error`, `timeout`, `unknown`).
- [ ] 1.2 Extend `ModelTierProbeResult` with `category?: ProbeFailureCategory`, `error?: string`, `hint?: string`, and `durationMs?: number`.

## 2. Probe Disambiguation Engine & Timeout Configuration
- [ ] 2.1 Implement `classifyProbeError(err: unknown, provider: string)` helper in `@canon-clerk/cli` analyzing HTTP status codes, error causes, and provider message signatures.
- [ ] 2.2 Implement `getRemediationHint(category: ProbeFailureCategory, tier: string, provider: string)` returning actionable setup instructions.
- [ ] 2.3 Implement `--probe-timeout <ms>` option and `CANON_CLERK_PROBE_TIMEOUT_MS` environment resolution with validation (positive integer).
- [ ] 2.4 Wire disambiguation and configurable timeout into `probeTier()` in `packages/cli/src/commands/check-config.ts` with pre-flight missing key short-circuit.

## 3. Formatting & Presentation
- [ ] 3.1 Update stylish formatter to print failure categories, error details, and remediation hints cleanly formatted under failing tiers.
- [ ] 3.2 Update JSON formatter to include `category`, `error`, and `hint` in `tier.probe`.
- [ ] 3.3 Add diagnostic tip in stylish formatter signposting 'canon-clerk check-config --probe' when credentials are configured but unprobed.

## 4. Test Verification
- [ ] 4.1 Add unit tests for `classifyProbeError` covering all 8 failure categories and provider-specific error payloads.
- [ ] 4.2 Add unit tests for stylish and JSON formatters verifying categorized failure outputs.
- [ ] 4.3 Add unit tests for `--probe-timeout` CLI flag, environment variable override, and invalid argument rejection.
- [ ] 4.4 Run full test and check suite (`npm run check`) ensuring zero regressions across all packages.
