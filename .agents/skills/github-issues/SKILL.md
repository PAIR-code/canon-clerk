---
name: github-issues
description: >-
  Automates triage, listing, searching, and inspection of GitHub issues, including
  unadorned TSV table listing, keyword search, remote auto-detection in triangular worktrees,
  and automatic shunting for large issue bodies. Invoke when prompted to: "List open issues",
  "Review open issues", "Triage issues", "Find issue related to X", or "Inspect issue #X".
---

# GitHub Issues Skill

This skill provides deterministic automation, architectural guidance, and companion scripts for listing, querying, triaging, and inspecting GitHub issues in Canon Clerk.

---

## 1. Architectural Model & Motivation

AI coding assistants frequently need to review, triage, and inspect GitHub issues: orienting to project roadmaps, verifying requirements against motivating issue mandates, and retrieving bug reproduction steps or acceptance criteria.

However, interacting directly with the GitHub CLI (`gh issue`) during agent sessions introduces three subtle traps that can disrupt automated triage:

1. **Remote Ambiguity in Triangular Worktrees:**  
   Canon Clerk uses a triangular Git worktree architecture where `origin` points to the developer's personal fork and `upstream` points to the canonical repository (`PAIR-code/canon-clerk`). Standard commands like `gh issue list` fail immediately with `X No default remote repository has been set`. The companion scripts automatically resolve `remote.upstream.url` (falling back to `remote.origin.url`) and pass `--repo` explicitly.
2. **Token Bombing & Context Window Hygiene:**  
   Querying issues with full Markdown bodies (`--json number,title,body...`) quickly exceeds context window limits and tool truncation thresholds (typically 8KB to 46KB), cutting off output mid-payload and wasting valuable context tokens on repetitive boilerplate.
3. **The "Ad-Hoc Scripting Tax":**  
   Without standardized issue scripts, agents repeatedly write and execute throwaway inline Python or shell scripts to download JSON dumps to temporary files, slice issues into batches, and filter specific fields across multiple turns.

By providing lightweight, token-hygienic companion scripts, this skill eliminates friction and allows assistants to discover and inspect issues in 1–2 deterministic calls.

---

## 2. Trigger Conditions & Discovery

Activate this skill when encountering requests such as:
- *"List open issues"* / *"Review open issues"*
- *"Triage issues"* / *"What issues are currently open?"*
- *"Inspect issue #41"* / *"Show me the details for issue #X"*
- *"Find issues related to worktrees"* / *"Search issues with label enhancement"*
- *"What are the acceptance criteria for issue #X?"*

---

## 3. Companion Scripts

Companion scripts reside in `.agents/skills/github-issues/scripts/` (executable from any directory within the workspace):

### A. Listing & Filtering Issues: `issue-list.sh`

Lists issues in an unadorned, token-efficient Tab-Separated Values (TSV) table:

```bash
# List open issues (default limit: 50):
./.agents/skills/github-issues/scripts/issue-list.sh

# Search issues by query keyword:
./.agents/skills/github-issues/scripts/issue-list.sh --search "worktree"
# Or using positional syntax:
./.agents/skills/github-issues/scripts/issue-list.sh worktree

# Filter by label or author:
./.agents/skills/github-issues/scripts/issue-list.sh --label enhancement
./.agents/skills/github-issues/scripts/issue-list.sh --author jimbojw

# Include closed issues:
./.agents/skills/github-issues/scripts/issue-list.sh --state all

# Output raw JSON:
./.agents/skills/github-issues/scripts/issue-list.sh --json
```

**Actions Performed:**
1. Dynamically detects repository from Git remotes (`remote.upstream.url` falling back to `remote.origin.url`), passing `--repo` explicitly.
2. Fetches issues and emits clean, unadorned TSV rows (`#NUMBER\tSTATE\tLABELS\tUPDATED\tTITLE`) without column truncation or decorative ASCII tables.
3. Automatically buffers and shunts output exceeding 8KB per canon.

---

### B. Inspecting Issue Details: `issue-view.sh`

Extracts structured metadata, authentic Markdown body, and closing references for a specific issue:

```bash
# View an issue:
./.agents/skills/github-issues/scripts/issue-view.sh 41

# Also accepts '#41':
./.agents/skills/github-issues/scripts/issue-view.sh '#41'

# Include comment thread:
./.agents/skills/github-issues/scripts/issue-view.sh 41 --comments

# Output raw JSON:
./.agents/skills/github-issues/scripts/issue-view.sh 41 --json
```

**Actions Performed:**
1. Validates issue number and resolves canonical repository automatically.
2. Emits structured metadata headers (`number`, `title`, `state`, `author`, `url`, `labels`, `assignees`, `milestone`, `closed_by_pr`) followed by the authentic unformatted Markdown body.
3. If `--comments` is specified, appends the comment stream with author and timestamp headers.
4. Automatically buffers and shunts output exceeding 8KB to `/tmp/issue-<number>-<timestamp>.md`, printing the file path and head summary for assistant inspection.

---

## 4. Issue Triage & Mandate Playbook

When using this skill to orient to or work on tasks:

### 1. Orienting to Open Work
1. Run `issue-list.sh` to get an unadorned inventory of active issues.
2. Filter by component or label to isolate relevant tracks (e.g. `issue-list.sh --label enhancement`).

### 2. Grounding in Issue Mandates Before Coding
Per repository governance canons ([`.canons/out-of-band-changes-must-be-shunted-or-sanctioned.md`](../../../.canons/out-of-band-changes-must-be-shunted-or-sanctioned.md)):
1. Run `issue-view.sh <number>` to read the full motivating problem, technical constraints, and acceptance criteria checklist.
2. Confirm your proposed code diff strictly fulfills the issue's stated mandate.
3. If ancillary bugs or cleanup opportunities are spotted during the task, **shunt them** to a new issue rather than bundling drive-by changes into your pull request.

---

## 5. Script Design Standards

All companion scripts comply with the scoped canons in `.agents/skills/.canons/`:

1. **Bourne Shell Standard (`*.sh`):** Zero external interpreter dependencies beyond `git`, `gh`, and standard coreutils (leveraging `gh`'s built-in `jq` support via `--jq`).
2. **Transparent Execution Tracing (`set -x`):** Scripts visibly trace commands so assistants can audit every action.
3. **Unadorned Plain Text Output:** Plain text TSV formatting with zero ANSI color escapes, progress spinners, or Unicode borders.
4. **Output Shunting (>8KB):** Commands emitting high-volume output shunt full logs to `/tmp/` and present concise diagnostic summaries.
