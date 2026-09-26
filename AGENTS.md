# AGENTS.md

Welcome! This document provides orientation, architectural anchors, and operational guidelines for AI coding assistants (such as Antigravity, Cursor, Claude Code, and Copilot) working in the **Canon Clerk** repository.

## 1. Project Overview

**Canon Clerk** is an automated review gate that enforces project-specific _canons_ authored as Markdown files (with optional YAML frontmatter). See [SPEC.md](SPEC.md) for the formal canon specification.

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
- **Characteristics:** Contains `.canons/`, `docs/`, `package.json`, `README.md`, and project files directly in `.`.
- **DIRECTIVE FOR AGENTS:** You are inside an active working branch. Proceed normally with code editing, testing, and Git operations.
