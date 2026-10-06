# Design: Removal of Legacy check-* Commands & Salvage of Probe Modules

## Context
Canon Clerk is transitioning to the Caseload Directed Acyclic Graph (DAG) architecture defined in PR #197. The legacy inspection subcommands (`check-canons`, `check-triggers`, `check-config`) represent an older imperative design that couples presentation formatting, cascade evaluation, and path filtering. 

Before introducing the new Caseload DAG nodes (`intake`, `discover`, `validate`, `configure`, `probe`, `docket`, `admit`, `audit`), the repository needs a clean slate:
1. presentation adapters (`packages/cli`) should be pruned of old commands and formatters,
2. domain logic for model probing should be salvaged into `@canon-clerk/configuration` to enforce hexagonal architecture boundaries,
3. legacy command scripts and living specs should be excised cleanly without backwards compatibility aliases, per `.canons/versioning/pre-v1-refactors-must-omit-backwards-compatibility-shims.md`.

## Goals / Non-Goals

**Goals:**
- Atomically remove `check-canons`, `check-triggers`, `check-config`, and all associated formatters from `packages/cli`.
- Salvage `probe-classifier` and `probe-runner` (and their unit tests) from `packages/cli` into `packages/configuration`.
- Prune `packages/cli/src/app.ts` into a clean, minimal Commander router skeleton with help, version, and POSIX signal handling.
- Maintain 100% passing tests, typecheck, and build across all packages.
- Prune legacy living specs from `openspec/specs/cli/`.

**Non-Goals:**
- Introducing any new Caseload schema types, DAG orchestration, or new subcommands in this phase.
- Adding backwards-compatibility shims or deprecated aliases for retired `check-*` commands.

## Decisions

### 1. Relocate Probe Logic to `@canon-clerk/configuration`
- **Decision:** Move `probe-classifier.ts` and `probe-runner.ts` directly into `@canon-clerk/configuration/src/` and re-export them from `packages/configuration/src/index.ts`.
- **Rationale:** Probing model endpoints and classifying network/auth/quota failures is environment and configuration diagnostics domain logic. Presentation adapters in `packages/cli` should only wrap and render, not house core probing logic. Moving these modules to `@canon-clerk/configuration` makes them available to both the upcoming `probe` DAG node and future consumers.
- **Alternatives Considered:** Leaving them in `packages/cli` or moving them to `@canon-clerk/core`. Core is focused on DAG primitives and engine evaluation, whereas provider diagnostics and credential verification belong in `@canon-clerk/configuration`.

### 2. Complete Removal of Legacy Formatters
- **Decision:** Delete `packages/cli/src/formatters/` entirely.
- **Rationale:** The formatters (`stylish.ts`, `check-config.ts`, `check-triggers.ts`, `json.ts`) were tightly coupled to the outputs of the legacy `check-*` commands. Caseload DAG outputs will define their own standard reporting formats in subsequent phases.

### 3. Bare CLI Router Skeleton
- **Decision:** Maintain `canon-clerk` root program with options `-v, --version`, `-h, --help`, POSIX signal traps (`SIGINT` -> 130, `SIGTERM` -> 143), and standard exit codes.
- **Rationale:** Preserves the CLI entrypoint harness and test infrastructure while ensuring no obsolete subcommands linger.

## Risks / Trade-offs
- **Risk:** Existing workflows or scripts calling `npm run check-canons` will fail.
  - **Mitigation:** Update root `package.json` to prune `check-canons` and `precheck-canons` and adjust the aggregate `check` script. Pre-v1 refactors explicitly permit zero-shim excisions.
