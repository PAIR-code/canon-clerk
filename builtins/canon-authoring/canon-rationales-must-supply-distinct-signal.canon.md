---
triggers:
  - "**/*.canon.md"
inspect:
  - diff
tags:
  - canon-authoring
---
When declared, a `Rationale` directive MUST articulate an underlying engineering trade-off, architectural constraint, or failure mode (Chesterton's Fence) not self-evident from the invariant statement or `Precedent` directive, rather than paraphrasing the rule or citing external standards.

Precedent: Canon Format Specification (§5); Chesterton's Fence.

Rationale: Reasoning models use rationales for semantic exegesis; restating the invariant or duplicating citations that belong in `Precedent` consumes tokens without supplying new latent signal.

**Remediation:** Articulate the non-obvious failure mode, trade-off, or architectural constraint, move authoritative citations to `Precedent:`, or omit `Rationale:` when the reasoning is already self-evident.
