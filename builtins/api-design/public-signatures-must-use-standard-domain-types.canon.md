---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - api-design
---
Following "Parse, Don't Validate" (King), Martin Fowler's Primitive Obsession, and the Rust API Guidelines (`C-NEWTYPE`), public function signatures MUST use standard-library domain types or closed enums rather than raw primitives when the target language's standard library provides the domain type.

Exception: Wire-protocol, FFI, or CLI serialization boundary types MAY use raw primitives IFF the external schema or ABI requires primitive representations.
