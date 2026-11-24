---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - parser-design
---
Anchored in formal compiler design (Aho, Lam, Sethi & Ullman, *Compilers*) and RFC 1122 (Postel's Robustness Principle), grammar productions and AST node builders handling repeatable directives or attributes MUST explicitly codify cardinality and merge semantics.
