---
name: openspec-update-change
description: >-
  Updates an active OpenSpec change by reconciling existing planning artifacts to maintain coherence.
  Invoke when prompted to: "Update change", "Revise proposal", "Reconcile specs",
  "openspec update change", or "opsx update".
---

# OpenSpec Update Change Skill

Revises an active change's planning artifacts (`proposal.md`, `specs/**/*.md`, `design.md`, `tasks.md`) to maintain architectural and scope coherence when requirements evolve.

---

## Operational Invariants

1. **Planning Boundary:** Revise planning artifacts only. Never edit implementation code in this workflow.
2. **Artifact Coherence:** An edit to any single artifact (e.g. design) requires cross-checking and updating dependent artifacts (e.g. specs and tasks) to prevent drift.
3. **Coarse Validation:** Run validation as a post-update hygiene check. Validation does not gate Git commits.

---

## Procedure

### 1. Ground in Change Artifacts

Identify `<change-name>` from input, conversation context, or active directories under `openspec/changes/`.

Read existing planning artifacts directly from disk:
- `openspec/changes/<change-name>/proposal.md`
- `openspec/changes/<change-name>/specs/**/*.md`
- `openspec/changes/<change-name>/design.md`
- `openspec/changes/<change-name>/tasks.md`

### 2. Identify Scope & Reconcile Drift

Identify the intended modification (e.g. modified architectural design, revised scope, updated requirements, or added implementation tasks).

Reconcile drift across the entire artifact suite:
- **Proposal (`proposal.md`):** Update `## Why`, `## What Changes`, or `## Capabilities` if the fundamental scope or intent changed.
- **Specs (`specs/**/spec.md`):** Update or add requirements and executable scenarios (`#### Scenario:`) reflecting the new behavior.
  > [!TIP]
  > Keep requirement descriptions under 500 characters and format scenarios with `#### Scenario:`.
- **Design (`design.md`):** Update architectural decisions, rationale, or trade-offs to reflect the revised technical approach.
- **Tasks (`tasks.md`):** Adjust milestones and task checklist items to match the updated scope.

### 3. Apply Updates Directly to Disk

Edit the affected files directly in `openspec/changes/<change-name>/`.

### 4. Coarse Validation Check

Run coarse validation on the updated change:
```bash
openspec validate "<change-name>" --strict
```
Address any syntax errors or broken references detected by the validator.

### 5. Completion Summary

Display the update summary:
- Name of updated change.
- List of modified planning files and key revisions.
- Recommended next step (e.g. resume implementation with `/openspec-apply-change`).
