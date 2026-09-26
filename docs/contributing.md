# How to Contribute

We would love to accept your patches and contributions to this project.

## Before you begin

### Sign our Contributor License Agreement

Contributions to this project must be accompanied by a
[Contributor License Agreement](https://cla.developers.google.com/about) (CLA).
You (or your employer) retain the copyright to your contribution; this simply
gives us permission to use and redistribute your contributions as part of the
project.

If you or your current employer have already signed the Google CLA (even if it
was for a different project), you probably don't need to do it again.

Visit <https://cla.developers.google.com/> to see your current agreements or to
sign a new one.

### Review our Community Guidelines

This project follows [Google's Open Source Community
Guidelines](https://opensource.google/conduct/).

### Development Setup

Before picking up an issue or submitting changes, review our
[Development Setup Guide](development-setup.md) to set up your local fork,
triangular Git worktrees, and GitHub CLI tooling.

## Contribution process

For our complete day-to-day workflow—including issue claiming, worktree management, commit conventions, and pull request etiquette—see the [Development Workflow Guide](development-workflow.md).

### Branching Convention

All feature branches and worktrees must follow the naming standard:

```text
<issue-number>-<slug>
```

For example, `1-development-setup` or `4-conventional-commits`.

### Commit Standards & Conventional Commits

Canon Clerk enforces the [Conventional Commits](https://www.conventionalcommits.org/) specification for commit messages and PR titles:

```text
type(scope): description
```

To support Spec-Driven Development (SDD) alongside standard engineering, we adopt an orthogonal matrix:
* **The Scope is ALWAYS the Surface / Component:** `(cli)`, `(action)`, `(core)`, `(canon)`, `(agents)`, `(spec)`.
* **The Type is ALWAYS the Intent:** `spec`, `feat`, `fix`, `test`, `docs`, `chore`, `build`, `ci`.

See the [Development Workflow Guide](development-workflow.md#3-making-changes--committing) for the complete surface-to-prefix mapping table.

### Pull Requests & Squash-and-Merge Policy

All submissions, including submissions by project members, require review via [GitHub pull requests](https://docs.github.com/articles/about-pull-requests).

* **Squash Merging as Repository Standard:** To support automated versioning and changelog generation via Google's `release-please`, all pull requests are squash-merged into `main` as a single atomic commit.
* **PR Title Becomes the Commit Message:** Under the squash-and-merge workflow, the **Pull Request Title** directly becomes the final commit message on `main`. Every PR title must adhere to our Conventional Commit standard (e.g. `feat(cli): add quiet flag (#20)`).
* **Iterative Branch Commits Welcome:** Granular, exploratory, or WIP commits on your personal feature branches are completely acceptable during development and review, as they will be cleanly squashed upon merge.
* **Automated PR Title Linting:** Every pull request is automatically validated by our CI title linter (`.github/workflows/pr-title-lint.yml`) to ensure compliance prior to merging.

> [!NOTE]
> **Repository Settings for Maintainers:**
> Under repository **Settings > General > Pull Requests** on `PAIR-code/canon-clerk`:
> - **Allow squash merging:** Enabled
> - **Default commit message for squash merges:** Select *"Pull request title"*
> - **Allow merge commits:** Disabled
> - **Allow rebase merging:** Disabled
> - **Automatically delete head branches:** Enabled

