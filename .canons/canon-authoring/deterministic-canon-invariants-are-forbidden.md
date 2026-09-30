---
triggers:
  - "**/.canons/**/*.md"
tags:
  - canon-authoring
---
Canons MUST enforce semantic invariants; constraints verifiable via static analysis, AST linters, or regular expressions are forbidden.

Rationale: Deterministic tools execute in milliseconds at zero token cost; LLM evaluation is reserved for semantic judgment.
