---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - parser-design
---
Fault-tolerant structured markup and grammar AST parsers MUST degrade gracefully on missing fields or wrongly-typed AST node payloads, emitting structured diagnostics with source spans while preserving raw input tokens rather than panicking or silently discarding malformed nodes.

Exception: Strict fail-fast parsers for binary wire protocols or security-critical schemas MAY abort on the first syntax violation IFF returning a structured parse error with source coordinates.

Rationale: Following resilient parsing principles (Language Server Protocol, tree-sitter, and Aho et al., *Compilers*), error recovery with preserved source spans enables downstream validators and editors to report multiple diagnostics in a single pass.
