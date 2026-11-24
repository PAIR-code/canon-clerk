---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - api-design
---
Public function signatures MUST use standard-library domain types rather than raw primitives when the target language's standard library provides the domain type.

Exception: Wire-protocol, FFI, or CLI serialization boundary types MAY use raw primitives IFF the external schema or ABI requires primitive representations.

Precedent: "Parse, Don't Validate" (King); Martin Fowler (*Refactoring*, Primitive Obsession).
