#!/bin/sh
set -e
set -x

# Helper script to execute a 4-step teardown of a completed/merged worktree:
# 1. Removes the worktree directory (git worktree remove)
# 2. Force-deletes the local branch (git branch -D)
# 3. Deletes the remote branch on origin if present (git push origin --delete)
# 4. Prunes stale worktree metadata (git worktree prune)
# Adheres to .agents/skills/.canons/ standards:
# - POSIX Bourne Shell
# - Command echoing via set -x
# - Unadorned plain text output

FORCE=false
TARGET=""

# Parse flags and arguments
for arg in "$@"; do
  case "$arg" in
    -f|--force)
      FORCE=true
      ;;
    *)
      if [ -z "$TARGET" ]; then
        TARGET="$arg"
      else
        echo "Error: Unexpected additional argument '$arg'." >&2
        echo "Usage: $0 [-f|--force] <branch-or-worktree>" >&2
        exit 1
      fi
      ;;
  esac
done

COMMON_DIR="$(git rev-parse --git-common-dir 2>/dev/null)"
if [ -z "$COMMON_DIR" ]; then
  echo "Error: Not inside a Git repository." >&2
  exit 1
fi

COMMON_DIR_ABS="$(cd "$COMMON_DIR" && pwd -P)"
CONTAINER_ROOT="$(dirname "$COMMON_DIR_ABS")"

# If no target specified, check current branch
if [ -z "$TARGET" ]; then
  CURRENT_BRANCH="$(git branch --show-current 2>/dev/null || true)"
  if [ -n "$CURRENT_BRANCH" ] && [ "$CURRENT_BRANCH" != "main" ] && [ "$CURRENT_BRANCH" != "master" ]; then
    TARGET="$CURRENT_BRANCH"
  else
    echo "Usage: $0 [-f|--force] <branch-or-worktree>" >&2
    exit 1
  fi
fi

# Strip potential trailing slashes
TARGET="${TARGET%/}"

BRANCH=""
WORKTREE_PATH=""

# Check if TARGET is an issue number (e.g. '13')
case "$TARGET" in
  ''|*[!0-9]*)
    ;;
  *)
    MATCHED_BRANCH="$(git for-each-ref --format='%(refname:short)' refs/heads/ | grep "^${TARGET}-" | head -n 1 || true)"
    if [ -n "$MATCHED_BRANCH" ]; then
      BRANCH="$MATCHED_BRANCH"
    fi
    ;;
esac

if [ -z "$BRANCH" ]; then
  if git rev-parse --verify --quiet "refs/heads/${TARGET}" >/dev/null 2>&1; then
    BRANCH="$TARGET"
  else
    BASE="$(basename "$TARGET")"
    if git rev-parse --verify --quiet "refs/heads/${BASE}" >/dev/null 2>&1; then
      BRANCH="$BASE"
    fi
  fi
fi

# Protect primary branches
if [ "$BRANCH" = "main" ] || [ "$BRANCH" = "master" ] || [ "$TARGET" = "main" ] || [ "$TARGET" = "master" ]; then
  echo "Error: Refusing to finish or delete protected branch 'main'." >&2
  exit 1
fi

# Find worktree path for this branch
if [ -n "$BRANCH" ]; then
  WORKTREE_PATH="$(git worktree list --porcelain | awk -v b="refs/heads/${BRANCH}" '
    /^worktree / { wt = substr($0, 10) }
    $1 == "branch" && $2 == b { print wt }
  ')"
fi

# If worktree path not found by branch, check if TARGET itself is a valid directory
if [ -z "$WORKTREE_PATH" ]; then
  if [ -d "$TARGET" ]; then
    WORKTREE_PATH="$(cd "$TARGET" && pwd -P)"
  elif [ -d "${CONTAINER_ROOT}/${TARGET}" ]; then
    WORKTREE_PATH="${CONTAINER_ROOT}/${TARGET}"
  fi
fi

# If BRANCH is still unknown but WORKTREE_PATH is known, determine branch from worktree list
if [ -z "$BRANCH" ] && [ -n "$WORKTREE_PATH" ]; then
  BRANCH="$(git worktree list --porcelain | awk -v p="${WORKTREE_PATH}" '
    $1 == "worktree" && $2 == p { match_wt = 1 }
    match_wt && $1 == "branch" { sub("refs/heads/", "", $2); print $2; exit }
    /^$/ { match_wt = 0 }
  ')"
fi

CURRENT_PWD="$(pwd -P)"
# If currently inside the worktree being deleted, cd to container root first
if [ -n "$WORKTREE_PATH" ]; then
  case "$CURRENT_PWD" in
    "$WORKTREE_PATH"|"$WORKTREE_PATH"/*)
      echo "Notice: Current working directory is inside the worktree to be removed. Moving to $CONTAINER_ROOT."
      cd "$CONTAINER_ROOT"
      ;;
  esac
fi

# 1. Remove worktree directory
if [ -n "$WORKTREE_PATH" ] && [ -d "$WORKTREE_PATH" ]; then
  if [ "$FORCE" = true ]; then
    git -C "$CONTAINER_ROOT" worktree remove --force "$WORKTREE_PATH"
  else
    git -C "$CONTAINER_ROOT" worktree remove "$WORKTREE_PATH"
  fi
fi

# 2. Force delete local branch
if [ -n "$BRANCH" ] && git rev-parse --verify --quiet "refs/heads/${BRANCH}" >/dev/null 2>&1; then
  git -C "$CONTAINER_ROOT" branch -D "$BRANCH"
fi

# 3. Delete remote branch on origin if it exists
if [ -n "$BRANCH" ] && git remote | grep -q '^origin$'; then
  if git ls-remote --heads origin "$BRANCH" 2>/dev/null | grep -q "refs/heads/${BRANCH}"; then
    git push origin --delete "$BRANCH"
  fi
fi

# 4. Prune worktrees
git -C "$CONTAINER_ROOT" worktree prune

echo ""
echo "=== Worktree Teardown Complete ==="
echo "Removed branch:   ${BRANCH}"
if [ -n "$WORKTREE_PATH" ]; then
  echo "Removed worktree: ${WORKTREE_PATH}"
fi
