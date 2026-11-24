---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
`SAFETY:`, `INVARIANT:`, and concurrency or locking justification comments MUST explain why the required precondition holds given the surrounding code, rather than merely asserting safety or restating the operation.

Precedent: The Rustonomicon; Rust standard library safety comment guidelines.
