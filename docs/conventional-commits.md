# Conventional Commit & Label Taxonomy Reference

This document provides the complete reference specification for commit messages, pull request titles, and GitHub repository labels across **Canon Clerk**.

---

## 1. Orthogonal Matrix Principles

To maintain clarity across a growing monorepo while supporting **Spec-Driven Development (SDD)**, our convention enforces an orthogonal matrix:
* **The Scope is ALWAYS the Surface / Component:** `(core)`, `(cli)`, `(action)`, `(canons)`, `(agents)`, `(spec)`, `(deps)`, `(deps-dev)`, `(readme)`.
* **The Type is ALWAYS the Intent:** `spec`, `feat`, `fix`, `test`, `docs`, `chore`, `build`, `ci`, `refactor`, `perf`, `revert`.

```text
type(scope): description
```

GitHub labels in `PAIR-code/canon-clerk` directly mirror this matrix, enabling seamless issue triage, PR titling, and release automation. Every issue and PR is labeled with its corresponding intent (`type:*`) and, where applicable, its surface component (`scope:*`).

---

## 2. Surface-to-Prefix Mapping

| Surface | Recommended Type & Scope | SemVer Impact | Description & Example |
| :--- | :--- | :--- | :--- |
| **Specifications (OpenSpec / RFCs)** | `spec(<surface>):` | None (Non-releasing) | Architectural contracts and OpenSpec files.<br>`spec(cli): define plugin hooks interface`<br>`spec(canons): define Exception and Remediation semantics` |
| **Core Auditor Engine** | `feat(core):`, `fix(core):` | Minor / Patch | Core analysis, prompt assembly, and screening logic.<br>`feat(core): support inline **Remediation:** markers` |
| **CLI Package** | `feat(cli):`, `fix(cli):` | Minor / Patch | CLI binary, arguments, flags, and local execution.<br>`feat(cli): add --quiet flag and json output` |
| **GitHub Action Package** | `feat(action):`, `fix(action):` | Minor / Patch | Action entrypoint, inputs, and Check Run posting.<br>`fix(action): handle empty diffs gracefully` |
| **Dogfood Canons & Canon Spec** | `chore(canons):`, `spec(canons):` | None | Canon specification ([`SPEC.md`](../SPEC.md)) and dogfood canons ([`.canons/`](../.canons/)).<br>`chore(canons): require manual test plan for ui` |
| **AI Agent Guidelines (`AGENTS.md`, `.agents/`)** | `chore(agents):` | None | Instructions, skills, and tools for AI coding assistants.<br>`chore(agents): add worktree navigation instructions` |
| **Formal Specifications (`specs/`)** | `spec(spec):`, `chore(spec):` | None | Formal system specifications and architecture contracts.<br>`spec(spec): introduce OpenSpec workflow` |
| **External Dependencies** | `chore(deps):`, `build(deps):`, `build(deps-dev):` | None | External runtime and development dependency updates and version bumps.<br>`build(deps-dev): bump vite from 7.0.6 to 7.3.6` |
| **Landing & Root Documentation** | `docs(readme):` | None | Top-level project `README.md` and repository landing documentation.<br>`docs(readme): introduce dual-pillar declarative lead` |
| **Local Tooling & Config** | `build:` / `test:` | None | `tsconfig`, `package.json`, `vitest`, linters.<br>`build: configure vitest and strict typescript` |
| **Remote CI/CD (`.github/workflows/`)** | `ci:` / `ci(action):` | None | GitHub Actions workflows and release automation.<br>`ci: add PR title linting workflow` |
| **Public Documentation (`docs/`)** | `docs:` / `docs(<surface>):` | None | User guides, onboarding, and tutorials.<br>`docs: add development-setup guide` |

> [!IMPORTANT]
> **User-Facing vs Internal Changes:**
> The `feat` and `fix` types strictly trigger SemVer releases via `release-please`. Changes to internal tools, AI agent guidelines, test suites, or documentation MUST use `chore`, `ci`, `build`, `test`, or `docs` rather than `feat` or `fix`.

