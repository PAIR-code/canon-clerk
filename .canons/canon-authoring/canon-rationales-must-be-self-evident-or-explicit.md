---
triggers:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - canon-authoring
---
The engineering rationale for each canon invariant MUST either be self-evident from the rule statement or explicitly articulated via a `Rationale` directive.

Rationale: Evaluator and assistant models require comprehension of author intent (Chesterton's Fence) to disambiguate edge cases and explain check failures; non-obvious invariants lacking rationales lead to brittle, literalist enforcement.

**Guidance:** If the reason for this invariant is domain-specific, counter-intuitive, or subtle, add an explicit `Rationale` directive articulating the underlying failure mode or trade-off. If the rationale is self-evident, omit the directive rather than writing tautological explanations.
