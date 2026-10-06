# Tasks

## 1. Salvage Domain Logic into `@canon-clerk/configuration`
- [ ] 1.1 Relocate `packages/cli/src/commands/probe-classifier.*` to `packages/configuration/src/probe-classifier.*`
- [ ] 1.2 Relocate `packages/cli/src/commands/probe-runner.*` to `packages/configuration/src/probe-runner.*`
- [ ] 1.3 Export `probe-classifier` and `probe-runner` from `packages/configuration/src/index.ts`
- [ ] 1.4 Ensure unit tests pass in `@canon-clerk/configuration`

## 2. Remove Legacy Incompatible CLI Modules
- [ ] 2.1 Delete `packages/cli/src/commands/check-canons.*`
- [ ] 2.2 Delete `packages/cli/src/commands/check-triggers.*`
- [ ] 2.3 Delete `packages/cli/src/commands/check-config.*`
- [ ] 2.4 Delete entire `packages/cli/src/formatters/` directory

## 3. Prune CLI Entrypoints to Clean Router Skeleton
- [ ] 3.1 Update `packages/cli/src/app.ts` to bare root Commander program with standard options, signal handlers, and exit codes
- [ ] 3.2 Update `packages/cli/src/index.ts` exports
- [ ] 3.3 Update `packages/cli/src/cli.integration.test.ts` to assert bare router behavior
- [ ] 3.4 Retain `packages/cli/src/testing/harness.ts`

## 4. Root Repository Scripts & Workflow Hygiene
- [ ] 4.1 Remove `"check-canons"` and `"precheck-canons"` from root `package.json`
- [ ] 4.2 Update `"check"` script in `package.json` to prune `"npm run check-canons"`

## 5. Prune Obsoleted OpenSpec Living Specs
- [ ] 5.1 Remove `openspec/specs/cli/check-canons/`
- [ ] 5.2 Remove `openspec/specs/cli/check-triggers/`
- [ ] 5.3 Remove `openspec/specs/cli/check-config/`
- [ ] 5.4 Update `openspec/specs/cli/spec.md` to prune legacy subcommand routing
- [ ] 5.5 Verify `npm run lint:specs` passes with zero validation errors
