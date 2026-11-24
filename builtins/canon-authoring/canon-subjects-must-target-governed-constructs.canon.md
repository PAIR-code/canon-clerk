---
triggers:
  - "**/*.canon.md"
inspect:
  - diff
tags:
  - canon-authoring
---
A canon invariant's grammatical subject MUST name the specific domain construct or modification action it governs rather than an enclosing container (such as whole packages, modules, or CLI commands) or umbrella category.

Rationale: During heuristic relevance screening, evaluator models determine jurisdiction from the invariant's grammatical subject; framing a construct-specific rule around its enclosing container or an umbrella noun induces false-positive applicability and boundary flips whenever unrelated changes touch the same container.

**Remediation:** Rewrite the opening subject clause to name the exact construct or change operation under governance (e.g. `When introducing or modifying a CLI file-input operand...` or `Refactors that rename internal domain concepts in 0.y.z packages...` rather than `CLI commands that accept file operands...` or `Packages with major version zero...`).
