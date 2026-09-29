---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI commands that perform inspection, auditing, or linting MUST be strictly read-only and MUST NOT mutate repository files, alter modified timestamps, or create untracked cache directories in the workspace.

Rationale: In line with POSIX query idempotency and linter purity standards (e.g. eslint, git-status), audit commands must never dirty working trees, invalidate build caches, or alter git status during inspection.

**Remediation:** Redirect temporary cache files and scratch databases to system temp directories (e.g. `/tmp` or `os.tmpdir()`), requiring an explicit `--fix` or `--write` flag for any intentional repository modifications.
