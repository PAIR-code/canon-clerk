---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - api-design
---
Fixed configuration-option parameter sets on public APIs MUST be declared as a typed options structure, interface, or named parameters rather than an untyped map or property bag.

Exception: Transparent passthrough wrappers or decorators MAY accept untyped keyword maps IFF forwarding arbitrary arguments untouched to an underlying API, or when modeling genuinely open-ended user metadata dictionaries.

Precedent: "Parse, Don't Validate" (King); Google Style Guides.
