---
inspect:
  - diff
tags:
  - cli
---
CLI configuration options resolution MUST adhere to strict hierarchical precedence: explicit CLI flags override environment variables, which override configuration files, which override hardcoded defaults.

Precedent: The Twelve-Factor App (§III, Config); standard CLI configuration hierarchies (Viper, Cosmiconfig).

Rationale: This precedence order ensures CLI flags reliably override persistent config files and environment variables during ad-hoc debugging.
