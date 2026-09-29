---
triggers:
  - "package.json"
  - ".npmrc"
  - ".nvmrc"
  - "Dockerfile"
inspect:
  - diff
tags:
  - internal
  - repo-governance
---
Pull requests introducing, upgrading, or modifying runtime prerequisites, system engines, or package management requirements MUST update developer onboarding documentation (`docs/development-setup.md`), workflow documentation (`docs/development-workflow.md`), and workspace scaffolding automation (`.agents/skills/git-worktree/scripts/worktree-start.sh`).

Rationale: When runtime prerequisites drift from documented setup steps and workspace scaffolding scripts, new contributors and AI agents encounter unbootstrapped environments, missing toolchains, and cryptic build failures.

**Remediation:** Document the newly introduced runtime or toolchain requirement under Prerequisites in `docs/development-setup.md`, update setup instructions in `docs/development-workflow.md`, and add necessary bootstrap commands (such as dependency installation) to `worktree-start.sh`.
