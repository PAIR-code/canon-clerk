# Proposal

## Why

Static linting of canons in `@canon-clerk/schema` provides in-memory validation via `lintCanon()`, but developers and automated pipelines currently have no domain service to discover canon files on disk or orchestrate workspace-wide lint execution (Issue #132). In accordance with the presentation delegation canon (`cli-must-delegate-domain-queries-to-core`), filesystem traversal, canon discovery, and linting execution MUST reside in `@canon-clerk/core` rather than being implemented inside presentation adapters like the CLI or GitHub Action.

## What Changes

- Implement streaming `lintWorkspace(workspaceRoot, options?)` in `@canon-clerk/core` as an async generator yielding `FileLintResult` records as canon files are discovered, read, and evaluated.
- Support default noise directory pruning (`DEFAULT_IGNORES`), custom ignores (`ignores`) with standard `.gitignore` semantics via the `ignore` package, and target filtering with explicit target precedence.
- Define and export structured, presentation-agnostic result and option types: `FileLintResult` and `LintWorkspaceOptions`.
- Export `lintWorkspace`, `LintWorkspaceOptions`, `FileLintResult`, `DEFAULT_IGNORES`, `toPosixPath`, and `CORE_VERSION` from `@canon-clerk/core`'s entrypoint.
- Add comprehensive unit test coverage with fixture directories testing streaming evaluation, clean canons, rule violations, missing files, and path filtering.

## Capabilities

### New Capabilities
- `canon-linter/workspace`: Defines streaming workspace canon linting orchestration (`lintWorkspace`) in `@canon-clerk/core`, filesystem traversal, ignore resolution adhering to `.gitignore` semantics, and presentation-agnostic `FileLintResult` records.

## Impact

- `@canon-clerk/core`: Expands core API to provide streaming workspace linting orchestration.
- Consumers (`@canon-clerk/cli`, `@canon-clerk/action`): Unblocks Issue #140 (`feat(cli): add lint subcommand and terminal formatters`) to consume uniform domain streaming services without implementing filesystem traversal or execution logic.

