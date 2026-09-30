# Design

## Context

The lexical scanner `tokenizeCanon()` exists in `@canon-clerk/schema/src/lexer.ts`. Per project canon `schema-must-be-pure-and-free-of-io`, `@canon-clerk/schema` must operate strictly in-memory with zero Node.js I/O or filesystem dependencies (`node:fs`, `node:path`).

This design establishes the static linting foundation: an immutable, lazily evaluated `RuleContext`, standard rule and diagnostic types, and a pure fault-tolerant runner `lintCanon()`.

## Goals / Non-Goals

**Goals:**
- Provide standard TypeScript type definitions for lint rules, rule contexts, severities, and diagnostics following ESLint flat conventions.
- Implement an immutable `RuleContext` with lazy evaluation and memoization so rules inspecting only body tokens incur zero YAML parsing overhead.
- Ensure cross-rule isolation by defensively freezing shared token collections (`Object.freeze`).
- Implement pure path parsing for `fileName` and `fileStem` without importing `node:path`.
- Implement a fault-tolerant runner `lintCanon()` that catches runtime exceptions gracefully, applies `ruleConfig` overrides (`off`, `warning`, `error`), and sorts diagnostics deterministically.

**Non-Goals:**
- Filesystem traversal, directory scanning, and `.canonlintrc.json` hierarchy resolution (deferred to `@canon-clerk/core`).
- CLI commands (`canon-clerk lint`), terminal reporters, `--max-warnings`, and exit codes (deferred to `@canon-clerk/cli`).
- Implementing concrete lint rules (deferred to follow-on issues: #117 for frontmatter schema, #118 for directives, #119 for markdown/references).

## Decisions

### Decision 1: Pure in-memory path derivation without `node:path`
- **Choice**: Parse `fileName` and `fileStem` using string manipulation and regular expressions rather than importing `node:path`.
- **Rationale**: Keeps `@canon-clerk/schema` completely pure and environment-agnostic (browser- and edge-safe), complying with `schema-must-be-pure-and-free-of-io`.
- **Alternatives considered**: Importing `node:path` or `pathe` (rejected to maintain zero runtime Node.js dependencies in schema).

### Decision 2: Lazy evaluation with memoization for `RuleContext`
- **Choice**: Implement getters for `tokens`, `frontmatterToken`, `frontmatterDoc`, `frontmatterLineCounter`, `rawFrontmatter`, `fileName`, and `fileStem` that compute values on first read and cache them.
- **Rationale**: Lint suites may contain dozens of rules. Rules that only inspect source lines or markdown token streams should not pay the CPU or memory cost of parsing YAML documents or CST line counters.
- **Alternatives considered**: Eagerly computing all representations at constructor time (rejected due to unnecessary overhead on token-only evaluations).

### Decision 3: Defensive freezing of token collections
- **Choice**: `Object.freeze` the token array when evaluated.
- **Rationale**: A single `RuleContext` instance is shared across all active rules for a file. Defensively freezing prevents any buggy or misbehaving rule from mutating the token sequence for subsequent rules.
- **Alternatives considered**: Cloning tokens for each rule (rejected due to excessive memory allocations).

### Decision 4: Two-tier severity model with configuration overrides
- **Choice**: `DiagnosticSeverity = 'error' | 'warning'` and `RuleSeverity = 'off' | DiagnosticSeverity`.
- **Rationale**: Simple, deterministic, and aligns with ESLint core conventions.
- **Evaluation pipeline**:
  - If a rule is configured as `'off'`, it is skipped entirely during execution.
  - If a rule has an explicit `'warning'` or `'error'` override, any diagnostics emitted by that rule have their `severity` field remapped to the configured level.

### Decision 5: Fault-tolerant execution runner
- **Choice**: `lintCanon()` wraps rule execution in `try / catch` blocks.
- **Rationale**: Unlike `parseCanon()`, which throws fatal errors on malformed syntax, the linter must be fault-tolerant. Malformed documents or unexpected rule exceptions are reported as structured `CanonDiagnostic` objects.
- **Alternatives considered**: Letting rule exceptions bubble up (rejected because a linter must report as many diagnostics as possible without crashing).

### Decision 6: Deterministic diagnostic sorting
- **Choice**: Sort diagnostics ascending by `line` (`(a.line ?? 0) - (b.line ?? 0)`), then `column` (`(a.column ?? 0) - (b.column ?? 0)`), then rule identifier `code` (`a.code.localeCompare(b.code)`).
- **Rationale**: Guarantees reproducible, deterministic output across test suites and CI runs regardless of rule execution order.

## Risks / Trade-offs

- **[Risk: YAML CST parsing errors during lazy evaluation]** → **Mitigation**: `frontmatterDoc` and `rawFrontmatter` getters catch YAML parse errors and return `undefined` or CST error nodes rather than throwing unhandled exceptions. Concrete frontmatter syntax rules (Issue #117) will explicitly validate syntax and report user-friendly errors.
- **[Risk: Excessive memory retention in batch linting]** → **Mitigation**: `RuleContext` instances are scoped to single-file runs and released when `lintCanon()` returns.
