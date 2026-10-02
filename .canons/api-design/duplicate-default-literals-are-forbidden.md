---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - api-design
  - constants
  - maintainability
---
Duplicate instances of the same inline default literal value are FORBIDDEN across module or package boundaries. Configurable defaults and fallback matching patterns MUST be defined once as an exported constant from their governing module and imported at call sites.

Exception: Non-magic primitive literals representing fundamental programmatic identities (such as `0` for initial index baselines, `""` for empty string accumulators, or boolean sentinels) MAY appear inline IFF the value represents a universal language idiom rather than a configurable domain threshold or default option.

Rationale: In accordance with the Single Source of Truth (SSOT) and Don't Repeat Yourself (DRY) principles (Hunt & Thomas, *The Pragmatic Programmer*), duplicating default literal values causes configuration drift across CLI option definitions, runtime evaluators, and test fixtures when defaults evolve.
