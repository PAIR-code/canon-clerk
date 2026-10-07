---
name: openspec-apply-change
description: >-
  Implements tasks from an OpenSpec change in cohesive batches with tests and milestone checkoffs.
  Invoke when prompted to: "Apply change", "Implement change", "Work on tasks",
  "openspec apply", or "opsx apply".
---

# OpenSpec Apply Change Skill

Executes implementation tasks from an active OpenSpec change in cohesive batches with accompanying tests and milestone task checkoffs.

---

## Operational Invariants

1. **Batch Implementation:** Frontier AI models implement cohesive milestones of tasks directly with accompanying tests, rather than polling for confirmation before every micro-task or bullet point.
2. **Direct Disk Grounding:** Read planning artifacts (`proposal.md`, `specs/**/*.md`, `design.md`, `tasks.md`) directly from disk under `openspec/changes/<change-name>/`.
3. **Coarse Validation:** Run validation checks at milestone completions or upon finishing all tasks as an advisory hygiene check. Validation does not gate Git commits.

---

## Procedure

### 1. Ground in Change Artifacts

Identify the target `<change-name>` from input or context (or inspect active changes under `openspec/changes/`).

Read planning artifacts directly from disk:
- `openspec/changes/<change-name>/proposal.md`
- `openspec/changes/<change-name>/specs/**/*.md`
- `openspec/changes/<change-name>/design.md`
- `openspec/changes/<change-name>/tasks.md`

Announce the active change: `"Applying change: <change-name>"`.

### 2. Implement Tasks in Cohesive Milestones

1. Identify the next logical milestone or phase in `tasks.md`.
2. Implement code changes, write/update unit or integration tests, and run test suites.
3. Once tests pass for the milestone, batch-update the completed task checkboxes in `openspec/changes/<change-name>/tasks.md` from `- [ ]` to `- [x]`.

### 3. Milestone Reporting & Pause Conditions

- Provide clear progress updates at milestone boundaries (listing completed tasks and test results).
- Pause and prompt the user only if:
  - Material ambiguity or an unresolvable design dilemma arises.
  - A blocker or unexpected test failure occurs.
  - Explicit user input was requested.

### 4. Coarse Validation Check

Run coarse validation:
```bash
openspec validate "<change-name>" --strict
```
Validation serves as an advisory hygiene check to ensure spec consistency and valid task tracking.

### 5. Completion Summary

When all tasks in `tasks.md` are complete (`- [x]`), summarize accomplishments and advise the user that the change is ready to archive via `/openspec-archive-change`.
