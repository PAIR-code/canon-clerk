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

### 1. Relocate Probe Logic to `@canon-clerk/core` (Receiving Normalized Config)
- **Decision:** Relocate `probe-classifier.ts` and `probe-runner.ts` into `@canon-clerk/core/src/` and re-export them from `packages/core/src/index.ts`. `probeTier` accepts a normalized `ModelConfig` produced by `@canon-clerk/configuration`.
- **Rationale:** Probing model endpoints and classifying network/auth/quota failures requires live model client creation and network execution (`createModelClient`). Placing probe routines in `@canon-clerk/configuration` would either create a circular dependency with `@canon-clerk/core` or force `@canon-clerk/configuration` to take on network dependencies. To adhere to clean hexagonal boundaries:
  - `@canon-clerk/configuration` is responsible strictly for configuration *performance* (host inspection, environment reading, cascading file walks). It is 100% offline, deterministic, and zero-network.
  - `@canon-clerk/core` owns the configuration *schema* (`ModelConfig`) and model transport/execution. `probeTier` accepts a normalized configuration and performs the probe.
  - `@canon-clerk/integration-tests-private` provides the integration test exercising `resolveModelConfig` alongside `probeTier`.
- **Alternatives Considered:** Moving probe logic to `@canon-clerk/configuration` (rejected due to circular dependency on `core`'s model client and violating offline purity of configuration).

### 2. Complete Removal of Legacy Formatters
- **Decision:** Delete `packages/cli/src/formatters/` entirely.
- **Rationale:** The formatters (`stylish.ts`, `check-config.ts`, `check-triggers.ts`, `json.ts`) were tightly coupled to the outputs of the legacy `check-*` commands. Caseload DAG outputs will define their own standard reporting formats in subsequent phases.

### 3. Bare CLI Router Skeleton
- **Decision:** Maintain `canon-clerk` root program with options `-v, --version`, `-h, --help`, POSIX signal traps (`SIGINT` -> 130, `SIGTERM` -> 143), and standard exit codes.
- **Rationale:** Preserves the CLI entrypoint harness and test infrastructure while ensuring no obsolete subcommands linger.

## Risks / Trade-offs
- **Risk:** Existing workflows or scripts calling `npm run check-canons` will fail.
  - **Mitigation:** Update root `package.json` to prune `check-canons` and `precheck-canons` and adjust the aggregate `check` script. Pre-v1 refactors explicitly permit zero-shim excisions.
