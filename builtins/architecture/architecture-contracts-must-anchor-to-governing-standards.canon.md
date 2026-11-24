---
triggers:
  - "docs/**/*.md"
  - "specs/**/*.md"
inspect:
  - diff
tags:
  - architecture
---
When an architecture document or specification introduces a wire format, syntax, CLI convention, configuration schema, or protocol, it MUST align with and cite the governing industry standard or established precedent.

Exception: A specification MAY introduce a bespoke format or convention IFF it explicitly documents why existing industry standards cannot satisfy the stated architectural requirements.

Rationale: Following ISO/IEC Directives Part 2 (Normative References) and RFC 2119 specification practice, anchoring design contracts to established standards (RFCs, POSIX, XDG, SemVer, clig.dev) prevents ad-hoc reinvention and gives human and AI implementers an unambiguous behavioral reference.
