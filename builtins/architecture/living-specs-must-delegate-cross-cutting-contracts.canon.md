---
triggers:
  - "docs/**/*.md"
  - "specs/**/*.md"
inspect:
  - diff
tags:
  - architecture
  - documentation
---
Normative specifications for cross-cutting contracts (shared options, default matrices, schemas, or protocols) MUST be single-sourced in an authoritative document and delegated via hyperlink from component specifications rather than duplicated inline.

Exception: A component specification MAY include a brief informative mention or component-specific override inline IFF it links to the authoritative document for the normative specification.

Rationale: Anchored in arc42 §8 (Crosscutting Concepts) and ISO/IEC Directives Part 2 (Normative References), separating normative definitions from informative component references prevents silent specification divergence across sibling documents.
