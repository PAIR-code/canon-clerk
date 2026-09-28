---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI commands that perform inspection, auditing, or linting MUST be strictly read-only and MUST NOT mutate repository files, alter modified timestamps, or create untracked cache directories in the workspace.

Rationale: Audit commands execute across clean and dirty worktrees alike; writing cache artifacts or altering files during evaluation dirties working trees and corrupts git status.

**Guidance:** Redirect temporary cache files and scratch databases to system temp directories (e.g. `/tmp` or `os.tmpdir()`), requiring an explicit `--fix` or `--write` flag for any intentional repository modifications.
