# Proposal

## Why

Static canon linting (`@canon-clerk/schema`) and workspace discovery orchestration (`@canon-clerk/core`) provide the foundational domain services for canon quality enforcement. However, developers, CI workflows, and automated bots currently lack a unified command-line entrypoint to execute linting across repositories or targeted paths, view human-readable diagnostic messages with source coordinates and actionable remediations, obtain machine-readable JSON envelopes, and enforce exit code thresholds (Issue #140).

Following architectural canons, `@canon-clerk/cli` must serve as a presentation adapter translating command-line operands and flags into domain orchestrations in `@canon-clerk/core`, leveraging the industry-standard `commander` framework alongside Node 24 standard built-in `node:util.styleText` for terminal styling.

## What Changes

- Implement a modular command router in `@canon-clerk/cli` supporting global options (`--help`, `--version`), subcommand dispatch (`lint`), and standard signal traps (`SIGINT`, `SIGTERM`).
- Implement the `canon-clerk lint` subcommand accepting positional targets, standard input (`-`), and configuration options (`-g, --glob`, `--stdin-filename`, `--format`, `--json`, `--quiet`, `--max-warnings`, `--ignore`, `--default-ignores`).
- Provide human-friendly terminal formatting (`stylish`) with column-aligned coordinates, color-coded severity badges, actionable remediation hints, and clean workspace status.
- Provide deterministic, actionable JSON reporting (`json`) emitting a canonical array of `FileLintResult` records containing only files with diagnostics directly to `stdout` (emitting `[]` on a clean workspace).
- Implement deterministic exit code mapping: `0` for clean runs or acceptable warnings, `1` for lint violations or exceeded thresholds, `2` for syntax/usage errors, `130`/`143` for signals, and `3+` for runtime failures.
- Wire `canon-clerk lint` into the repository's root `npm run check` pipeline to dogfood static linting across all repository canons.

## Capabilities

### New Capabilities
- `cli`: Command-line interface entrypoint, subcommand routing, global options, signal handling, and standardized exit code resolution.
- `cli/lint`: Static canon linting subcommand presentation adapter, target and standard input ingestion, terminal styling, and canonical JSON output.

## Impact

- `@canon-clerk/cli`: Becomes a fully functional CLI binary providing the `lint` command and extensible routing for future commands (`audit`, `sync`).
- Monorepo developer workflow: Enables local canon verification via `npm run lint:canons` and continuous enforcement in `npm run check`.
- CI / Automated Tooling: Unblocks deterministic machine-readable JSON inspection and exit-code quality gates in pull request workflows.
