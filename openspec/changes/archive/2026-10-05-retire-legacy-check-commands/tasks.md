# Tasks

## 1. Salvage Domain Logic into `@canon-clerk/core`
- [x] 1.1 Relocate `probe-classifier.*` to `packages/core/src/probe-classifier.*`
- [x] 1.2 Relocate `probe-runner.*` to `packages/core/src/probe-runner.*` (accepting normalized `ModelConfig`)
- [x] 1.3 Export `probe-classifier` and `probe-runner` from `packages/core/src/index.ts`
- [x] 1.4 Ensure unit tests pass in `@canon-clerk/core` and integration tests pass in `@canon-clerk/integration-tests-private`

## 2. Remove Legacy Incompatible CLI Modules
- [x] 2.1 Delete `packages/cli/src/commands/check-canons.*`
- [x] 2.2 Delete `packages/cli/src/commands/check-triggers.*`
- [x] 2.3 Delete `packages/cli/src/commands/check-config.*`
- [x] 2.4 Delete entire `packages/cli/src/formatters/` directory

## 3. Prune CLI Entrypoints to Clean Router Skeleton
- [x] 3.1 Update `packages/cli/src/app.ts` to bare root Commander program with standard options, signal handlers, and exit codes
- [x] 3.2 Update `packages/cli/src/index.ts` exports
- [x] 3.3 Update `packages/cli/src/cli.integration.test.ts` to assert bare router behavior
- [x] 3.4 Retain `packages/cli/src/testing/harness.ts`

## 4. Root Repository Scripts & Workflow Hygiene
- [x] 4.1 Remove `"check-canons"` and `"precheck-canons"` from root `package.json`
- [x] 4.2 Update `"check"` script in `package.json` to prune `"npm run check-canons"`

## 5. Prune Obsoleted OpenSpec Living Specs
- [x] 5.1 Remove `openspec/specs/cli/check-canons/`
- [x] 5.2 Remove `openspec/specs/cli/check-triggers/`
- [x] 5.3 Remove `openspec/specs/cli/check-config/`
- [x] 5.4 Update `openspec/specs/cli/spec.md` to prune legacy subcommand routing
- [x] 5.5 Verify `npm run lint:specs` passes with zero validation errors
