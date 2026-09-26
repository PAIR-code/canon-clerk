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

Canon Clerk enforces the [Conventional Commits](https://www.conventionalcommits.org/) specification for commit messages and PR titles:

```text
type(scope): description
```

Common types include:
- `feat`: A new feature or capability.
- `fix`: A bug fix.
- `spec`: Architectural contracts and specifications (Spec-Driven Development).
- `docs`: Documentation updates.
- `chore`: Maintenance, repository configuration, or internal canons.
- `build`: Build system or dependency updates.
- `ci`: CI workflows and automation.

### Iterative Branch Commits

Feel free to make granular, WIP, or exploratory commits on your feature branch while developing and addressing review feedback. When your PR is merged, it will be squash-merged into `main` as a single clean commit using your PR title.

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

### PR Title Expectations

Because pull requests are squash-merged into `main`, **your PR title becomes the final commit message on `main`**. Ensure your PR title follows Conventional Commits:

```text
docs: Add development setup guide (#1)
```

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
