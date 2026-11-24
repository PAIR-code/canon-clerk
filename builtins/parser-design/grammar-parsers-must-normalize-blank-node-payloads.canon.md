---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - parser-design
---
AST directive and field payload normalization rules in structured markup parsers MUST explicitly codify semantics for empty or whitespace-only payloads.

Exception: Syntax productions whose formal grammar assigns semantic significance to whitespace (such as fenced code blocks, indentation-sensitive blocks, or literal string tokens) MAY preserve raw whitespace IFF codified in the grammar specification.

Rationale: Anchored in formal compiler design (Aho, Lam, Sethi & Ullman, *Compilers*), unnormalized whitespace-only AST payloads leak phantom non-empty nodes into downstream semantic analyzers and evaluators.
