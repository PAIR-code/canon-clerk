# Proposal: Retire Legacy check-* Commands and Salvage Probe Modules

## Why
With the adoption of the Caseload DAG pipeline architecture (PR #197 / #196 / #195), Canon Clerk is transitioning from disjoint legacy inspection subcommands (`check-canons`, `check-triggers`, `check-config`) to a cohesive Caseload DAG (`intake` -> `discover` -> `validate` on Branch A; `configure` on Branch B; converging at `docket` -> `admit` -> `audit`, with diagnostic `probe`).

Before implementing the new Caseload DAG pipeline nodes, the repository must undergo Phase 1: Removal & Salvage. This greenfield pruning excises obsolete presentation-layer CLI commands and formats, while relocating reusable domain logic (specifically probe classifier and probe runner routines) from `packages/cli` into `@canon-clerk/configuration` to adhere to hexagonal architecture boundaries.

## What Changes
1. **Salvage Domain Logic:** Move `probe-classifier.*` and `probe-runner.*` from `packages/cli/src/commands/` to `packages/configuration/src/`, re-exporting them from `@canon-clerk/configuration`.
2. **Remove Retired CLI Commands:** Delete `check-canons.*`, `check-triggers.*`, and `check-config.*` alongside unit and integration tests, as well as the entire `packages/cli/src/formatters/` directory.
3. **Prune CLI Router:** Simplify `packages/cli/src/app.ts` to a bare Commander program with version (`-v, --version`), help (`-h, --help`), POSIX signal handling (`SIGINT` -> 130, `SIGTERM` -> 143), and standard exit codes.
4. **Root Workflow Hygiene:** Remove obsolete `check-canons` and `precheck-canons` scripts from the root `package.json`, updating the `check` script.
5. **Retire OpenSpec Specs:** Remove living specs for `cli/check-canons`, `cli/check-triggers`, and `cli/check-config`, and update `cli/spec.md`.

## Capabilities
### Modified Capabilities
- `cli`: Prune legacy `check-*` subcommand routing in favor of the bare root router skeleton and updated help/version/signal handling.

### Retired Capabilities
- `cli/check-canons`: Retired in favor of upcoming Caseload DAG `validate` node.
- `cli/check-triggers`: Retired in favor of upcoming Caseload DAG `discover` node.
- `cli/check-config`: Retired in favor of upcoming Caseload DAG `configure` and `probe` nodes.

## Impact
- **`@canon-clerk/configuration`**: Now exports `classifyProbeError`, `getMissingCredentialsHint`, `probeTier`, `executeCascadeProbes`, and associated types.
- **`packages/cli`**: Stripped of legacy `check-*` subcommands and formatters; exports only base app and version utilities.
- **Root `package.json`**: Pruned `check-canons` invocation.
