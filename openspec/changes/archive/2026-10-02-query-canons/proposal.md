# Proposal

## Why
In Canon Clerk's evaluation cascade, callers (including coding agents, CLI query subcommands, and CI review gates) must answer the foundational query: *"Given these file paths (real or prospective), what are all the triggered canons?"* 

Following the completion of Stage 0 pure primitives (`checkFileInCanonScope`, `matchesTriggers` in #155) and unified discovery infrastructure (`GlobQueryOptions` in #157), the core engine requires an orchestration API that evaluates bipartite discovery domains (canons and target files) and streams relational match activations with zero intermediate memory buffering, explicit workspace scoping, and lazy parsing.

## What Changes
- Introduce `queryCanons(options: QueryCanonsOptions)` streaming evaluation generator in `@canon-clerk/core`.
- Provide symmetrical bipartite discovery inputs (`targetQuery` and `canonQuery`) accepting strings, string arrays, or full `GlobQueryOptions`.
- Resolve literal target paths deterministically without requiring filesystem existence (supporting prospective and imagined files).
- Stream atomic relational tuples (`QueryCanonsResult`) bundling full file provenance, parsed domain entities, and scope coordinates.
- Ensure zero I/O early filtering: discard out-of-scope targets before reading or parsing canon file contents from disk.
- Guarantee each triggered canon is read and parsed at most once per evaluation run.
- Enforce strict workspace boundaries, deterministic lexical emission, and zero reliance on ambient `process.cwd()`.

## Capabilities
### New Capabilities
- `query-canons`: Streams relational activation tuples between discovered or prospective codebase files and triggered repository canons across bipartite discovery domains.

### Modified Capabilities
None.

## Impact
- Adds `queryCanons`, `QueryCanonsOptions`, `QueryCanonsResult`, `CanonMatch`, `TargetMatch`, and `FileMatch` to `@canon-clerk/core` public exports.
- Consumes Stage 0 primitives (`checkFileInCanonScope`, `matchesTriggers`) and glob query utilities (`normalizeGlobQueryOptions`, `isPathIgnored`, `isPathMatch`, `getMatchedGlobs`).
- Provides the core evaluation engine required by `canon-clerk match` (Issue #158) and subsequent Stage 1 screening pipelines.
