---
governs:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - universal
  - canon-authoring
---
Each canon MUST only address a single, cohesive concept for its invariant.

Rationale: Multi-rule canon files increase the likelihood of evaluation flakiness, as subsequent model evaluations may focus on different subsets of the rule set.

**Guidance:** Recommend splitting multi-invariant canons into individual, atomic canons.
