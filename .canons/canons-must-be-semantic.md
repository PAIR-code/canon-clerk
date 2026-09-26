---
paths:
  - "**/.canons/**/*.md"
inspect:
  - diff
---
Each canon MUST enforce a semantic invariant that cannot be verified deterministically with static analysis, AST linters, or regular expressions.

Rationale: Invariants that can be validated with regular expressions, compilers, or linters SHOULD be enforced by deterministic tooling. Canons are reserved for semantic invariants requiring contextual comprehension.