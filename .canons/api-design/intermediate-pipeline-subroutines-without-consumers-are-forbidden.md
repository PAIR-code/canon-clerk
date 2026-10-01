---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - api-design
  - domain-driven-design
---
Intermediate stages of an execution pipeline MUST NOT be exposed as public domain exports unless they have distinct, documented external consumers beyond the orchestrator itself.

Exception: Low-level utility packages whose explicit mandate is general-purpose primitives (such as path normalization or text lexing) MAY export atomic operations.

Rationale: In accordance with Domain-Driven Design (Evans) and YAGNI ("You Aren't Gonna Need It"), exporting unconsumed intermediate pipeline stages bloats public API surface area, encourages redundant caller loops, and prematurely freezes internal implementation details.
