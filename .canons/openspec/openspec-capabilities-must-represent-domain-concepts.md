---
triggers:
  - "openspec/specs/**"
  - "openspec/changes/**/specs/**"
inspect:
  - diff
tags:
  - openspec
  - architecture
---
OpenSpec capabilities under `openspec/specs/` MUST represent cohesive domain behaviors or entities rather than mirroring physical code package or directory names.

Rationale: Following Domain-Driven Design (Evans) Bounded Contexts and Living Documentation principles (Martraire), specifications must map to enduring domain capabilities rather than ephemeral code packaging.
