---
triggers:
  - "docs/**/*.md"
  - "specs/**/*.md"
inspect:
  - diff
tags:
  - architecture
---
Architecture specification documents MUST NOT declare physical data structures, concrete type schemas, or language-specific function signatures.

Exception: Architecture specifications MAY include concrete schema or wire-format examples IFF documenting an external, language-agnostic interchange contract (such as a JSON/YAML config schema, HTTP payload, or CLI flag interface).

Rationale: Following the C4 Model (Brown) and arc42 architectural documentation standards, embedding physical code signatures in living architecture documents couples high-level design contracts to implementation churn and causes rapid documentation drift.
