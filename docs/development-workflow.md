# Development Workflow

This guide covers day-to-day development practices for contributing to **Canon Clerk**, assuming you have completed the one-time machine setup in [Development Setup](development-setup.md).

---

## 1. Picking Up Work

Before starting on code changes:

1. **Find or open an issue:** Check the [open issues](https://github.com/PAIR-code/canon-clerk/issues) on `PAIR-code/canon-clerk`. If you have a new idea or bug to report, open an issue first to discuss the design with maintainers.
2. **Claim the issue:** Leave a comment indicating you would like to work on it so effort isn't duplicated.

---

## 2. Feature Worktrees & Branch Lifecycle

Canon Clerk uses Git worktrees to keep your working directories clean and isolated.

### Branch & Worktree Naming Convention

All branches and worktree directories must follow the convention:

```text
<issue-number>-<slug>
```

**Examples:**
- `1-development-setup`
- `4-conventional-commits`
- `7-monorepo-scaffolding`

### Starting a New Task

Prefer to branch off the latest `upstream/main`:

```bash
# From your workspace root
git fetch upstream

# Create the worktree and branch
git worktree add -b <issue-number>-<slug> <issue-number>-<slug> upstream/main

# Navigate into the new worktree
cd <issue-number>-<slug>
```

You now have an isolated directory containing the full repository checkout, ready for development.

Alternatively, you may branch off of other feature branches if you're actively working on a chain of interdependent PRs.

### Syncing with Upstream

Before creating a branch or opening a PR, ensure your local `main` is current.

---

## 3. Making Changes & Committing

### Conventional Commits

Canon Clerk strictly enforces the [Conventional Commits](https://www.conventionalcommits.org/) specification for all commit messages and PR titles.

To maintain clarity across a growing monorepo while supporting **Spec-Driven Development (SDD)**, our convention follows a strict orthogonal matrix:
* **The Scope is ALWAYS the Surface / Component:** `(cli)`, `(action)`, `(core)`, `(canon)`, `(canons)`, `(agents)`, `(spec)`, `(deps)`.
* **The Type is ALWAYS the Intent:** `spec`, `feat`, `fix`, `test`, `docs`, `chore`, `build`, `ci`, `refactor`, `perf`, `revert`.

```text
type(scope): description
```

### Surface-to-Prefix Mapping

| Surface | Recommended Type & Scope | SemVer Impact | Description & Example |
| :--- | :--- | :--- | :--- |
| **Specifications (OpenSpec / RFCs)** | `spec(<surface>):` | None (Non-releasing) | Architectural contracts and OpenSpec files.<br>`spec(cli): define plugin hooks interface`<br>`spec(canon): draft Guidance vs Supplement semantics` |
| **Core Auditor Engine** | `feat(core):`, `fix(core):` | Minor / Patch | Core analysis, prompt assembly, and screening logic.<br>`feat(core): support inline **Supplement:** markers` |
| **CLI Package** | `feat(cli):`, `fix(cli):` | Minor / Patch | CLI binary, arguments, flags, and local execution.<br>`feat(cli): add --quiet flag and json output` |
| **GitHub Action Package** | `feat(action):`, `fix(action):` | Minor / Patch | Action entrypoint, inputs, and Check Run posting.<br>`fix(action): handle empty diffs gracefully` |
| **Dogfood Canons (`.canons/`)** | `chore(canons):` | None | Internal project rules governing this repository.<br>`chore(canons): require manual test plan for ui` |
| **AI Agent Guidelines (`AGENTS.md`, `.agents/`)** | `chore(agents):` | None | Instructions, skills, and tools for AI coding assistants.<br>`chore(agents): add worktree navigation instructions` |
| **Local Tooling & Config** | `build:` / `test:` | None | `tsconfig`, `package.json`, `vitest`, linters.<br>`build: configure vitest and strict typescript` |
| **Remote CI/CD (`.github/workflows/`)** | `ci:` / `ci(action):` | None | GitHub Actions workflows and release automation.<br>`ci: add PR title linting workflow` |
| **Public Documentation (`docs/`)** | `docs:` / `docs(<surface>):` | None | User guides, onboarding, and tutorials.<br>`docs: add development-setup guide` |

### OpenSpec & Spec-Driven Development (SDD) Lifecycle

Introducing `spec` as a first-class Conventional Commit type affords a structured SDD progression:
1. **Spec Proposal (Design Phase):**
   - Author or revise architecture contracts under `specs/` or `openspec/` using `spec(<surface>):`.
   - These commits document architectural decisions and appear under a dedicated **"Specifications"** section in changelogs, but do not bump package SemVer versions.
2. **Implementation (Code Phase):**
   - Write tests and code fulfilling the specification using `test(<surface>):`, `feat(<surface>):`, or `fix(<surface>):`, referencing the spec in the commit description.
3. **PR Squash-Merge to `main`:**
   - When the PR squash-merges, the PR title triggers the appropriate SemVer bump (e.g. `feat(cli): support streaming output (#50)` triggers a minor bump) while the baselined spec lands atomically with the fulfilling code.

### Iterative Branch Commits

Feel free to make granular, WIP, or exploratory commits on your feature branch while developing and addressing review feedback. Because all PRs are squash-merged upon completion, your branch commit history will be squashed into a single clean commit on `main`.

---

## 4. Submitting a Pull Request

### Pushing to Your Fork

Push your feature branch to your personal fork (`origin`):

```bash
git push -u origin <issue-number>-<slug>
```

*(Because `remote.pushDefault` was configured to `origin` during setup, `git push` automatically targets your fork.)*

### Opening the PR

Open a Pull Request targeting `PAIR-code/canon-clerk:main`:

- **Via GitHub CLI:**
  ```bash
  gh pr create --web
  ```
  *(Or run `gh pr create` for an interactive terminal prompt.)*
- **Via Web UI:** Visit `https://github.com/PAIR-code/canon-clerk` where GitHub will offer a prompt to open a PR from your recently pushed branch.

### PR Title Expectations & Automated Linting

Under Canon Clerk's squash-and-merge policy, **your PR title directly becomes the commit message on `main`**, which drives automated changelogs and releases via Google's `release-please`.

* **PR Title Format:** Every PR title must adhere strictly to the Conventional Commit standard:
  ```text
  <type>(<scope>): <short description> (#<issue-number>)
  ```
  *Examples:*
  - `feat(cli): add streaming json output (#7)`
  - `spec(canon): define Guidance vs Supplement semantics (#3)`
  - `ci: add PR title linting workflow (#9)`
  - `docs: add development setup guide (#1)`

* **Automated CI Validation:** Our CI runs a PR title linter ([`.github/workflows/pr-title-lint.yml`](../.github/workflows/pr-title-lint.yml)) on every opened, edited, or synchronized PR. If the check fails, edit your PR title in GitHub to immediately clear the check.


---

## 5. Cleaning Up Post-Merge

Once your pull request has been merged into upstream:

```bash
# Return to the workspace container root
cd ..

# Remove the worktree directory
git worktree remove <issue-number>-<slug>

# Delete the local branch
git branch -d <issue-number>-<slug>

# (Optional) Delete the branch on your remote fork
git push origin --delete <issue-number>-<slug>
```
