---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
`SAFETY:`, `INVARIANT:`, and concurrency or locking justification comments MUST explain why the required precondition holds given the surrounding code, rather than merely asserting safety or restating the operation.

Rationale: Following the Rustonomicon and Rust standard library safety comment guidelines, deterministic linters verify only that a safety comment exists, not that its soundness argument is grounded in the surrounding invariants.
