# Tasks

## 1. Core Types & Rule Context

- [x] 1.1 Define linting interfaces and types (`DiagnosticSeverity`, `RuleSeverity`, `CanonDiagnostic`, `RuleConfig`, `LintCanonOptions`, `RuleContext`, `CanonLintRule`) in `packages/schema/src/types.ts` and verify typechecking with `npm run typecheck -w packages/schema`.
- [x] 1.2 Implement the immutable `RuleContext` class in `packages/schema/src/context.ts` with pure in-memory path derivation, lazy memoized property getters, and defensive token freezing.
- [x] 1.3 Add unit tests for `RuleContext` in `packages/schema/src/context.test.ts` verifying filename/stem extraction, lazy memoization (ensuring zero YAML parsing overhead for token-only queries), and token immutability via `Object.freeze`; verify tests pass with `npm test -w packages/schema`.

## 2. Fault-Tolerant Runner & Severity Resolution

- [x] 2.1 Implement `lintCanon()` in `packages/schema/src/runner.ts` with safe rule orchestration, unhandled exception catching, severity configuration override application, and deterministic diagnostic sorting (line -> column -> code).
- [x] 2.2 Add unit tests for `lintCanon()` in `packages/schema/src/runner.test.ts` verifying fault-tolerant execution, rule error encapsulation, `'off'` rule filtering, `'warning'`/`'error'` severity remapping, and coordinate sorting; verify tests pass with `npm test -w packages/schema`.

## 3. Package Integration & Validation

- [x] 3.1 Export public linting types, `RuleContext`, and `lintCanon` from `packages/schema/src/index.ts` and update `packages/schema/src/index.test.ts` to assert export availability.
- [x] 3.2 Run repository-wide checks via `npm run check` (lockfile lint, spec validation, typecheck across workspaces, builds, and test runs) to verify zero I/O side-effects and green build pipeline.
