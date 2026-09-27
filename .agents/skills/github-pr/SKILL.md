---
name: github-pr
description: >-
  Automates triage and status inspection of pull requests on GitHub, including
  querying CI status checks, diagnosing failing runs, retrieving workflow logs
  without 404 errors, and shunting large logs. Invoke when prompted to: "Check PR status",
  "Why did CI fail?", "Inspect failing checks", "Get PR review status", "Triage CI",
  or "View PR failure logs".
---

# GitHub PR Skill

Inspects pull request status, triages continuous integration (CI) failures, and retrieves diagnostic workflow logs.

---

## Companion Scripts

All scripts reside in `.agents/skills/github-pr/scripts/` (executable from any directory within the workspace):

### 1. `pr-status.sh [--watch] [pr-number]`

Resolves the PR, executes `gh pr view` and `gh pr checks` without synthetic reformatting, and auto-shunts output exceeding 8KB. If checks are pending, offers in situ guidance to watch with `--watch`:

```bash
# Auto-detects PR for current feature branch:
./.agents/skills/github-pr/scripts/pr-status.sh

# Watch CI checks until completion:
./.agents/skills/github-pr/scripts/pr-status.sh --watch

# Or inspect a specific PR number:
./.agents/skills/github-pr/scripts/pr-status.sh 18
./.agents/skills/github-pr/scripts/pr-status.sh --watch 18
```

### 2. `pr-failed-logs.sh [pr-number | --job <id>]`

Queries failing checks, retrieves logs via REST endpoint, saves complete logs to `/tmp/`, and prints the final 60 lines:

```bash
# Auto-detects failing checks for current feature branch:
./.agents/skills/github-pr/scripts/pr-failed-logs.sh

# Inspect failing checks for a specific PR:
./.agents/skills/github-pr/scripts/pr-failed-logs.sh 18

# Inspect a specific GitHub Actions job directly:
./.agents/skills/github-pr/scripts/pr-failed-logs.sh --job 108349663838
```

---

## CI Failure Triage Checklist

When diagnosing failing status checks, classify the failure into one of four categories:

1. **PR Title Linter (`Validate PR Title`):** Fails if PR title violates Conventional Commits. Fix with `gh pr edit <pr-number> --title "type(scope): description (#issue)"` (no code commit needed).
2. **Security & Policy Scans (`Google GitHub Admin`, `zizmor`):** Fails on workflow policies (e.g. unpinned actions, script injection). Fix by pinning action SHAs or addressing SARIF findings.
3. **Contributor License Agreement (`cla/google`):** Fails if author email lacks signed Google CLA. Remind author to sign via `https://cla.developers.google.com/`.
4. **Repository CI (Tests, Lint, Build):** Inspect failure lines via `pr-failed-logs.sh`, reproduce locally in the worktree (`npm test`), and push fixes.
