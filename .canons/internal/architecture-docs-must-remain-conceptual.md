---
triggers:
  - "docs/architecture/**/*.md"
exists:
  - "docs/architecture/**"
inspect:
  - diff
tags:
  - internal
  - architecture
  - repo-governance
---
Architecture documentation under `docs/architecture/**` MUST express domain concepts, behavioral invariants, and conceptual data plane constraints in prose, tables, or diagrams, and MUST NOT specify low-level implementation mechanics, pseudo-code type definitions, or function signature blocks.

Rationale: Grounded in Architecture Decision Record (ADR) conventions (Nygard) and the ANSI/SPARC conceptual-physical schema separation, living architecture documents must capture domain contracts and system invariants conceptually; embedding pseudo-structs or function signatures couples architectural specifications to physical code layouts and guarantees documentation drift.
