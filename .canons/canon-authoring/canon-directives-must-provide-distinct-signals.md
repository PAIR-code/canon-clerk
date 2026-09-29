---
triggers:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - canon-authoring
---
Each directive in a canon (`Exception`, `Rationale`, `Guidance`, `Supplement`) MUST provide distinct, non-redundant signal tailored to its target consumer rather than paraphrasing the invariant rule.

Rationale: Canon components serve distinct cognitive roles for the evaluator and assistant models; echoing the invariant across directives consumes context tokens without supplying new semantic information.

**Guidance:** Ensure `Exception` specifies verifiable deviation criteria, `Rationale` articulates the underlying engineering trade-off (Chesterton's Fence), and `Guidance` provides actionable remediation steps.
