---
inspect:
  - diff
tags:
  - cli
---
Stage-specific CLI flags in multi-stage pipelines MUST be prefixed with the target stage name rather than exposed as unqualified flags or cross-stage group names.

Precedent: GNU long-option conventions; Command Line Interface Guidelines (clig.dev).

Rationale: Stage-prefixed flags prevent parameter collisions across composed stages and preserve CLI stability when pipeline stages are added or reordered.
