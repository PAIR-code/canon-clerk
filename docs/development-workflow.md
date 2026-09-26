# Development Workflow

This guide covers day-to-day development practices for contributing to **Canon Clerk**, assuming you have completed the one-time machine setup in [Development Setup](development-setup.md).

Canon Clerk is designed for **AI-assisted pair programming**. As a contributor, you primarily steer project intent, review diffs, and guide architecture decisions, while your AI coding assistant (such as Antigravity, Claude Code, Cursor, or Copilot) executes the underlying operational workflows guided by [`AGENTS.md`](../AGENTS.md) and repository skills in [`.agents/skills/`](../.agents/skills/).

---

## 1. Picking Up Work

Before starting on code changes:

1. **Find or open an issue:** Check the [open issues](https://github.com/PAIR-code/canon-clerk/issues) on `PAIR-code/canon-clerk`. If you have a new idea or bug to report, open an issue first to discuss the design with maintainers.
2. **Claim the issue:** Leave a comment indicating you would like to work on it so effort isn't duplicated.

---

## 2. Feature Worktrees & Branch Lifecycle

Canon Clerk uses Git worktrees in a triangular layout (`.bare`, super-root container, `main/`, and isolated feature worktrees) to keep workspaces clean and isolate development tasks.

### Branch & Worktree Naming Convention

All branches and worktree directories follow the convention:

```text
<issue-number>-<slug>
```

**Examples:**
- `1-development-setup`
- `4-conventional-commits`
- `13-git-worktree-skill`

---

### Starting a New Task

#### Directing Your AI Assistant (Recommended)
Prompt your assistant:
> *"Start working on issue #18"* or *"Scaffold a worktree for issue #18 github-pr"*

**What happens:** Your assistant consults [`AGENTS.md`](../AGENTS.md), activates the `git-worktree` skill, runs the scaffolding helper, and sets its working directory context to the newly created worktree.

#### Under the Hood & Manual Fallback
Under the hood, the assistant runs the companion script:
```bash
./.agents/skills/git-worktree/scripts/worktree-start.sh <issue-number> <slug>
```

If you are working without an AI assistant, you can run the script above directly, or execute the raw Git commands from your workspace container super-root:
```bash
git fetch upstream --prune
git worktree add -b <issue-number>-<slug> <issue-number>-<slug> upstream/main
cd <issue-number>-<slug>
```

---

### Syncing with Upstream

Because `main` is checked out in its own dedicated worktree directory, running `git checkout main` from inside a feature worktree will fail with a Git branch-lock collision error.

#### Directing Your AI Assistant (Recommended)
Prompt your assistant:
> *"Sync main with upstream"* or *"Update local main before I rebase"*

**What happens:** The assistant invokes `worktree-sync.sh`, safely updating `<container>/main` via `git -C` and pushing to your personal fork (`origin/main`) without switching branches or altering your current worktree.

#### Under the Hood & Manual Fallback
Under the hood, the assistant runs:
```bash
./.agents/skills/git-worktree/scripts/worktree-sync.sh
```

Or perform the operations manually from anywhere:
```bash
git fetch upstream --prune
git -C main merge --ff-only upstream/main
git -C main push origin main
```


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

### Monitoring CI & Diagnosing Failures

Once your PR is open, continuous integration checks will run automatically.

#### Directing Your AI Assistant (Recommended)
Prompt your assistant:
> *"Check PR status"* or *"Why did CI fail on my PR?"*

**What happens:** Your assistant activates the `github-pr` skill:
1. Runs `pr-status.sh` to summarize check status, elapsed times, and check run URLs.
2. If checks failed, runs `pr-failed-logs.sh` to fetch diagnostic logs directly via the GitHub Actions REST endpoint (avoiding 404 errors on organization-injected security scans like Zizmor or Google GitHub Admin) and extracts high-signal error slices while shunting large logs (>8KB) to disk.

#### Under the Hood & Manual Fallback
Under the hood, the assistant runs the companion scripts in `.agents/skills/github-pr/scripts/`:

```bash
# Check status of the active branch's PR (or pass an explicit PR number)
./.agents/skills/github-pr/scripts/pr-status.sh [pr-number]

# Inspect failing check logs and extract diagnostic error slices
./.agents/skills/github-pr/scripts/pr-failed-logs.sh [pr-number]
```

---

## 5. Cleaning Up Post-Merge

Because Canon Clerk uses squash-merging, tearing down a completed task requires four distinct operations across three surfaces (removing the directory, force-deleting the local branch with `-D`, deleting the remote tracking branch on your fork, and pruning worktree metadata).

### Directing Your AI Assistant (Recommended)

#### 1. Auditing Active Worktrees
Prompt your assistant:
> *"Audit active worktrees and check which branches are ready to clean up"*

**What happens:** The assistant runs `worktree-doctor.sh` to inspect all active worktrees, verify working copy status, and correlate branches against merged GitHub PRs.

#### 2. Tearing Down Merged Worktrees
Prompt your assistant:
> *"Clean up merged worktree for issue #18"* or *"Teardown completed branches"*

**What happens:** The assistant runs `worktree-finish.sh <branch>`, which safely escapes the directory, removes the worktree, force-deletes the local branch, deletes the fork tracking branch, and prunes metadata.

---

### Under the Hood & Manual Fallback

Under the hood, the assistant runs the companion scripts:
```bash
# Audit active worktrees and merged PRs
./.agents/skills/git-worktree/scripts/worktree-doctor.sh

# Teardown completed worktree
./.agents/skills/git-worktree/scripts/worktree-finish.sh <issue-number>-<slug>
```

If performing cleanup manually without the skill:
```bash
# Return to the workspace container root
cd ..

# Remove the worktree directory
git worktree remove <issue-number>-<slug>

# Delete the local branch (using -D to handle squash merges)
git branch -D <issue-number>-<slug>

# Delete the branch on your remote fork (if not deleted by GitHub)
git push origin --delete <issue-number>-<slug>

# Prune stale worktree references
git worktree prune
```


