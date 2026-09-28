---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI commands MUST signal execution outcomes via deterministic exit codes (`0` for success, non-zero for failure), and MUST NOT terminate with exit code `0` when an unhandled error or validation failure occurs.

Rationale: CI runners, pre-commit hooks, and orchestrators rely strictly on process exit codes to determine pipeline success; exiting `0` on failures causes broken builds to pass silently.

**Guidance:** Standardize process termination using explicit exit codes (`0` for success, `1` for lint or audit violations, `2` for invalid CLI usage or syntax, and `3+` for runtime exceptions).
