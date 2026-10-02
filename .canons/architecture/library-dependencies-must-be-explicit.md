---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - architecture
  - api-design
  - clean-architecture
---
Reusable library packages and domain engines MUST declare all operational dependencies and contextual inputs (such as configuration options, resource handles, and clock sources) as explicit function parameters or constructor arguments.

Exception: Top-level application entrypoints, CLI commands, and test harness setup code MAY construct and inject dependencies from ambient configuration or environment state.

Rationale: In accordance with the Explicit Dependencies Principle (Seemann) and Clean Architecture (Martin), requiring callers to supply dependencies explicitly eliminates hidden environmental couplings, guarantees referential transparency where applicable, and ensures modules remain testable in isolation without mocking global state.
