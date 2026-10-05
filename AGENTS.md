# AGENTS.md

Welcome! This document provides orientation, architectural anchors, and operational guidelines for AI coding assistants (such as Antigravity, Cursor, Claude Code, and Copilot) working in the **Canon Clerk** repository.

## 1. Project Overview

**Canon Clerk** is an automated review gate that enforces _canons_—standard engineering rule packs and repository-specific invariants authored as Markdown files (with optional YAML frontmatter). Evaluation is orchestrated across a **Seven-Stage Caseload DAG** (`intake` → `discover` → `validate` on Branch A; `configure` on Branch B; converging at `docket` → `admit` → `audit`). See [SPEC.md](SPEC.md) for the formal canon specification and [docs/architecture.md](docs/architecture.md) (with [overview](docs/architecture/overview.md)) for the Caseload pipeline architecture.

> **Canon:** *(n)*. A source code or repository change rule that is: **semantic**, **atomic**, **falsifiable**, **bounded**, **grounded**, **salient**, and **correctable**.

**Primary Repository:** `PAIR-code/canon-clerk`

## 2. Workspace Orientation: Super-Root vs. Worktree Checkout

Canon Clerk uses a **triangular Git worktree layout**. Depending on how the workspace was opened, your current working directory may be at one of two distinct levels:

### Level A: The Workspace Container (Super-Root)
- **Characteristics:** Contains `.bare/` (bare Git directory), a root `.git` pointer file (`gitdir: ./.bare`), root symlinks (`AGENTS.md -> ./main/AGENTS.md`, `.agents/ -> ./main/.agents`), and sibling directories like `main/` and `<issue-number>-<slug>/`.
- **CRITICAL DIRECTIVE FOR AGENTS:**
  - **Do NOT** execute builds, tests, or code edits directly in the container super-root.
  - **Always navigate into a worktree directory** before executing development tasks (e.g. `cd main` or `cd <issue-number>-<slug>`).
  - When starting work on an issue, scaffold a new worktree using the `git-worktree` skill. 

### Level B: A Worktree Checkout (Repo Root)
- **Characteristics:** Contains `.canons/`, `docs/`, `openspec/`, `package.json`, `README.md`, and project files directly in `.`.
- **DIRECTIVES FOR AGENTS:**
  - You are inside an active working branch. Proceed normally with code editing, testing, and Git operations.
  - **OpenSpec Invariants:** Always execute OpenSpec via npm scripts (`npm run opsx -- <command>` or `npm run openspec -- <command>`). Never invoke `npx openspec` or assume bare `openspec` exists in `$PATH`. Canon Clerk adheres to default OpenSpec conventions (`spec-driven` schema, repo-local root); draft and stage planning suites in cohesive batches without CLI micro-polling. Validation serves as an advisory hygiene check and does not gate Git commits.
