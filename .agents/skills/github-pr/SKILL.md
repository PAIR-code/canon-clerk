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

This skill provides deterministic automation, architectural guidance, and companion scripts for inspecting pull request status, triaging continuous integration (CI) failures, and retrieving diagnostic workflow logs in Canon Clerk.

---

## 1. Architectural Model & Motivation

AI coding assistants frequently interact with in-flight pull requests: verifying CI checks, investigating build/test regressions, and ensuring PRs are merge-ready.

However, interacting with the GitHub CLI (`gh`) for CI diagnostics involves four subtle traps that can disrupt automated triage:

1. **Correlation Fragmentation Across API Surfaces:**  
   `gh pr checks` and `gh run list` use disparate schemas and omit direct Actions job IDs. The authoritative source for complete, consolidated check run status is `gh pr view --json statusCheckRollup`.
2. **The "Admin / Org Workflow" 404 Trap:**  
   Organization-injected security workflows (such as `Google GitHub Admin: Actions Workflow Security Scan` and `zizmor`) are injected at the repository/org level rather than checked into `.github/workflows/`. Standard commands like `gh run view <run-id> --log-failed` fail with `HTTP 404: Not Found`. Reliably retrieving these logs requires querying the REST job logs endpoint directly: `gh api --allow-escape-sequences repos/<owner>/<repo>/actions/jobs/<job-id>/logs`.
3. **Terminal Escape Sequence Blocking:**  
   When fetching raw job logs via `gh api`, GitHub CLI detects ANSI escape codes and abruptly aborts unless `--allow-escape-sequences` is explicitly specified.
4. **Token Bombing & Context Window Hygiene:**  
   Raw CI logs often span thousands of lines of container setup, dependency caching, and post-job teardown. Emitting raw logs directly to stdout exhausts token limits and triggers tool truncation. Logs exceeding 8KB must be shunted to disk while returning a concise failure slice and file path.

---

## 2. Trigger Conditions & Discovery

Activate this skill when encountering requests such as:
- *"Check PR status"* / *"Did the checks pass on my PR?"*
- *"Why did CI fail?"* / *"Inspect failing checks on PR #18"*
- *"Get PR review status"* / *"Triage in-flight PR"*
- *"Show me the error logs from GitHub Actions"*

---

## 3. Companion Scripts

Companion scripts reside in `.agents/skills/github-pr/scripts/` (executable from any directory within the workspace):

### A. Inspecting Status & Checks: `pr-status.sh`

Resolves the PR and executes standard `gh pr view` and `gh pr checks` without synthetic reformatting:

```bash
# Auto-detects PR for current feature branch:
./.agents/skills/github-pr/scripts/pr-status.sh

# Or inspect a specific PR number:
./.agents/skills/github-pr/scripts/pr-status.sh 18
```

**Actions Performed:**
1. Dynamically detects repository from Git remotes (`remote.upstream.url` falling back to `remote.origin.url`), passing `--repo` explicitly.
2. If `[pr-number]` is omitted, automatically resolves the pull request matching the active Git branch.
3. Outputs standard, authentic CLI outputs:
   - `gh pr view`: Title, State, Author, Branches, Description.
   - `gh pr checks`: Tab-separated list of check names, pass/fail/skip states, elapsed durations, and URLs.
4. Automatically buffers and shunts output exceeding 8KB per canon.

---

### B. Extracting Failure Logs: `pr-failed-logs.sh`

Identifies failing checks, retrieves logs via REST endpoint, saves complete logs to disk, and dumps the log tail:

```bash
# Auto-detects failing checks for current feature branch:
./.agents/skills/github-pr/scripts/pr-failed-logs.sh

# Inspect failing checks for a specific PR:
./.agents/skills/github-pr/scripts/pr-failed-logs.sh 18

# Inspect a specific GitHub Actions job directly:
./.agents/skills/github-pr/scripts/pr-failed-logs.sh --job 108349663838
```

**Actions Performed:**
1. Queries `gh pr checks` for checks with bucket `fail`.
2. Extracts the GitHub Actions `job-id` from each failing check run's URL.
3. Queries `gh api --allow-escape-sequences repos/<owner>/<repo>/actions/jobs/<job-id>/logs` (preventing 404 errors on org-level scans).
4. Strips terminal escape codes and saves the complete, unedited raw log to `/tmp/pr-<number>-job-<job-id>.log`.
5. Outputs the saved log path, byte size, and the last 60 lines of the job log (`tail -n 60`) where the fatal failure occurred.

**Directive for Agents:** The script does not guess failure causes with brittle regexes. Inspect the tail output or use `view_file` on the resulting `/tmp/...` log path to examine surrounding context with your own reasoning.

---

## 4. CI Triage & Remediation Playbook

When diagnosing failing status checks, classify the failure into one of four primary categories:

### 1. PR Title Linter (`Lint PR Title` / `Validate PR Title`)
- **Symptoms:** `Validate PR Title` fails with exit code 1 or commitlint errors.
- **Cause:** The PR title does not comply with Conventional Commits (`<type>(<scope>): <description> (#<issue>)`).
- **Remediation:** Update the PR title directly via `gh pr edit <pr-number> --title "type(scope): description (#issue)"`. No code commit is required.

### 2. Security & Policy Scans (`Google GitHub Admin`, `zizmor`, `Actions Workflow Security Scan`)
- **Symptoms:** Check fails on workflow security policies (e.g. `pull_request_target` usage, unpinned actions, script injection).
- **Cause:** A workflow file under `.github/workflows/` introduces an insecure trigger or unpinned action SHA.
- **Remediation:** Inspect the failure slice from `pr-failed-logs.sh`. Address findings by pinning action SHAs, switching triggers from `pull_request_target` to `pull_request`, or resolving SARIF findings.

### 3. Contributor License Agreement (`cla/google`)
- **Symptoms:** `cla/google` status check remains pending or fails with a target URL to `https://cla.developers.google.com/`.
- **Cause:** Commit author email is not associated with a signed Google CLA.
- **Remediation:** Direct the user to visit the CLA URL to link their GitHub account and sign the agreement.

### 4. Repository CI (Unit Tests, Typecheck, Build)
- **Symptoms:** Test suite or compilation errors in repository workflows.
- **Cause:** TypeScript compilation errors, Vitest test assertion failures, or linter errors.
- **Remediation:** Inspect failure lines via `pr-failed-logs.sh`, reproduce locally in the worktree (`npm test` / `npx tsc`), and push a fix.

---

## 5. Script Design Standards

All companion scripts comply with the scoped canons in `.agents/skills/.canons/`:

1. **Bourne Shell Standard (`*.sh`):** Zero external interpreter dependencies beyond `git`, `gh`, and standard coreutils (leveraging `gh`'s built-in `jq` support via `--jq`).
2. **Transparent Execution Tracing (`set -x`):** Scripts visibly trace commands so assistants can audit every action.
3. **Unadorned Plain Text Output:** Plain text formatting with zero ANSI color escapes, progress spinners, or Unicode borders.
4. **Output Shunting (>8KB):** Commands emitting high-volume output shunt full logs to `/tmp/` and present concise diagnostic summaries.
