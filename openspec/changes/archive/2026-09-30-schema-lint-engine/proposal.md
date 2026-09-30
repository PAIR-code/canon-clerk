# Proposal

## Why

Static linting is required to deterministically scan and catch syntactic, structural, and semantic issues in `.canons/` directories prior to expensive Stage 1/2 LLM evaluation cascades (Issue #36). To preserve architectural boundaries and comply with the project purity canon (`schema-must-be-pure-and-free-of-io`), all AST- and token-level validation must reside purely within `@canon-clerk/schema` with zero Node.js I/O or filesystem side-effects.

## What Changes

- Define core static linting types in `@canon-clerk/schema`: `DiagnosticSeverity` (`'error' | 'warning'`), `RuleSeverity` (`'off' | DiagnosticSeverity`), `CanonDiagnostic`, `RuleConfig`, `LintCanonOptions`, `RuleContext`, and `CanonLintRule`.
- Implement an immutable, lazily evaluated `RuleContext` class that provides memoized getters for `tokens`, `frontmatterToken`, `frontmatterDoc`, `frontmatterLineCounter`, `rawFrontmatter`, `fileName`, and `fileStem`, with defensively frozen token arrays to prevent cross-rule mutations.
- Implement a pure, fault-tolerant execution runner `lintCanon()` that orchestrates rule execution without unhandled exceptions, applies severity configuration overrides, and deterministically sorts diagnostics by source line, column, and rule code.
- Export linting interfaces and functions from the `@canon-clerk/schema` package entrypoint.

## Capabilities

### New Capabilities
- `canon-linter`: Static canon linting engine, immutable evaluation context (`RuleContext`), rule interfaces, severity configuration resolution, and pure execution runner (`lintCanon`).

### Modified Capabilities

## Impact

- `@canon-clerk/schema`: Adds pure in-memory linting types, `RuleContext`, and `lintCanon()` entrypoint.
- Downstream packages (`@canon-clerk/core`, `@canon-clerk/cli`): Establishes the in-memory execution pipeline and contract consumed by filesystem traversers, `.canonlintrc.json` hierarchy loaders, and CLI lint commands in follow-on issues.
