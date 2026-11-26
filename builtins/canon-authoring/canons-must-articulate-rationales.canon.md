---
triggers:
  - "**/*.canon.md"
inspect:
  - diff
tags:
  - canon-authoring
---
Each canon MUST articulate its underlying engineering failure mode or trade-off via an explicit `Rationale` directive.

Exception: A canon MAY omit a `Rationale` directive IFF the invariant statement explicitly names the prevented failure mode or trade-off, or an explicit `Precedent` directive is present.

Precedent: Canon Format Specification (§5); Chesterton's Fence.

Rationale: Evaluator and assistant models rely on author intent to disambiguate edge cases and explain check failures; non-obvious invariants lacking rationales lead to brittle, literalist enforcement.
