---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - api-design
---
Function parameters, return types, and struct fields representing inherently discrete quantities MUST use integer types rather than floating-point types.

Exception: Languages lacking a distinct standard integer numeric type (such as JavaScript or TypeScript `number` where `bigint` is impractical) or JSON/IEEE 754 wire-serialization boundaries MAY use floating-point numeric types IFF constrained by the host language or external wire schema.

Precedent: IEEE 754 floating-point semantics; *Code Complete* (McConnell).
