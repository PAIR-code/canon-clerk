---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - parser-design
---
Structured markup and grammar AST parsers MUST explicitly codify normalization semantics for empty or whitespace-only directive and field payloads (such as omitting optional AST nodes or producing empty collections).

Exception: Syntax productions whose formal grammar assigns semantic significance to whitespace (such as fenced code blocks, indentation-sensitive blocks, or literal string tokens) MAY preserve raw whitespace IFF codified in the grammar specification.

Rationale: Anchored in formal compiler design (Aho, Lam, Sethi & Ullman, *Compilers*), unnormalized whitespace-only AST payloads leak phantom non-empty nodes into downstream semantic analyzers and evaluators.
