---
triggers:
  - ".github/workflows/**"
tags:
  - repo-governance
---
GitHub Actions workflow steps MUST only invoke dedicated script files or native CLI commands.

Rationale: Standalone script files guarantee local testability and shift-left verification; procedural logic embedded in workflow YAML cannot be executed or debugged in the workspace prior to pushing.
