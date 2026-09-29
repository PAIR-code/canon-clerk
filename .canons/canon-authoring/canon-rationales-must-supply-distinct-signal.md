---
triggers:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - canon-authoring
---
When declared, a `Rationale` directive MUST articulate an underlying engineering trade-off, architectural constraint, or failure mode (Chesterton's Fence) not self-evident from the invariant statement, rather than paraphrasing the rule.

Rationale: As codified in the Canon Format Specification (SPEC.md §5), reasoning models use rationales for semantic exegesis; restating the rule consumes tokens without supplying new latent signal.

**Guidance:** Articulate the underlying failure mode, performance trade-off, or historical context being addressed, or omit the directive if the rationale is already self-evident.
