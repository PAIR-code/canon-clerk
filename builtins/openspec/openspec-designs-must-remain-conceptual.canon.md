---
triggers:
  - "openspec/changes/**/design.md"
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
OpenSpec design documents (`design.md`) and specifications (`spec.md`) MUST capture architectural decisions, conceptual models, and system trade-offs rather than specifying low-level implementation mechanics or language-specific code structures.

Precedent: Architecture Decision Record (ADR) conventions (Nygard); ANSI/SPARC conceptual-physical schema separation.

Rationale: Overspecifying code mechanics couples design rationale to implementation syntax and guarantees documentation drift.
