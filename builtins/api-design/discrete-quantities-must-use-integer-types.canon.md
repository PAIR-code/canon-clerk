---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - api-design
---
Function parameters, return types, and struct fields representing inherently discrete quantities (counts, indices, retries, capacities, or discrete identifiers) MUST use integer types rather than floating-point types (`f32`, `f64`, `float`, `double`).

Exception: Languages lacking a distinct standard integer numeric type (such as JavaScript or TypeScript `number` where `bigint` is impractical) or JSON/IEEE 754 wire-serialization boundaries MAY use floating-point numeric types IFF constrained by the host language or external wire schema.

Rationale: Per IEEE 754 floating-point semantics, *Effective Java* (Bloch), and *Code Complete* (McConnell), floating-point representations permit fractional values, `NaN`, infinities, and precision loss on large integers, expanding the domain beyond valid discrete states.
