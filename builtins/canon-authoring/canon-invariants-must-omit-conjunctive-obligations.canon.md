---
triggers:
  - "**/*.canon.md"
inspect:
  - diff
tags:
  - canon-authoring
---
Canon invariants, whether stated in the body or derived from the filename or heading when the body omits an invariant clause, MUST state a single obligation rather than conjoining independent obligations into one rule.

Exception: A canon invariant MAY use `and` IFF it connects parts of a single cohesive mechanism, noun phrase, or precondition rather than independent compliance obligations.

Precedent: ISO/IEC/IEEE 29148 (Singular Requirements); SPEC.md §2 (Atomic).

Rationale: The Caseload already composes separate canon files as a logical `AND`; bundling independent obligations into one file induces evaluation flakiness as models attend unevenly to subsets of the compound rule.
