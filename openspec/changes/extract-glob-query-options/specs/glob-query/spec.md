# Spec Delta

## Purpose

Defines flexible filesystem discovery configuration, default noise directory ignores, pattern normalization, and path filtering predicates across query domains in `@canon-clerk/core`.

## ADDED Requirements

### Requirement: Default Repository Noise Ignores
The system SHALL export a frozen array `DEFAULT_IGNORES` containing standard repository noise directories, including package manager dependencies and version control directories such as `node_modules` and `.git`.

#### Scenario: Inspecting default ignores
- **WHEN** inspecting `DEFAULT_IGNORES` from `@canon-clerk/core`
- **THEN** it is a frozen array of strings containing standard noise patterns including `node_modules` and `.git`

### Requirement: Flexible Discovery Option Normalization
The system SHALL provide `normalizeGlobQueryOptions` to normalize string, string array, or `GlobQueryOptions` inputs into `NormalizedGlobQueryOptions` containing concrete `globs` and `ignores` arrays. Unless `defaultIgnores` is explicitly `false`, active ignore rules SHALL include `DEFAULT_IGNORES` prepended to custom ignore patterns.

#### Scenario: Normalizing undefined input with fallback globs
- **WHEN** calling `normalizeGlobQueryOptions(undefined, ['**/.canons/**/*.md'])`
- **THEN** `globs` equals `['**/.canons/**/*.md']` and `ignores` contains `DEFAULT_IGNORES`

#### Scenario: Normalizing string input
- **WHEN** calling `normalizeGlobQueryOptions('src/**/*.ts')`
- **THEN** `globs` equals `['src/**/*.ts']` and `ignores` contains `DEFAULT_IGNORES`

#### Scenario: Disabling default ignores
- **WHEN** calling `normalizeGlobQueryOptions({ defaultIgnores: false, ignores: ['custom/**'] })`
- **THEN** `ignores` contains only `['custom/**']` without `DEFAULT_IGNORES`

### Requirement: Path Ignore Evaluation
The system SHALL provide `isPathIgnored(path, options, workspaceRoot?)` returning true if a relative path or directory matches any active ignore pattern adhering to `.gitignore` syntax semantics.

#### Scenario: Path inside ignored directory
- **WHEN** evaluating `isPathIgnored('node_modules/foo/index.js', options)` where default ignores are active
- **THEN** returns `true`

#### Scenario: Unignored path
- **WHEN** evaluating `isPathIgnored('packages/core/src/index.ts', options)`
- **THEN** returns `false`

### Requirement: Path Pattern Matching Predicate
The system SHALL provide `isPathMatch(path, options, workspaceRoot?)` returning true if and only if the path matches at least one glob pattern in `options.globs` AND matches zero ignore patterns in `options.ignores`.

#### Scenario: Matching glob and not ignored
- **WHEN** evaluating `isPathMatch('.canons/pr-rules.md', options)` where globs match and ignores do not
- **THEN** returns `true`

#### Scenario: Matching glob but ignored
- **WHEN** evaluating `isPathMatch('node_modules/.canons/foo.md', options)` where default ignores are active
- **THEN** returns `false`

#### Scenario: Not matching glob
- **WHEN** evaluating `isPathMatch('src/index.ts', options)` where globs only select markdown
- **THEN** returns `false`

### Requirement: Matched Glob Attribution
The system SHALL provide `getMatchedGlobs(path, options, workspaceRoot?)` returning all glob patterns from `options.globs` that match the path, or an empty array if the path is ignored or matches no globs.

#### Scenario: Path matching multiple globs
- **WHEN** a path matches two configured glob patterns and is not ignored
- **THEN** returns both matching glob patterns

#### Scenario: Ignored path matching globs
- **WHEN** an ignored path matches a glob pattern
- **THEN** returns an empty array
