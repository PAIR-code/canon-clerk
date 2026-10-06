---
name: git-worktree
description: >-
  Automates the lifecycle of triangular Git worktrees in Canon Clerk, including scaffolding
  new feature worktrees off upstream/main, safely synchronizing main without branch-lock errors,
  executing 4-step teardown post-merge, and auditing active worktrees. Invoke when prompted to:
  "Start working on issue #X", "Create a worktree", "Sync with upstream", "Sync main",
  "Clean up merged branch", "Teardown worktree", or "Audit worktrees".
---

# Git Worktree Skill

Automates management of Canon Clerk's triangular Git worktree lifecycle (`.bare` bare repository, container super-root, `main/` branch checkout, and isolated `<issue-number>-<slug>/` worktrees).

---

## Companion Scripts

All scripts reside in `.agents/skills/git-worktree/scripts/` (executable from any directory within the workspace):

### 1. `worktree-start.sh <issue-number> <slug>`

Scaffolds a new feature worktree branched off `upstream/main`, seeds dependencies from `main/` via hardlinks (`cp -al`), builds packages, and runs smoke tests:

```bash
./.agents/skills/git-worktree/scripts/worktree-start.sh 18 github-pr
```

### 2. `worktree-sync.sh`

Safely updates the local `main` worktree without branch collision errors:

```bash
./.agents/skills/git-worktree/scripts/worktree-sync.sh
```

### 3. `worktree-finish.sh [-f] [target]`

Executes the complete 4-step teardown (removes worktree, deletes local branch, deletes remote tracking branch, and prunes metadata):

```bash
# Target specific worktree / branch (from main):
./.agents/skills/git-worktree/scripts/worktree-finish.sh 18-github-pr
```

### 4. `worktree-doctor.sh`

Transparently audits active worktrees, uncommitted modifications, and merged PRs eligible for cleanup:

```bash
./.agents/skills/git-worktree/scripts/worktree-doctor.sh
```

---

## Operational Invariants

1. **Workspace Context Levels:**
   - **Super-root (Level A):** Contains `.bare/` and worktree directories. Only use for managing worktrees; never run builds, tests, or code edits here.
   - **Worktree checkout (Level B):** Active branch directory (`main/` or `<issue>-<slug>/`). Run all edits, tests, and commits here.
2. **Never Check Out `main` Directly:** Running `git checkout main` inside a feature worktree causes `fatal: 'main' is already checked out`. Use `worktree-sync.sh` instead.
3. **Safe Teardown Context:** When finishing a branch, always navigate to `main/` first (e.g. `cd ../main`) before invoking `worktree-finish.sh <branch-or-worktree>`. Attempting teardown from inside the target worktree will fail with an error to prevent an orphaned working directory.
