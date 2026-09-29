---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - parser-design
  - compiler-architecture
---
Parsers converting structured text into domain entities MUST decouple lexical tokenization from semantic AST construction via a two-stage pipeline.

Rationale: Following classic compiler architecture (Aho, Lam, Sethi, Ullman), decoupling the lexical scanner from syntax parsing enables downstream linters and language servers to track source coordinates and preserve trivia without re-parsing.

**Remediation:** Introduce a pure lexical tokenizer that scans raw source into an ordered stream of typed tokens with line/column coordinates, and refactor the semantic parser to consume this token stream.
