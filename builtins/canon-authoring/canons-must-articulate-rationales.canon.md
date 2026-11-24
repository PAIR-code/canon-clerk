---
triggers:
  - "**/*.canon.md"
inspect:
  - diff
tags:
  - canon-authoring
---
Each canon MUST articulate its underlying engineering failure mode or trade-off via an explicit `Rationale` directive.

Exception: A canon MAY omit a `Rationale` directive IFF the invariant statement itself explicitly names the prevented failure mode, trade-off, or governing external standard.

Rationale: Evaluator and assistant models rely on author intent (Chesterton's Fence) to disambiguate edge cases and explain check failures; non-obvious invariants lacking rationales lead to brittle, literalist enforcement.
