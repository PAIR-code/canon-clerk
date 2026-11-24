---
triggers:
  - "docs/**/*.md"
  - "specs/**/*.md"
inspect:
  - diff
tags:
  - architecture
---
Wire formats, syntaxes, CLI conventions, configuration schemas, and protocols introduced in architecture specifications MUST align with and cite the governing industry standard or established precedent.

Exception: A specification MAY introduce a bespoke format or convention IFF it explicitly documents why existing industry standards cannot satisfy the stated architectural requirements.

Precedent: ISO/IEC Directives Part 2 (Normative References); RFC 2119.
