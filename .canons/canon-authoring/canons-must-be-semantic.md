---
governs:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - canon-authoring
---
Each canon MUST enforce a semantic invariant that cannot be verified deterministically with static analysis, AST linters, or regular expressions.

Rationale: Deterministic tooling provides millisecond execution, zero token cost, and 100% reproducibility without hallucination risk; reserving LLM evaluation for semantic invariants optimizes CI efficiency.

**Guidance:** Migrate deterministic checks (such as syntax patterns, structural constraints, or naming conventions) into a linter, compiler rule, or script rather than an LLM review gate.
