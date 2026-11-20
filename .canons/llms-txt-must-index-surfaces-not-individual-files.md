---
triggers:
  - "llms.txt"
references:
  - "llms.txt"
inspect:
  - diff
tags:
  - internal
  - repo-governance
---
The repository root `llms.txt` MUST index architectural surfaces, subsystems, or directory groups rather than enumerating exhaustive lists of individual files.

Rationale: AI coding assistants require high-level topographical boundaries to navigate large repositories; itemizing individual files reproduces the file system tree and dilutes model attention across primary entry points.

**Remediation:** Replace granular file listings with a consolidated entry that points to the enclosing directory or overarching guide.
