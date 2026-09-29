---
triggers:
  - "SPEC.md"
  - "docs/architecture.md"
references:
  - "SPEC.md"
  - "docs/architecture.md"
inspect:
  - diff
tags:
  - internal
  - repo-governance
---
`docs/architecture.md` MUST define execution cascade mechanics, directive roles, and CI verdict contracts that strictly align with the normative definitions in `SPEC.md`.

Rationale: `SPEC.md` delegates runtime evaluation architecture to `docs/architecture.md`; discrepancies between specification grammar and architectural execution contracts introduce ambiguous CI semantics.
