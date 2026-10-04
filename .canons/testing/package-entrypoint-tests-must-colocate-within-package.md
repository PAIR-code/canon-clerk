---
triggers:
  - "packages/**"
  - "crates/**"
  - "modules/**"
  - "**/*.test.*"
  - "**/*.spec.*"
  - "**/*_test.*"
  - "**/test_*.py"
inspect:
  - diff
tags:
  - testing
  - architecture
  - colocation
---
Hermetic integration tests verifying package entrypoints, binary CLI routers, or distribution bundling MUST remain colocated within that package's test suite rather than relocated to external test workspaces.

Rationale: Citing the Principle of Cohesion (Yourdon & Constantine) and Colocation, tests verifying package packaging, argument dispatch, and process exit codes must reside with the code they verify so that localized changes can be validated independently without cross-package test orchestration.
