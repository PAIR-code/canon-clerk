---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - architecture
  - api-design
---
Reusable library packages and domain engines MUST declare all operational dependencies and contextual inputs as explicit function parameters or constructor arguments.

Exception: Top-level application entrypoints, CLI commands, and test harness setup code MAY read ambient configuration or environment state IFF they construct and explicitly inject the resolved dependencies into downstream library modules.

Precedent: Explicit Dependencies Principle (Seemann); Clean Architecture (Martin).

Rationale: Requiring callers to supply dependencies explicitly eliminates hidden environmental couplings, guarantees referential transparency where applicable, and ensures modules remain testable in isolation without mocking global state.
