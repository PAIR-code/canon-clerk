---
inspect:
  - diff
tags:
  - cli-ergonomics
---
Every single-character short flag supported by a CLI command MUST serve as an alias for a self-descriptive long-form flag; standalone short flags lacking long equivalents are forbidden.

Rationale: Short flags optimize for interactive human typing speed, but automated scripts, CI pipelines, and documentation require self-documenting long-form flags for long-term maintainability.

**Guidance:** Define the long-form flag (e.g. `--output`) as the primary option in the CLI definition, attaching the single-character flag (e.g. `-o`) purely as an alias.
