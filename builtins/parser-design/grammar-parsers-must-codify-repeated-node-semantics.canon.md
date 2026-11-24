---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - parser-design
---
Structured markup and grammar AST parsers MUST explicitly codify cardinality and merge semantics (collection, concatenation, override, or syntax diagnostic) for repeated directives, attributes, or grammar productions.

Rationale: Anchored in formal compiler design (Aho, Lam, Sethi & Ullman, *Compilers*) and RFC 1122 (Postel's Robustness Principle), unspecified cardinality for repeated AST nodes causes silent last-write-wins data loss or nondeterministic tree construction.
