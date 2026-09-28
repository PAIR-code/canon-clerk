---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI configuration options resolution MUST adhere to strict hierarchical precedence: explicit CLI flags override environment variables, which override configuration files, which override hardcoded defaults.

Rationale: Inverting or muddling configuration hierarchy prevents developers from temporarily overriding repository config files or CI environment variables via ad-hoc command-line flags.

**Guidance:** Implement option parsing so values supplied via command-line arguments take precedence over environment variables, falling back to loaded configuration files and finally default settings.
