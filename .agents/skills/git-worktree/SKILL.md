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

This skill provides deterministic automation and guidelines for managing Canon Clerk's **triangular Git worktree architecture** (`.bare` bare repository, container super-root, `main/` branch checkout, and isolated `<issue-number>-<slug>/` worktrees).

---

## 1. Architectural Model & Orientation

Canon Clerk repositories are structured in a triangular layout:

```text
canon-clerk/                      <-- Container Super-Root (Level A)
├── .bare/                        <-- Bare Git repository storage (gitdir)
├── .git                          <-- Git pointer file (gitdir: ./.bare)
├── main/                         <-- Permanent primary worktree (Level B)
├── 13-git-worktree-skill/        <-- Feature worktree (Level B)
└── 18-github-pr/                 <-- Feature worktree (Level B)
```

### Context Levels

| Level | Description | Permitted Actions | Prohibited Actions |
| :--- | :--- | :--- | :--- |
| **Level A: Container Super-Root** | Parent directory containing `.bare/` and worktrees. | Managing worktrees, running companion scripts. | Code edits, builds, tests, running linters. |
| **Level B: Worktree Checkout** | Active worktree directory (`main/` or `<issue>-<slug>/`). | Code editing, tests, commits, git diff/status. | Switching checked-out branches (e.g. `git checkout main`). |

---

## 2. Trigger Conditions & Discovery

Activate this skill when encountering requests such as:
- *"Start working on issue #18"* / *"Create a branch for issue #X"*
- *"Sync main with upstream"* / *"Pull upstream changes"*
- *"Clean up merged worktree"* / *"Teardown branch after PR merge"*
- *"Doctor / check active worktrees"* / *"Which worktrees can be deleted?"*

---

## 3. Companion Scripts

Companion scripts reside in `.agents/skills/git-worktree/scripts/` (executable from any directory within the workspace):

### A. Scaffolding a Task: `worktree-start.sh`

Scaffolds a new feature worktree branched off `upstream/main`:

```bash
# Syntax: worktree-start.sh <issue-number> <slug>
#     or: worktree-start.sh <issue-number>-<slug>
./.agents/skills/git-worktree/scripts/worktree-start.sh 18 github-pr
```

**Actions Performed:**
1. Validates `<issue-number>` (digits only) and `<slug>` (kebab-case).
2. Runs `git fetch upstream --prune` (falling back gracefully to cached ref if offline).
3. Adds the worktree sibling to `main/`:
   ```bash
   git worktree add -b <issue>-<slug> <issue>-<slug> upstream/main
   ```
4. Prints the destination path and directs the assistant to switch working context.

**Directive for Agents:** Immediately after running `worktree-start.sh`, navigate into the newly created directory before performing any development work.

---

### B. Synchronizing Main: `worktree-sync.sh`

Safely updates the local `main` worktree without branch collision errors:

```bash
./.agents/skills/git-worktree/scripts/worktree-sync.sh
```

**Why this is needed:** Running `git checkout main` or standard `git pull` from inside a feature worktree fails with `fatal: 'main' is already checked out at '...'`. 

**Actions Performed:**
1. Locates the `main` worktree path dynamically.
2. Verifies the `main` worktree is clean (no uncommitted edits).
3. Fetches `upstream` with prune.
4. Performs a fast-forward merge directly into the `main` worktree:
   ```bash
   git -C <container>/main merge --ff-only upstream/main
   ```
5. Pushes `main` to `origin/main` (fork) if `origin` remote exists.

---

### C. Post-Merge Teardown: `worktree-finish.sh`

Executes the complete 4-step cleanup after a pull request has been squash-merged:

```bash
# Syntax: worktree-finish.sh [-f|--force] <issue-number | branch-name | worktree-path>
./.agents/skills/git-worktree/scripts/worktree-finish.sh 18-github-pr
```

*(If invoked without arguments from inside a feature worktree, it automatically targets the current feature branch.)*

**Actions Performed:**
1. Confirms target is not `main` or `master`.
2. Automatically navigates out of the directory if currently inside the target worktree.
3. Removes the worktree directory:
   ```bash
   git worktree remove <worktree-path>
   ```
4. Deletes the local branch (using `-D` to handle squash merges cleanly):
   ```bash
   git branch -D <branch>
   ```
5. Deletes the remote tracking branch on `origin` if present:
   ```bash
   git push origin --delete <branch>
   ```
6. Prunes worktree metadata:
   ```bash
   git worktree prune
   ```

---

### D. Workspace Health Audit: `worktree-doctor.sh`

Transparently audits all active worktrees, uncommitted modifications, and merged PRs:

```bash
./.agents/skills/git-worktree/scripts/worktree-doctor.sh
```

**Actions Performed:**
1. Traces and executes `git worktree list -v` to show all active worktrees, branches, and commit hashes.
2. Traces and executes `git worktree prune --dry-run` to detect stale worktree references.
3. Checks working tree status in each active worktree (`git status --short`).
4. Queries GitHub CLI (`gh pr list --state merged`) to display merged pull requests whose worktrees can be reaped.
5. Shunts output to `/tmp/worktree-doctor-<timestamp>.log` if output exceeds 8KB, preserving assistant context tokens.


---

## 4. Script Design Principles

All companion scripts comply with the scoped canons in `.agents/skills/.canons/`:

1. **Bourne Shell Standard (`*.sh`):** Zero external interpreter prerequisites beyond `git`, `gh`, and standard POSIX utilities.
2. **Transparent Execution Tracing (`set -x`):** Scripts visibly trace commands so assistants can verify every Git action.
3. **Unadorned Plain Text Output:** Strictly no ANSI color escapes, progress spinners, or Unicode borders.
4. **Output Shunting (>8KB):** Commands with potentially high-volume output buffer their data and redirect to a temporary file when exceeding 8KB.
