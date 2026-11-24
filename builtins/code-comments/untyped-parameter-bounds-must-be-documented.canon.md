---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
When a public function enforces runtime domain bounds or sentinel meanings on a primitive parameter that are not encoded in its type (such as probabilities in `[0.0, 1.0]`, `timeout_ms = 0` meaning unlimited, or positive-only counts), its doc comment MUST state those enforced bounds or sentinel semantics.

Rationale: Per *Design by Contract* (Meyer), *Code Complete* (McConnell), and Google Style Guides, unenforced type-level invariants remain invisible to callers until runtime failures or unexpected sentinel behavior occur.
