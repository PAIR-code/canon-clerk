---
triggers:
  - "**/*.canon.md"
inspect:
  - diff
tags:
  - canon-authoring
---
Normative canon invariant clauses MUST omit inexhaustive parenthetical exemplar lists, relying instead on precise domain terminology to define scope.

Exception: An invariant clause MAY enumerate concrete tokens IFF those tokens constitute a closed or canonical syntactic set governed directly by the rule, and `Exception` or `Remediation` directives MAY include concrete examples IFF required to specify verifiable conditions or actionable templates.

Rationale: Forward-pass relevance discriminators can fixate on enumerated exemplars as a soft closed set (inducing false negatives on unlisted instances) while lexical overlap with listed examples primes false-positive activation on unrelated diffs.

**Remediation:** Replace the parenthetical with a precise domain term if the examples instantiate one cohesive concept, or decompose the rule into disjoint, construct- or language-scoped atomic canons if the examples span distinct syntactic mechanisms.
