# Proposal

## Why

Static linting of canons in `@canon-clerk/schema` provides in-memory validation via `lintCanon()`, but developers and automated pipelines currently have no domain service to discover canon files on disk or orchestrate workspace-wide lint execution (Issue #132). In accordance with the presentation delegation canon (`cli-must-delegate-domain-queries-to-core`), filesystem traversal, canon discovery, and linting execution MUST reside in `@canon-clerk/core` rather than being implemented inside presentation adapters like the CLI or GitHub Action.

## What Changes

- Implement `discoverCanonFiles(workspaceRoot, options?)` in `@canon-clerk/core` to recursively discover `.canons/**/*.md` files across workspace root and package subdirectories while skipping `node_modules`, `dist`, `.bare`, and hidden VCS directories.
- Implement `lintWorkspace(workspaceRoot, options?)` in `@canon-clerk/core` to read discovered canon files, execute `@canon-clerk/schema`'s `lintCanon()` with `DEFAULT_RULES`, and aggregate diagnostics into structured summary records.
- Define and export structured, presentation-agnostic result types: `FileLintResult`, `WorkspaceLintResult`, `DiscoverCanonsOptions`, and `LintWorkspaceOptions`.
- Export discovery and workspace linting functions and types from `@canon-clerk/core`'s entrypoint.
- Add comprehensive unit test coverage with fixture directories testing clean canons, rule violations, missing files, and path filtering.

## Capabilities

### New Capabilities
- `canon-discovery`: Filesystem discovery of canon markdown files across workspace root and package subdirectories, respecting exclusions and path filters.

### Modified Capabilities
- `canon-linter`: Extends static linting with workspace-wide orchestration (`lintWorkspace`) in `@canon-clerk/core` and structured diagnostic aggregation (`FileLintResult`, `WorkspaceLintResult`).

## Impact

- `@canon-clerk/core`: Expands core API to include canon filesystem discovery and workspace linting orchestration.
- Consumers (`@canon-clerk/cli`, `@canon-clerk/action`): Unblocks Issue #140 (`feat(cli): add lint subcommand and terminal formatters`) to consume uniform domain services without implementing filesystem traversal or execution logic.
