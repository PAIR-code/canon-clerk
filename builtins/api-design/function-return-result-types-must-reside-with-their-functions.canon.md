---
triggers:
  - "**/src/**/*.ts"
inspect:
  - diff
tags:
  - api-design
  - typescript
---
A bespoke Result or Output type representing the return value of a specific function MUST be declared and exported from that function's module rather than an external types module.

Exception: Return types MAY reside in a shared domain module IFF they represent domain entities or value objects consumed across multiple independent operations.

Precedent: Common Closure Principle (Martin).

Rationale: Isolating operation results in external type files produces asymmetric, fractured function signatures and creates unorganized types sinkholes.
