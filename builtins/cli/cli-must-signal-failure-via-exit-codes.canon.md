---
inspect:
  - diff
tags:
  - cli
---
CLI commands MUST signal execution outcomes via deterministic exit codes (`0` for success, non-zero for failure), and MUST NOT terminate with exit code `0` when an unhandled error or validation failure occurs.

Precedent: POSIX.1-2017 exit status (IEEE Std 1003.1); BSD `sysexits.h`.

Rationale: Automation orchestrators rely strictly on non-zero exit codes to halt failing workflows and prevent silent defects.

**Remediation:** Standardize process termination using explicit exit codes (`0` for success, `1` for lint or audit violations, `2` for invalid CLI usage or syntax, and `3+` for runtime exceptions).
