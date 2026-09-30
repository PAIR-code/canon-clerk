---
name: openspec-propose
description: >-
  Proposes a new OpenSpec change by scaffolding the change directory and staging all
  planning artifacts in a single cohesive pass. Invoke when prompted to:
  "Propose a change", "Create a proposal", "openspec propose", or "opsx propose".
---

# OpenSpec Propose Skill

Scaffolds a new change and drafts all planning artifacts in a single cohesive pass without turn-by-turn CLI micro-polling.

---

## Operational Invariants

1. **CLI Execution Invariant:** Always execute OpenSpec via npm scripts (`npm run opsx -- <command>` or `npm run openspec -- <command>`). Never invoke bare `openspec` or `npx openspec`.
2. **High-Capability Staging:** Frontier AI models draft and stage the full 4-artifact planning suite (`proposal.md`, `specs/<capability>/spec.md`, `design.md`, `tasks.md`) directly to disk in one pass. Never perform intermediate CLI roundtrips (`instructions`, `status`, etc.) between individual files.
3. **Planning Boundary:** Author planning artifacts only. Do not edit project code or begin implementation until the user explicitly requests implementation via the apply workflow.
4. **Lean on Defaults:** Rely on Canon Clerk's standard `spec-driven` schema and repo-local `openspec/` root. Omit multi-store discovery, custom schema switching, or uninitialized root checks.

---

## Procedure

### 1. Scaffold Change Directory

From the user's intent, derive a concise kebab-case change name (e.g. `streamline-openspec`). Clarify material ambiguity regarding scope or architecture upfront.

Scaffold the change directory:
```bash
npm run opsx -- new change "<change-name>"
```
This generates `openspec/changes/<change-name>/.openspec.yaml`.

### 2. Draft & Stage All 4 Artifacts Directly to Disk

Author all four planning files directly inside `openspec/changes/<change-name>/`:

#### A. `proposal.md`
```markdown
# Proposal

## Why
<Problem statement and architectural motivation>

## What Changes
<High-level summary of proposed changes>

## Capabilities
### New Capabilities
- `<capability>`: <Summary of new capabilities introduced>

### Modified Capabilities
- `<capability>`: <Summary of existing capabilities modified>

## Impact
<Affected packages, consumers, APIs, and dependencies>
```

#### B. `specs/<capability-path>/spec.md`
Delta specification capturing behavioral additions, modifications, or removals using RFC 2119 keywords (`SHALL`, `MUST`) and executable scenarios.

> [!TIP]
> - Keep individual requirement descriptions under 500 characters to satisfy strict length limits.
> - Scenarios must use exactly four hashtags (`#### Scenario:`).

```markdown
# Spec Delta

## Purpose
<Required when defining a new capability; seeds the living specification>

## ADDED Requirements
### Requirement: <Requirement Name>
The system SHALL <specified behavior>.

#### Scenario: <Scenario Name>
- **WHEN** <input or condition>
- **THEN** <expected behavior or outcome>

## MODIFIED Requirements
### Requirement: <Requirement Name>
The system SHALL <updated behavior>.

#### Scenario: <Scenario Name>
- **WHEN** <input or condition>
- **THEN** <expected behavior or outcome>

## REMOVED Requirements
### Requirement: <Requirement Name>

## RENAMED Requirements
- FROM: `### Requirement: <Old Name>`
- TO: `### Requirement: <New Name>`
```

#### C. `design.md`
```markdown
# Design

## Context
<Background context, current architectural state, and problem drivers>

## Goals / Non-Goals
**Goals:**
- <Primary objective>

**Non-Goals:**
- <Explicitly excluded scope>

## Decisions
### 1. <Decision Title>
- **Decision:** <Chosen solution>
- **Rationale:** <Why this approach best satisfies requirements>
- **Alternatives Considered:** <Alternatives evaluated and why rejected>

## Risks / Trade-offs
- <Identified risk or trade-off and mitigation strategy>
```

#### D. `tasks.md`
Actionable task checklist organized into logical milestones with verification criteria:
```markdown
# Tasks

## 1. <Milestone Title>
- [ ] 1.1 <Actionable implementation task with verification criteria>
- [ ] 1.2 <Actionable implementation task with verification criteria>

## 2. <Milestone Title>
- [ ] 2.1 <Actionable implementation task with verification criteria>
```

### 3. Coarse Validation Check

Run coarse validation on the newly staged change:
```bash
npm run opsx -- validate "<change-name>" --strict
```
Validation serves as an advisory hygiene check to catch syntax errors or broken references. Address any syntax issues found. Validation does not gate Git commits.

### 4. Present Completion Summary

Summarize the change name, staged artifact paths, and notify the user that planning is ready for review or implementation (`/openspec-apply-change`).
