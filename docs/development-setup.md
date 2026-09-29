# Development Setup

This guide walks you through setting up your local development environment for contributing to **Canon Clerk**.

Canon Clerk recommends a **triangular Git workflow** combined with **Git worktrees**. This setup allows you to juggle multiple concurrent pull requests, test against separate branches simultaneously, and collaborate seamlessly with AI coding assistants—all while sharing a single local Git object store.

---

## 1. Forking Canon Clerk

Canon Clerk follows the standard open-source contribution model where contributions originate from your personal fork:

1. Navigate to [`PAIR-code/canon-clerk`](https://github.com/PAIR-code/canon-clerk) on GitHub.
2. Click **Fork** (or use the GitHub CLI: `gh repo fork PAIR-code/canon-clerk`).
3. Your fork will be available at `git@github.com:<your-username>/canon-clerk.git`.

In Git terminology:
- **`upstream`**: The canonical repository (`git@github.com:PAIR-code/canon-clerk.git`).
- **`origin`**: Your personal fork (`git@github.com:<your-username>/canon-clerk.git`).

---

## 2. Triangular Git Worktree Setup

### Architecture Overview

A traditional clone keeps one working directory per repository. When working on multiple issues or using AI assistants, you would otherwise need to frequently stash changes, switch branches, or maintain multiple redundant clones.

With a **bare clone + worktrees**, your directory structure looks like this:

```text
canon-clerk/                      # Workspace root (container folder)
├── .bare/                        # Bare Git repository (database, objects, refs)
├── .git                          # Pointer file: "gitdir: ./.bare"
├── AGENTS.md                     # Symlink: ./main/AGENTS.md
├── .agents/                      # Symlink: ./main/.agents
├── main/                         # Worktree tracking upstream/main
└── <issue-number>-<slug>/        # Dedicated worktrees for feature branches
```

In a **triangular workflow**:
- You fetch updates directly from `upstream/main`.
- You push your feature branches to `origin/<branch>`.
- Pull requests are opened from `origin:<branch>` to `upstream:main`.
- Local `main` stays pristine and never drifts out of sync with `upstream`.

### Step-by-Step Installation

#### 1. Clone your fork as a bare repository
Create the workspace directory and clone your fork into a hidden `.bare` directory:

```bash
# Replace <your-username> with your GitHub handle
git clone --bare git@github.com:<your-username>/canon-clerk.git .bare
```

#### 2. Fix the bare clone fetch refspec (Crucial Gotcha)
By default, Git configures `--bare` clones with a fetch refspec (`+refs/heads/*:refs/heads/*`) that directly overwrites local branches on fetch. To make it behave like a standard repository with remote-tracking branches (`refs/remotes/origin/*`), run:

```bash
git --git-dir=.bare config remote.origin.fetch "+refs/heads/*:refs/remotes/origin/*"
```

#### 3. Add canonical `upstream` and fetch
Add the canonical repository as `upstream` and fetch all remotes:

```bash
git --git-dir=.bare remote add upstream git@github.com:PAIR-code/canon-clerk.git
git --git-dir=.bare fetch --all --prune
```

#### 4. Configure triangular push defaults
Configure Git so that any push defaults to your personal fork (`origin`), matching branch names:

```bash
git --git-dir=.bare config remote.pushDefault origin
git --git-dir=.bare config push.default current
```

#### 5. Create the root `.git` pointer
Create a `.git` pointer file at the root of the workspace directory. This allows Git commands, linters, and editor extensions executed from the workspace root to automatically find the `.bare` repository:

```bash
echo "gitdir: ./.bare" > .git
```

#### 6. Create the `main` worktree
Create your primary `main` working directory tracking `upstream/main`:

```bash
git worktree add -B main main upstream/main
```

---

## 3. Node.js & Toolchain Setup

Canon Clerk is built as a TypeScript monorepo using npm workspaces, `tsup`, and `vitest`.

### Prerequisites: Node.js 24

The repository strictly requires **Node.js 24 (`^24.0.0`)** and npm (enforced via `.npmrc` with `engine-strict=true` and `package.json` engines).

If you use a Node version manager such as `nvm`:

```bash
nvm install 24
nvm use 24
```

Verify your active version:

```bash
node -v   # Should output v24.x.x
npm -v
```

### Monorepo Installation

Always run `npm install` inside a worktree directory (e.g. `main/` or a feature worktree), **never** in the workspace container super-root:

```bash
cd main
npm install
```

Running `npm install`:
1. **Links workspaces:** Resolves and cross-links monorepo workspace packages (`@canon-clerk/schema`, `@canon-clerk/cli`, `@canon-clerk/action`).
2. **Installs development dependencies:** Installs the compiler, bundler (`tsup`), test runner (`vitest`), and static analysis tools.

### Validating Your Setup

Verify that the toolchain is working and all tests pass:

```bash
npm run check
```

---

## 4. Setting Up GitHub CLI (`gh`)

The [GitHub CLI](https://cli.github.com/) (`gh`) is recommended for managing issues, pull requests, and reviews directly from your terminal.

### Authentication & Protocol

Set SSH as the preferred Git protocol and log in:

```bash
gh config set git_protocol ssh
gh auth login
```

### Setting Default Remote Repository (Crucial Gotcha)

Because our repository setup includes both `origin` (fork) and `upstream` (canonical), `gh` needs to know which repository to target by default when listing issues or creating PRs.

Run this inside any worktree (e.g. `main/`):

```bash
gh repo set-default PAIR-code/canon-clerk
```

This ensures commands like `gh issue list` and `gh pr create` automatically communicate with canonical upstream instead of failing or querying your personal fork.

### Linux Keyring / Headless Troubleshooting

If you are running on Linux without an active desktop secret service (e.g. remote SSH session, headless server, tmux, or custom window managers), `gh auth login` may hang or fail when interacting with `gnome-keyring`.

Recommended solution: Open a **remote desktop session** in which to run your `gh auth login`. This way, you'll be able to see and respond to the keyring dialog.

---

## 5. Working with AI Coding Assistants

If you use AI coding assistants (such as Antigravity, Cursor, Claude Code, or Copilot):

- **Opening the Workspace:** You can open your AI editor either at the **workspace container root** (`canon-clerk/`, strongly recommended) or directly inside a **specific worktree** (`canon-clerk/<issue-number>-<slug>/`).
- **Super-Root Symlinks:** If opening at the workspace container root, create symlinks to [`AGENTS.md`](../AGENTS.md) and [`.agents/`](../.agents/) so assistants automatically discover orientation rules and skills upon launch:
  ```bash
  # From the workspace container root
  ln -s ./main/AGENTS.md AGENTS.md
  ln -s ./main/.agents .agents
  ```

---

## Next Steps


Now that your local machine and repository are configured, see the [Development Workflow Guide](development-workflow.md) for day-to-day practices on picking up issues, managing worktrees, and submitting pull requests.
