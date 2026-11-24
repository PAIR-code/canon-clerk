---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - parser-design
---
Error-recovery paths in fault-tolerant structured markup and grammar AST parsers MUST degrade gracefully on missing fields or wrongly-typed AST node payloads, emitting structured diagnostics with source spans while preserving raw input tokens rather than panicking or silently discarding malformed nodes.

Exception: Strict fail-fast parsers for binary wire protocols or security-critical schemas MAY abort on the first syntax violation IFF returning a structured parse error with source coordinates.

Precedent: Language Server Protocol (LSP); tree-sitter resilient parsing; Aho, Lam, Sethi & Ullman (*Compilers*).
