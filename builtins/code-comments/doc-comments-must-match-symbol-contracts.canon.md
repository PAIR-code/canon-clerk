---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Symbol and API doc comments on functions, methods, types, fields, and modules MUST factually match the symbol's actual parameters, return values, error or panic conditions, and side effects.

Rationale: Following the Rust API Guidelines, Effective Go, and Google Style Guides, callers rely on doc comments as the authoritative interface contract; drifted doc comments induce integration bugs at call sites.
