---
triggers:
  - "docs/**/*.md"
  - "specs/**/*.md"
inspect:
  - diff
tags:
  - documentation
---
References to domain concepts, components, or architectural tracks in living specification documents MUST use a single canonical term consistently across the document and its linked canonical labels rather than introducing ad-hoc synonyms.

Exception: A living specification MAY introduce a secondary term IFF explicitly defining a formal alias, an external industry mapping, or a user-facing wire or CLI identifier distinct from the internal concept name.

Precedent: Domain-Driven Design *Ubiquitous Language* (Evans); ISO/IEC Directives Part 2 (Consistent Terminology).

Rationale: When sibling specifications rely on concept-name cross-references rather than inline member lists, ad-hoc synonyms break lexical search and cause AI coding agents to treat one architectural construct as multiple distinct entities.
