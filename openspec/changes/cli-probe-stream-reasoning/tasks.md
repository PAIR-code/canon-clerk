# Tasks: Echo Reasoning Thoughts during check-config Probing

## 1. Diagnostics & Probe Runner Streaming Integration
- [ ] 1.1 Update `ModelTierProbeResult` in `packages/configuration/src/diagnostics.ts` with `timeToFirstThoughtMs`, `timeToFirstTokenMs`, and `thoughtTokens`
- [ ] 1.2 Add `onThought` callback parameter to `ProbeTierOptions` and `ExecuteCascadeProbesOptions` in `packages/cli/src/commands/probe-runner.ts`
- [ ] 1.3 Update `probeTier` to call `client.streamStructured` when available, invoke `onThought`, and capture latency telemetry
- [ ] 1.4 Maintain graceful fallback to unary generation when `streamStructured` is not available

## 2. CLI Command Wiring & Formatter Updates
- [ ] 2.1 Add `--no-thoughts` option to `canon-clerk check-config` command definition and `CANON_CLERK_PROBE_THOUGHTS` env fallback
- [ ] 2.2 Wire `onThought` in `runCheckConfigCommand` in `packages/cli/src/commands/check-config.ts` to stream thoughts to `stderr`
- [ ] 2.3 Suppress thought output when `--no-thoughts` is passed, `CANON_CLERK_PROBE_THOUGHTS=false`, or in quiet mode (`-q`)
- [ ] 2.4 Update `formatCheckConfigStylish` in `packages/cli/src/formatters/check-config.ts` to render `Latency:` line when thoughts occurred
- [ ] 2.5 Ensure `--json` output on `stdout` embeds telemetry fields and remains valid JSON

## 3. Unit & Integration Testing
- [ ] 3.1 Add unit tests in `packages/cli/src/commands/probe-runner.test.ts` for streaming thoughts, latency telemetry, and fallback
- [ ] 3.2 Add tests in `packages/cli/src/commands/check-config.test.ts` and `formatters.test.ts` verifying thought streaming to `stderr`, `--no-thoughts` silencing, latency formatting, and JSON validity on `stdout`
- [ ] 3.3 Verify full test suite passes (`npm test`)

## 4. OpenSpec Specifications & Verification
- [ ] 4.1 Update living specification in `openspec/specs/cli/check-config/spec.md`
- [ ] 4.2 Validate OpenSpec change with `npm run opsx -- validate cli-probe-stream-reasoning --strict`
- [ ] 4.3 Run full repository build and test suite (`npm run check`)
