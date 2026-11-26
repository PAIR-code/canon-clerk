---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - api-design
---
Intermediate stages of an execution pipeline MUST NOT be exposed as public domain exports unless they have distinct, documented external consumers beyond the orchestrator itself.

Exception: Low-level utility packages MAY export atomic operations IFF those primitives are documented and tested as standalone public utilities independent of any enclosing pipeline orchestrator.

Precedent: Domain-Driven Design (Evans); YAGNI ("You Aren't Gonna Need It").

Rationale: Exporting unconsumed intermediate pipeline stages bloats public API surface area, encourages redundant caller loops, and prematurely freezes internal implementation details.
