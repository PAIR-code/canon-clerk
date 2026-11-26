---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - parser-design
---
Parsers converting structured text into domain entities MUST decouple lexical tokenization from semantic AST construction via a two-stage pipeline.

Precedent: Classic compiler architecture (Aho, Lam, Sethi, Ullman, *Compilers: Principles, Techniques, and Tools*).

Rationale: Decoupling the lexical scanner from syntax parsing enables downstream linters and language servers to track source coordinates and preserve trivia without re-parsing.

**Remediation:** Introduce a pure lexical tokenizer that scans raw source into an ordered stream of typed tokens with line/column coordinates, and refactor the semantic parser to consume this token stream.
