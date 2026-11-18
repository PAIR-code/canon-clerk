---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI commands that execute multi-stage pipelines MUST prefix stage-specific flags with the target stage name (e.g. `--<stage>-<option>`) rather than exposing unqualified flags or cross-stage group names.

Rationale: Following GNU long-option conventions and clig.dev, stage-prefixed flags prevent parameter collisions across composed stages and preserve CLI stability when pipeline stages are added or reordered.
