---
triggers:
  - "openspec/changes/**/specs/**"
  - "openspec/specs/**"
exists:
  - "openspec/**"
inspect:
  - diff
tags:
  - openspec
  - architecture
---
OpenSpec capabilities under `openspec/specs/` MUST represent cohesive domain behaviors or entities rather than mirroring physical code package or directory names.

Precedent: Domain-Driven Design Bounded Contexts (Evans); Living Documentation (Martraire).

Rationale: Mapping specifications to ephemeral code packaging couples living contracts to physical refactors and obscures enduring domain boundaries.
