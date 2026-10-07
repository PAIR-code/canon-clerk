---
name: openspec-sync-specs
description: >-
  Syncs delta specs from an active OpenSpec change into living specs without archiving the change.
  Invoke when prompted to: "Sync specs", "Update main specs from delta", "openspec sync", or "opsx sync".
---

# OpenSpec Sync Specs Skill

Intelligently merges delta specs from an active change into the repository's living specifications (`openspec/specs/`) without archiving the change.

---

## Operational Invariants

1. **Living Spec Format:** Living specs in `openspec/specs/` must never contain delta operation headers (`## ADDED/MODIFIED/REMOVED Requirements`). All requirements live directly under a single `## Requirements` section.
2. **Coarse Validation:** Run validation as a post-sync hygiene check. Validation does not gate Git commits.

---

## Procedure

### 1. Identify Target Change & Delta Specs

Identify `<change-name>` from input, conversation context, or active changes under `openspec/changes/`.

Discover delta specifications:
```bash
ls openspec/changes/<change-name>/specs/**/spec.md
```
If no delta specs exist under `openspec/changes/<change-name>/specs/`, notify the user that there are no specs to sync and stop.

### 2. Merge Delta Specs into Living Specs (`openspec/specs/`)

For each delta spec under `openspec/changes/<change-name>/specs/<capability-path>/spec.md`:

#### A. New Capability (`openspec/specs/<capability-path>/spec.md` does not exist)
1. Create `openspec/specs/<capability-path>/spec.md` (and parent directory if needed).
2. Seed the file using the delta's `## Purpose` and all requirements from `## ADDED Requirements`:
   ```markdown
   # <Capability> Specification

   ## Purpose
   <Purpose text from delta spec>

   ## Requirements

   ### Requirement: <Requirement Name>
   The system SHALL <behavior>.

   #### Scenario: <Scenario Name>
   - **WHEN** <input>
   - **THEN** <expected outcome>
   ```

#### B. Existing Capability (`openspec/specs/<capability-path>/spec.md` exists)
Intelligently merge delta requirements into the existing living spec:
- **ADDED Requirements:** Append new requirements to `## Requirements`.
- **MODIFIED Requirements:** Update the corresponding requirement text and scenarios, preserving untouched scenarios.
- **REMOVED Requirements:** Remove the specified requirement blocks. (If all requirements are removed, retire the capability by deleting the spec file).
- **RENAMED Requirements:** Update requirement headings from old to new names.
- Ensure the living spec retains a single `## Requirements` section with no delta operation headers.

### 3. Coarse Validation Check

Run spec validation to confirm syntax and schema hygiene:
```bash
openspec validate --specs --strict
```
Address any syntax issues found.

### 4. Completion Summary

Display the sync outcome:
- Name of synced change.
- List of living specifications created or updated in `openspec/specs/`.
- Remind the user that the change remains active and can be archived when implementation is complete.
