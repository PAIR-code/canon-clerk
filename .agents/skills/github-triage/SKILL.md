---
name: github-triage
description: >-
  Triage and audit GitHub issues and pull requests against the Conventional
  Commit label matrix. Scans titles, cross-references .github/labels.yml,
  and reports missing type:/scope: labels and unrecognized monorepo surfaces.
  Invoke when prompted to: "Triage open issues", "Audit label coverage",
  "Backfill missing type/scope labels", "Check which issues lack labels",
  or "Report Conventional Commit label gaps".
---

# GitHub Triage Skill

Use this skill to audit and backfill `type:*` and `scope:*` labels on open
issues and pull requests. The deterministic script extracts candidate labels
from titles; the agent resolves semantic ambiguity and applies reviewed changes.

---

## Label Matrix Quick Reference

Canonical source: `.github/labels.yml`. The Conventional Commit form is
`type(scope): summary`, mapping to `type: <type>` and, where a scope is
present, `scope: <scope>`.

### Type labels (intent; SemVer drivers noted)

| Title type | GitHub label | SemVer |
|---|---|---|
| `feat` | `type: feat` | minor |
| `fix` | `type: fix` | patch |
| `spec` | `type: spec` | none |
| `chore` | `type: chore` | none |
| `docs` | `type: docs` | none |
| `ci` | `type: ci` | none |
| `build` | `type: build` | none |
| `test` | `type: test` | none |
| `refactor` | `type: refactor` | none |
| `perf` / `revert` | (no dedicated label) | per the type's usual meaning |

### Scope labels (surface / component)

| Title scope | GitHub label |
|---|---|
| `core` | `scope: core` |
| `schema` | `scope: schema` |
| `configuration` | `scope: configuration` |
| `cli` | `scope: cli` |
| `action` | `scope: action` |
| `canons` | `scope: canons` |
| `agents` | `scope: agents` |
| `spec` | `scope: spec` |
| `deps` | `scope: deps` |
| `deps-dev` | `scope: deps-dev` |
| `readme` | `scope: readme` |

`type` and `scope` are independent axes: apply the type label whenever the
title carries a Conventional Commit type, and apply the scope label only when a
recognized scope is present.

---

## Companion Scripts

All scripts reside in `.agents/skills/github-triage/scripts/` and auto-resolve
the canonical upstream repository.

### `triage-audit.sh [options]`

Outputs an unadorned TSV table
(`NUMBER\tTYPE\tSCOPE\tSTATUS\tMISSING_LABELS\tUNRECOGNIZED_SCOPES`). Shunts to
`/tmp/` if the output exceeds 8KB.

```bash
# Audit open issues and PRs:
./.agents/skills/github-triage/scripts/triage-audit.sh

# Audit only issues without any type: or scope: label:
./.agents/skills/github-triage/scripts/triage-audit.sh --unlabeled

# Audit from a specific issue number onward (backfill pass):
./.agents/skills/github-triage/scripts/triage-audit.sh --since 142

# Include pull requests in addition to issues:
./.agents/skills/github-triage/scripts/triage-audit.sh --prs

# Output the raw JSON instead of the TSV summary:
./.agents/skills/github-triage/scripts/triage-audit.sh --json
```

---

## Resolution Rules

1. **Recognized type, recognized scope:** apply both `type: <type>` and
   `scope: <scope>`.
2. **Recognized type, unrecognized scope:** apply `type: <type>` immediately;
   report the scope in `UNRECOGNIZED_SCOPES` and ask a maintainer whether a new
   `scope:` label should be created in `.github/labels.yml`. Do not invent a
   label.
3. **`-private` workspace packages:** these are internal and do not drive
   public SemVer. Prefer `type: chore` (or `type: build` / `type: ci`) over a
   `type: feat` / `type: fix` that would trigger a false release, unless the
   change is genuinely user-facing on a public surface.
4. **Missing labels:** add only the labels the title justifies; never widen a
   type to force a SemVer bump or narrow one to avoid one.
5. **Ambiguity is an agent decision:** the script reports candidates; the agent
   confirms intent before running `gh issue edit` / `gh pr edit`.

---

## Triage Workflow (Scan → Audit → Review → Apply)

1. **Scan** — `triage-audit.sh --unlabeled` (add `--since <number>` for a
   backfill pass) to enumerate issues/PRs missing labels.
2. **Audit** — inspect the TSV `MISSING_LABELS` and `UNRECOGNIZED_SCOPES`
   columns against the matrix above.
3. **Review** — for each row, confirm the correct labels; resolve unrecognized
   scopes and `-private` cases per the rules.
4. **Apply** — `gh issue edit <number> --add-label "type: feat", "scope: cli"`
   (or `gh pr edit`), one item at a time, re-running the audit to confirm the
   delta.

---

## Operational Invariants

1. **Deterministic extraction, semantic judgment:** the script extracts and
   cross-references; the agent resolves ambiguity and executes changes.
2. **Inspect shunted output:** if the TSV exceeds 8KB, inspect the printed
   `/tmp/...` file with line slicing rather than re-running.
3. **Preserve scope:** do not bundle unrelated drive-by label edits into a
   triage pass; a new surface that needs a label is its own RFC/issue.
4. **Verify against live state:** run the script before and after applying to
   confirm the label delta matches intent.
