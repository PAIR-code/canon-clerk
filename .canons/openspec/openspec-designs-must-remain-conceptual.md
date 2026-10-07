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
OpenSpec design documents (`design.md`) and specifications (`spec.md`) MUST focus on architectural decisions, conceptual models, and system trade-offs, and MUST NOT specify low-level implementation mechanics or language-specific code structures.

Rationale: Grounded in Architecture Decision Record (ADR) conventions (Nygard) and the ANSI/SPARC conceptual-physical schema separation, design documents must capture architectural decisions and data models conceptually; overspecifying code mechanics couples design rationale to implementation syntax and guarantees documentation drift.
