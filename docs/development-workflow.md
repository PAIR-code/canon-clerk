# Development Workflow

This guide covers day-to-day development practices for contributing to **Canon Clerk**, assuming you have completed the one-time machine setup in [Development Setup](development-setup.md).

Canon Clerk is designed for **AI-assisted pair programming**. As a contributor, you primarily steer project intent, review diffs, and guide architecture decisions, while your AI coding assistant (such as Antigravity, Claude Code, Cursor, or Copilot) executes the underlying operational workflows guided by [`AGENTS.md`](../AGENTS.md) and repository skills in [`.agents/skills/`](../.agents/skills/).

---

## 1. Picking Up Work

Before starting on code changes:

1. **Find or open an issue:** Check the [open issues](https://github.com/PAIR-code/canon-clerk/issues) on `PAIR-code/canon-clerk`. If you have a new idea or bug to report, open an issue first to discuss the design with maintainers.
2. **Claim the issue:** Leave a comment indicating you would like to work on it so effort isn't duplicated.

### Triaging & Inspecting Issues with Your AI Assistant

Your AI assistant can query and inspect issues directly using the `github-issues` skill:

#### Directing Your AI Assistant (Recommended)
Prompt your assistant:
> *"List open issues"* or *"Find issues related to worktrees"*  
> *"Inspect issue #41"* or *"Show me acceptance criteria for issue #18"*

**What happens:** Your assistant activates the `github-issues` skill, querying the canonical repository without remote ambiguity, streaming unadorned TSV tables, and safely shunting large issue bodies.

#### Under the Hood & Manual Fallback
Under the hood, the assistant runs the companion scripts:
```bash
# List open issues in an unadorned TSV table:
./.agents/skills/github-issues/scripts/issue-list.sh [options] [query]

# Inspect a specific issue's metadata, markdown body, and acceptance criteria:
./.agents/skills/github-issues/scripts/issue-view.sh <issue-number>
```

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

**What happens:** Your assistant consults [`AGENTS.md`](../AGENTS.md), activates the `git-worktree` skill, runs the scaffolding helper (which creates the branch and sets up the worktree), and sets its working directory context to the newly created worktree.

#### Under the Hood & Manual Fallback
Under the hood, the assistant runs the companion script:
```bash
./.agents/skills/git-worktree/scripts/worktree-start.sh <issue-number> <slug>
```

If you are working without an AI assistant, you can run the script above directly, or execute the raw commands from your workspace container super-root:
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
* **The Scope is ALWAYS the Surface / Component:** `(core)`, `(schema)`, `(configuration)`, `(cli)`, `(action)`, `(canons)`, `(agents)`, `(spec)`, `(deps)`, `(deps-dev)`, `(readme)`.
* **The Type is ALWAYS the Intent:** `spec`, `feat`, `fix`, `test`, `docs`, `chore`, `build`, `ci`, `refactor`, `perf`, `revert`.

```text
type(scope): description
```

**Common Examples:**
- `feat(cli): add list subcommand` (user-facing functionality)
- `chore(canons): add canons governing cli design` (internal repository rule)
- `chore(agents): update worktree lifecycle skill` (internal assistant tooling)
- `docs: add development workflow guide` (documentation)
- `ci: add PR title linting workflow` (CI automation)

> [!TIP]
> **Complete Reference Specification:**
> For the complete surface-to-prefix mapping, SemVer release rules, GitHub label taxonomy, and Spec-Driven Development (SDD) progression, see the **[Conventional Commit & Label Taxonomy Reference](conventional-commits.md)**.

### Iterative Branch Commits

Feel free to make granular, WIP, or exploratory commits on your feature branch while developing and addressing review feedback. Because all PRs are squash-merged upon completion, your branch commit history will be squashed into a single clean commit on `main`.

### Scope Discipline: Shunt vs. Upstream Chase

To preserve atomic commits, full historical provenance, and clean squash-merge changelogs, pull requests must strictly adhere to their motivating issue mandate (enforced via [`.canons/git-workflow/out-of-band-changes-must-be-shunted-or-sanctioned.md`](../.canons/git-workflow/out-of-band-changes-must-be-shunted-or-sanctioned.md)).

When you or your AI assistant encounter an unrelated bug, missing configuration, or cleanup opportunity mid-task, avoid folding the drive-by fix into the in-flight PR. Instead, employ one of two disciplined escape hatches:

1. **Shunt it (Recommended):** Immediately file a new tracking issue documenting the problem, discovery context, and proposed fix. Keep your current branch and PR strictly focused on its original mandate.
2. **Upstream Chase (Deliberate Expansion):** If the out-of-band change is genuinely coupled or strictly necessary for the current task to land, deliberately expand the mandate by updating the motivating Issue text and PR description *before* committing the change.

### Fast Iteration & Development

To test the CLI binary while iterating on code without running a manual build step:

```bash
npm run cli -- <args>
```

The `npm run cli` script leverages `precli` to automatically rebuild `@canon-clerk/cli` incrementally before invoking `./packages/cli/dist/cli.js`.

For focused testing and development of individual packages:
- `npm run dev`: Run `tsup` build in watch mode
- `npm test`: Run the Vitest test suite
- `npm run typecheck`: Run TypeScript typechecking across workspaces

### Pre-Push Verification (`npm run check`)

Before pushing branches or opening PRs, run the comprehensive shift-left validation suite:

```bash
npm run check
```

This single command deterministically executes the local equivalent of the CI pipeline across all monorepo workspaces, running independent verification lanes concurrently to complete in <8 seconds:
- `npm run lint:lockfile`: Audits `package-lock.json` against untrusted registry URLs.
- `npm run lint:specs`: Validates living specifications and active change proposals (`openspec validate --all --strict`).
- `npm run typecheck`: Runs static typechecking across all workspaces (`tsc --noEmit`).
- `npm run build`: Bundles distribution packages with `tsup` in a consolidated monorepo build process.
- `npm test`: Runs all unit and integration tests via `vitest`.

### Dependency Management & Lockfile Integrity

Canon Clerk strictly validates package provenance and lockfile integrity via `lockfile-lint` (`--allowed-hosts npm`). All dependencies in `package-lock.json` must resolve from the official npm registry (`https://registry.npmjs.org/`).

- **Canonical Registry Pinning:** The repository root `.npmrc` explicitly pins `registry=https://registry.npmjs.org/` and `omit-lockfile-registry-resolved=true`. This ensures local installations override ambient user or system configurations (such as caching proxies or internal mirrors) to generate clean, compliant lockfiles.
- **Troubleshooting Proxy / Mirror Environments:** If working in an environment where an ambient caching proxy lags behind upstream npm (causing `E404 Not Found` errors on newly published packages or native bindings), you can explicitly enforce direct registry resolution when installing packages:
  ```bash
  npm install <package> --registry https://registry.npmjs.org
  ```

### Spec-Driven Development (SDD) with OpenSpec

Canon Clerk employs **Spec-Driven Development (SDD)** via OpenSpec to specify architectural contracts, CLI flags, exit codes, and engine behaviors before writing code.

Living specifications reside in `openspec/specs/` (e.g. `core`, `cli`, `action`), while active change proposals live in `openspec/changes/<change-name>/`.

#### The SDD Progression

1. **Design / Propose:**
   - Author a change proposal containing `proposal.md`, `specs/<capability>/spec.md` (deltas with `## ADDED/MODIFIED/REMOVED Requirements` and `#### Scenario:` blocks), `design.md`, and `tasks.md`.
   - Commit using the `spec(<surface>):` prefix.
2. **Verify / TDD:**
   - Author unit or conformance tests reflecting the spec requirements.
3. **Implement:**
   - Write code fulfilling the specification using `feat(<surface>):` or `fix(<surface>):`.
4. **Baseline & Archive:**
   - Archive the change using `openspec-archive-change` (or sync deltas via `openspec-sync-specs`), promoting changes into living specs under `openspec/specs/`.

#### Directing Your AI Assistant (Recommended)

Prompt your assistant:
> *"Propose a new spec for CLI streaming output"*  
> *"Sync specs from the active change"*  
> *"Apply the tasks from change cli-streaming"*  
> *"Archive change cli-streaming"*

**What happens:** The assistant activates the appropriate OpenSpec skill in `.agents/skills/`:
- `openspec-propose`: Drafts proposal, spec deltas, design, and implementation tasks.
- `openspec-explore`: Explores problem space and codebase patterns.
- `openspec-apply-change`: Executes implementation tasks step-by-step.
- `openspec-sync-specs`: Semantically merges spec deltas into main specs without archiving.
- `openspec-archive-change`: Completes tasks and promotes deltas into `openspec/specs/`.
- `openspec-update-change`: Updates existing change artifacts.

#### Under the Hood & Manual Fallback

You can run the OpenSpec CLI commands directly:
```bash
# Validate all specs and active changes:
openspec validate --all --strict

# Create a new change proposal:
openspec new change <change-name>

# Check status of an in-flight change:
openspec status --change <change-name>

# Archive a completed change into living specs:
openspec archive <change-name>
```


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
  - `spec(canons): define Exception and Remediation semantics (#3)`
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
# Check status of the active branch's PR (or pass --watch to stream checks until completion)
./.agents/skills/github-pr/scripts/pr-status.sh [--watch] [pr-number]

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

**What happens:** The assistant navigates to the `main` worktree and runs `worktree-finish.sh <branch>`, which removes the worktree, force-deletes the local branch, deletes the fork tracking branch, and prunes metadata.

---

### Under the Hood & Manual Fallback

Under the hood, the assistant runs the companion scripts:
```bash
# Audit active worktrees and merged PRs
./.agents/skills/git-worktree/scripts/worktree-doctor.sh

# Teardown completed worktree (run from main)
./.agents/skills/git-worktree/scripts/worktree-finish.sh <issue-number>-<slug>
```

If performing cleanup manually without the skill:
```bash
# Switch to the main worktree
cd ../main

# Remove the worktree directory
git worktree remove <issue-number>-<slug>

# Delete the local branch (using -D to handle squash merges)
git branch -D <issue-number>-<slug>

# Delete the branch on your remote fork (if not deleted by GitHub)
git push origin --delete <issue-number>-<slug>

# Prune stale worktree references
git worktree prune
```


