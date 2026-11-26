---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - configuration
  - api-design
---
Multi-source configuration resolution MUST evaluate inputs in strict descending order of specificity: explicit CLI or programmatic arguments, target-specific environment variables, shared environment variables, user configuration files, and built-in defaults.

Precedent: The Twelve-Factor App (§III, Config).

Rationale: Cascading strictly from most explicit to broadest default guarantees predictable overrides across ad-hoc CLI debugging, CI pipelines, and zero-config local setups.
