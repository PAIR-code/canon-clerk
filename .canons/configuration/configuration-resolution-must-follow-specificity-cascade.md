---
triggers:
  - "packages/configuration/**"
tags:
  - internal
  - configuration
  - api-design
---
Multi-source configuration resolution engines MUST evaluate inputs along a deterministic, descending hierarchy of specificity: explicit programmatic overrides, tier-specific environment variables, general environment variables, vendor environment variables, host operating system credential stores, and static defaults.

Rationale: Grounded in The Twelve-Factor App (III. Config) and layered configuration conventions, cascading from most explicit to broadest defaults guarantees predictability across CLI overrides, tier-specific workflows, and zero-friction developer setup.
