---
name: openspec-archive-change
description: >-
  Archives a completed OpenSpec change by syncing delta specs to living specs, moving the change
  to the archive directory, and running coarse validation. Invoke when prompted to:
  "Archive change", "Finalize change", "openspec archive", or "opsx archive".
---

# OpenSpec Archive Change Skill

Executes deterministic, low-ceremony archiving of completed OpenSpec changes by promoting delta specs to living specs, relocating the change to the archive directory, and validating repository specs.

---

## Operational Invariants

1. **CLI Execution Invariant:** Always execute OpenSpec via npm scripts (`npm run opsx -- <command>` or `npm run openspec -- <command>`). Never invoke bare `openspec` or `npx openspec`.
2. **Deterministic Archiving:** Follow a clean 4-step sequence: Verify Completion &rarr; Sync Living Specs &rarr; Relocate Directory &rarr; Validate Repository. Omit multi-turn confirmation loops.
3. **Living Spec Format:** Living specs in `openspec/specs/` must never contain delta operation headers (`## ADDED/MODIFIED/REMOVED Requirements`). All requirements live directly under `## Requirements`.
4. **Coarse Validation:** Run validation as a post-archive hygiene check. Validation does not gate Git commits.

---

## Procedure

### 1. Verify Change Completion

Identify the target `<change-name>` from input, conversation context, or active changes in `openspec/changes/`.

Inspect `openspec/changes/<change-name>/tasks.md`.
- Verify all tasks are checked off (`- [x]`).
- If incomplete tasks remain, warn the user and confirm before proceeding.

### 2. Sync Delta Specs to Living Specs (`openspec/specs/`)

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

### 3. Relocate Change to Archive Directory

1. Ensure the archive directory exists:
   ```bash
   mkdir -p openspec/changes/archive
   ```
2. Determine target archive directory name: `YYYY-MM-DD-<change-name>` using the current date (if `<change-name>` already starts with `YYYY-MM-DD-`, preserve the name as-is without double-dating).
3. Relocate the change:
   ```bash
   mv "openspec/changes/<change-name>" "openspec/changes/archive/<target-name>"
   ```

### 4. Coarse Validation Check

Run repository-wide validation:
```bash
npm run opsx -- validate --all --strict
```
Verify that all living specs and archived changes satisfy schema and syntax rules. Address any syntax issues found.

### 5. Completion Summary

Display the archive outcome:
- Archived change name and target location (`openspec/changes/archive/<target-name>`).
- Capabilities created or updated in `openspec/specs/`.
