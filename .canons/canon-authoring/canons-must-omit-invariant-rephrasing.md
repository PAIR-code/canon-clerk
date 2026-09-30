---
triggers:
  - "**/.canons/**/*.md"
tags:
  - canon-authoring
---
A canon Markdown body MUST NOT repeat or paraphrase the invariant established by its filename or heading unless introducing qualifying constraints.

Rationale: Per SPEC.md §4.2.7, Canon Clerk automatically derives the normative invariant from the filename; echoing the title in the body consumes prompt tokens without adding new semantic signal.
