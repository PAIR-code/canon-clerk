---
inspect:
  - diff
tags:
  - cli
---
CLI query and filtering commands MUST terminate with exit code `0` and emit an empty collection when zero matching items are found, reserving non-zero exit codes strictly for invalid syntax or execution failures.

Precedent: Standard Unix and SQL query conventions (`find`, SQL).

Rationale: Empty search results represent valid operational evaluations; exiting non-zero breaks downstream scripting pipelines executing under `set -e`.

**Remediation:** Return an empty JSON array `[]` or 0-byte stream with exit code `0` when filters yield no matches, directing informational notices to `stderr` only if interactive.