---

## 3. GitHub Label Taxonomy

Repository labels are codified declaratively in [`.github/labels.yml`](../.github/labels.yml), serving as the canonical source of truth for the Conventional Commit and triage taxonomy.

### 1. `type:*` Labels (Intent & SemVer Drivers)

| Label | Color | Description & SemVer Impact |
| :--- | :--- | :--- |
| `type: feat` | `#0E8A16` (Green) | New user-facing functionality (triggers SemVer minor release). |
| `type: fix` | `#D93F0B` (Red) | Bug fix (triggers SemVer patch release). |
| `type: spec` | `#D4C5F9` (Lavender) | Architecture contracts, OpenSpec, and canon specifications (non-releasing). |
| `type: chore` | `#CFD3D7` (Light Gray) | Repository housekeeping, maintenance, and internal tooling. |
| `type: docs` | `#0075CA` (Blue) | Documentation guides, onboarding, and tutorials. |
| `type: ci` | `#5319E7` (Purple) | GitHub Actions workflows and CI automation. |
| `type: build` | `#F9D0C4` (Coral) | Local build tooling, package configuration, and linters. |
| `type: test` | `#FEF2C0` (Yellow) | Test suite, Vitest fixtures, and test infrastructure. |
| `type: refactor` | `#BFD4F2` (Ice Blue) | Code refactoring without behavior or API changes. |

### 2. `scope:*` Labels (Surface / Component)

Standardized strictly on **`scope: canons`** (collapsing the deprecated `canon` scope):

| Label | Color | Description |
| :--- | :--- | :--- |
| `scope: core` | `#1D76DB` (Blue) | Core Auditor Engine, prompt assembly, and screening logic. |
| `scope: cli` | `#006B75` (Teal) | CLI binary and command-line execution. |
| `scope: action` | `#0E8A16` (Dark Green) | GitHub Action entrypoint and Check Run posting. |
| `scope: canons` | `#FBCA04` (Yellow) | Canon specification ([`SPEC.md`](../SPEC.md)) and dogfood canons ([`.canons/`](../.canons/)). |
| `scope: agents` | `#C2E0C6` (Mint) | AI coding assistant orientation, skills, and tools ([`.agents/`](../.agents/)). |
| `scope: spec` | `#B60205` (Crimson) | Formal system specifications and architecture contracts. |
| `scope: deps` | `#0366D6` (Slate) | External dependency updates and version bumps. |
| `scope: deps-dev` | `#0366D6` (Slate) | External development dependency updates and version bumps. |
| `scope: readme` | `#D4C5F9` (Lavender) | Top-level project `README.md` and repository landing documentation. |

### 3. Community & Triage Labels

| Label | Color | Description |
| :--- | :--- | :--- |
| `good first issue` | `#7057FF` | Good for newcomers. |
| `help wanted` | `#008672` | Extra attention is needed from maintainers. |
| `blocked` | `#E99695` | Blocked by an upstream dependency or open design decision. |
| `duplicate` | `#CFD3D7` | This issue or pull request already exists. |
| `wontfix` | `#FFFFFF` | This will not be worked on. |

---

## 4. OpenSpec & Spec-Driven Development (SDD) Lifecycle

Introducing `spec` as a first-class Conventional Commit type affords a structured SDD progression:

1. **Spec Proposal (Design Phase):**
   - Author or revise architecture contracts under `specs/` or `openspec/` using `spec(<surface>):`.
   - These commits document architectural decisions and appear under a dedicated **"Specifications"** section in changelogs, but do not bump package SemVer versions.
2. **Implementation (Code Phase):**
   - Write tests and code fulfilling the specification using `test(<surface>):`, `feat(<surface>):`, or `fix(<surface>):`, referencing the spec in the commit description.
3. **PR Squash-Merge to `main`:**
   - When the PR squash-merges, the PR title triggers the appropriate SemVer bump (e.g. `feat(cli): support streaming output (#50)` triggers a minor bump) while the baselined spec lands atomically with the fulfilling code.
