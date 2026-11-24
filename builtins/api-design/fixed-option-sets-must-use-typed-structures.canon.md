---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - api-design
---
Public functions accepting a known, finite set of configuration options MUST declare a typed options struct, interface, or named parameters rather than an untyped map or property bag (`**kwargs`, `Record<string, any>`, `map[string]any`).

Exception: Transparent passthrough wrappers or decorators MAY accept untyped keyword maps IFF forwarding arbitrary arguments untouched to an underlying API, or when modeling genuinely open-ended user metadata dictionaries.

Rationale: Following "Parse, Don't Validate" (King) and Google Style Guides, untyped property bags defeat static type checking, hide typos until runtime, and obscure the accepted option schema from callers and IDE tooling.
