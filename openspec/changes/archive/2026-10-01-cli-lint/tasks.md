# Tasks

## 1. CLI Router & Entrypoint Infrastructure
- [x] 1.1 Implement subcommand dispatcher and entrypoint harness in `packages/cli/src/cli.ts` supporting `lint`, `--help`, and `--version`.
- [x] 1.2 Register process signal traps for `SIGINT` (exit 130) and `SIGTERM` (exit 143) with graceful exit.
- [x] 1.3 Implement standardized error handling emitting usage guidance and remediations to `stderr` with exit code 2.

## 2. Argument Parsing & Core Delegation
- [x] 2.1 Implement `lint` command and option definitions using `commander` supporting targets, `-g, --glob`, `--stdin-filename`, `--format, -f`, `--json`, `--quiet, -q`, `--max-warnings`, `--ignore`, and `--default-ignores`.
- [x] 2.2 Implement `stdin` Markdown content ingestion when `-` is supplied as a target operand (evaluated via `lintCanon` with optional `--stdin-filename`).
- [x] 2.3 Implement environment variable fallback resolution (`CANON_CLERK_FORMAT`, `CANON_CLERK_MAX_WARNINGS`).
- [x] 2.4 Delegate discovery and evaluation to `@canon-clerk/core`'s `lintCanons()`, streaming results in real time with lexicographical ordering inherited from core.

## 3. Presentation Formatters & Exit Code Contracts
- [x] 3.1 Implement `stylish` terminal formatter using `node:util.styleText` with TTY detection, `NO_COLOR` support, column alignment, severity badges, indented remediation hints, and clean workspace summary.
- [x] 3.2 Implement canonical `json` formatter emitting flat array of `FileLintResult` containing only files with diagnostics to stdout (emitting `[]` on clean workspace).
- [x] 3.3 Implement deterministic exit code resolver evaluating total errors, warnings, `--quiet`, and `--max-warnings`.
- [x] 3.4 Implement categorized help screens with runnable examples per CLI help canons.

## 4. Verification, Testing & Dogfooding
- [x] 4.1 Add unit tests for argument parsing, option precedence, formatters, and exit code calculations.
- [x] 4.2 Add CLI integration tests in `packages/cli/src/cli.integration.test.ts` testing clean runs, violation reporting, JSON payloads, quiet mode, max warnings ratchet, stdin piping, syntax errors, and signal traps.
- [x] 4.3 Add `"lint:canons": "canon-clerk lint"` to root `package.json` and wire into `npm run check`.
- [x] 4.4 Verify all checks pass cleanly (`npm run check` and `npm run opsx -- validate --all --strict`).
