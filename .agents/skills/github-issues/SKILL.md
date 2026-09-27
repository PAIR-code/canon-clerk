---
name: github-issues
description: >-
  Lists, searches, and inspects GitHub issues via token-efficient TSV and shunted
  markdown views. Auto-resolves triangular remotes. Invoke when prompted to:
  "List open issues", "Review open issues", "Triage issues", "Find issue related to X",
  or "Inspect issue #X".
---

# GitHub Issues Skill

Use this skill to list, search, and inspect GitHub issues without triangular remote errors or context token truncation.

---

## Companion Scripts

All scripts reside in `.agents/skills/github-issues/scripts/` and automatically resolve the canonical upstream repository:

### 1. `issue-list.sh [options] [query]`

Outputs an unadorned TSV table (`NUMBER\tSTATE\tLABELS\tUPDATED\tTITLE`). Shunts to `/tmp/` if >8KB.

```bash
# List open issues:
./.agents/skills/github-issues/scripts/issue-list.sh

# Search by keyword:
./.agents/skills/github-issues/scripts/issue-list.sh "search terms"

# Filter by label, author, or state:
./.agents/skills/github-issues/scripts/issue-list.sh --label enhancement
./.agents/skills/github-issues/scripts/issue-list.sh --author jimbojw
./.agents/skills/github-issues/scripts/issue-list.sh --state all

# Output raw JSON:
./.agents/skills/github-issues/scripts/issue-list.sh --json
```

---

### 2. `issue-view.sh <number> [options]`

Outputs structured metadata headers followed by authentic Markdown body. Shunts to `/tmp/issue-<number>.md` if >8KB.

```bash
# View issue metadata & body:
./.agents/skills/github-issues/scripts/issue-view.sh 41

# Include comment thread:
./.agents/skills/github-issues/scripts/issue-view.sh 41 --comments

# Output raw JSON:
./.agents/skills/github-issues/scripts/issue-view.sh 41 --json
```

---

## Operational Invariants

1. **Verify Mandate Before Coding:** Run `issue-view.sh <number>` to ground changes in stated requirements and acceptance checklists before editing files.
2. **Inspect Shunted Output:** If an issue exceeds 8KB, inspect the printed `/tmp/...` file using `view_file` (with line slicing) to preserve context window hygiene.
3. **Preserve Scope:** If an ancillary bug or missing canon is discovered, shunt it to a new issue rather than bundling drive-by edits.
