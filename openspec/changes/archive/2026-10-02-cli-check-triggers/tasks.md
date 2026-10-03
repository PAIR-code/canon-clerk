# Tasks

## 1. Command Infrastructure & Routing
- [x] 1.1 Implement `createCheckTriggersCommand` and `runCheckTriggersCommand` in `packages/cli/src/commands/check-triggers.ts`.
- [x] 1.2 Register `check-triggers` in `createApp` in `packages/cli/src/app.ts` without aliases, and update root help screen to document the subcommand.
- [x] 1.3 Implement categorized help screen for `check-triggers --help` with runnable examples per CLI help canons.

## 2. Argument Parsing, Validation & Core Delegation
- [x] 2.1 Implement positional target parsing, expanding globs, directories, and concrete paths, and enforcing workspace root containment (`--cwd`).
- [x] 2.2 Implement standard input ingestion for targets (`-`) and canons (`--canon -`), enforcing mutual exclusivity with exit code 2.
- [x] 2.3 Implement naked invocation validation rejecting runs without targets, `-`, `--all`, or `--canon` with exit code 2 and remediation hint.
- [x] 2.4 Implement single-canon inversion defaulting targets to `**/*` when `--canon` is provided without targets.
- [x] 2.5 Resolve tiered ignore options (`--ignore`, `--default-ignores`, `--target-ignore`, `--canon-ignore`, etc.) and delegate to `@canon-clerk/core`'s `queryCanons`.

## 3. Presentation Formatters & Exit Codes
- [x] 3.1 Implement `stylish` terminal formatter grouping matches by canon, displaying scope and triggering files with matched triggers.
- [x] 3.2 Implement canonical `json` formatter emitting match records with 3-plane pattern attribution and empty array `[]` on zero matches.
- [x] 3.3 Implement predicate mode (`--quiet` / `-q`), short-circuiting on the first match tuple and returning exit status 0 (triggered) vs 1 (none).
- [x] 3.4 Ensure non-quiet mode exits with status 0 on zero matches per `cli-queries-must-exit-zero-on-empty-results`.

## 4. Verification & Testing
- [x] 4.1 Unit tests in `packages/cli/src/commands/check-triggers.test.ts` covering option resolution, stdin ingestion, and error handling.
- [x] 4.2 Integration tests in `packages/cli/src/commands/check-triggers.integration.test.ts` testing end-to-end execution, stylish output, JSON schema attribution, quiet mode predicate exit codes, naked invocation errors, and stdin piping.
- [x] 4.3 Run full check suite (`npm run check`) and OpenSpec validation (`npm run opsx -- validate cli-check-triggers --strict`).
