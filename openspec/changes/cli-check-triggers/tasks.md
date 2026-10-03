# Tasks

## 1. Command Infrastructure & Routing
- [ ] 1.1 Implement `createCheckTriggersCommand` and `runCheckTriggersCommand` in `packages/cli/src/commands/check-triggers.ts`.
- [ ] 1.2 Register `check-triggers` in `createApp` in `packages/cli/src/app.ts` without aliases, and update root help screen to document the subcommand.
- [ ] 1.3 Implement categorized help screen for `check-triggers --help` with runnable examples per CLI help canons.

## 2. Argument Parsing, Validation & Core Delegation
- [ ] 2.1 Implement positional target parsing, expanding globs, directories, and concrete paths, and enforcing workspace root containment (`--cwd`).
- [ ] 2.2 Implement standard input ingestion for targets (`-`) and canons (`--canon -`), enforcing mutual exclusivity with exit code 2.
- [ ] 2.3 Implement naked invocation validation rejecting runs without targets, `-`, `--all`, or `--canon` with exit code 2 and remediation hint.
- [ ] 2.4 Implement single-canon inversion defaulting targets to `**/*` when `--canon` is provided without targets.
- [ ] 2.5 Resolve tiered ignore options (`--ignore`, `--default-ignores`, `--target-ignore`, `--canon-ignore`, etc.) and delegate to `@canon-clerk/core`'s `queryCanons`.

## 3. Presentation Formatters & Exit Codes
- [ ] 3.1 Implement `stylish` terminal formatter grouping matches by canon, displaying scope and triggering files with matched triggers.
- [ ] 3.2 Implement canonical `json` formatter emitting match records with 3-plane pattern attribution and empty array `[]` on zero matches.
- [ ] 3.3 Implement predicate mode (`--quiet` / `-q`), short-circuiting on the first match tuple and returning exit status 0 (triggered) vs 1 (none).
- [ ] 3.4 Ensure non-quiet mode exits with status 0 on zero matches per `cli-queries-must-exit-zero-on-empty-results`.

## 4. Verification & Testing
- [ ] 4.1 Unit tests in `packages/cli/src/commands/check-triggers.test.ts` covering option resolution, stdin ingestion, and error handling.
- [ ] 4.2 Integration tests in `packages/cli/src/commands/check-triggers.integration.test.ts` testing end-to-end execution, stylish output, JSON schema attribution, quiet mode predicate exit codes, naked invocation errors, and stdin piping.
- [ ] 4.3 Run full check suite (`npm run check`) and OpenSpec validation (`npm run opsx -- validate cli-check-triggers --strict`).
