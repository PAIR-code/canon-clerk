---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - parser-design
---
Grammar productions and AST node builders handling repeatable directives or attributes MUST explicitly codify cardinality and merge semantics.

Precedent: Aho, Lam, Sethi & Ullman (*Compilers*); RFC 1122 (Postel's Robustness Principle).
