# Tasks

## 1. CLI Command Rebranding (`check-canons`)
- [x] 1.1 Implement `createCheckCanonsCommand` in `packages/cli/src/commands/check-canons.ts` (renamed from `lint.ts`) with updated help text, examples, and descriptions.
- [x] 1.2 Update `packages/cli/src/app.ts` to register `check-canons`, update root help screen, and update error hints.
- [x] 1.3 Update CLI test suite (`check-canons.test.ts`, `check-canons.integration.test.ts`, `cli.integration.test.ts`) to verify `check-canons` execution and rejection of retired `lint`.
- [x] 1.4 Update root `package.json` scripts (`check-canons`, `npm run check`) and rebuild/test CLI package.

## 2. Architectural Blueprint Publication
- [x] 2.1 Publish formal architecture specification at `docs/architecture/evaluation-cascade.md` detailing the Three-Phase state machine, verb contracts, and `FileArtifact` abstraction.
- [x] 2.2 Update `docs/architecture.md` to reflect Three-Phase nomenclature and link to `evaluation-cascade.md`.

## 3. Terminology & Documentation Harmonization
- [x] 3.1 Update `SPEC.md` to reflect "Phase 1: Check", "Phase 2: Docket", and "Phase 3: Audit".
- [x] 3.2 Update `README.md` to reference the Three-Phase cascade and link to the architecture blueprint.
- [x] 3.3 Update `AGENTS.md` and `llms.txt` to harmonize evaluation cascade summaries.
- [x] 3.4 Update code comments and command descriptions across `packages/cli`, `packages/schema`, and `packages/core`.

## 4. OpenSpec Living Spec Alignment & Verification
- [x] 4.1 Update living specifications in `openspec/specs/` (`cli/check-canons`, `cli/check-triggers`, `query-canons`) to reflect the Three-Phase vocabulary.
- [x] 4.2 Run `npm run check` and verify that all test suites, typechecks, builds, specs, and canon linting pass cleanly.
