---
triggers:
  - "**/*.canon.md"
inspect:
  - diff
tags:
  - canon-authoring
---
Each canon MUST articulate its underlying engineering failure mode or trade-off via an explicit `Rationale` directive.

Exception: A canon MAY omit a `Rationale` directive IFF the underlying failure mode or trade-off is self-evident from the invariant statement or an explicit `Precedent` directive.

Rationale: Evaluator and assistant models rely on author intent (Chesterton's Fence) to disambiguate edge cases and explain check failures; non-obvious invariants lacking rationales lead to brittle, literalist enforcement.
