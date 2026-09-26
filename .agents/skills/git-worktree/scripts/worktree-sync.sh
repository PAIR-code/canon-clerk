#!/bin/sh
set -e
set -x

# Helper script to safely synchronize the 'main' branch in a triangular Git layout.
# Updates local main and pushes to origin/main without switching branch in current worktree.
# Adheres to .agents/skills/.canons/ standards:
# - POSIX Bourne Shell
# - Command echoing via set -x
# - Unadorned plain text output

# Determine common git dir and container root
COMMON_DIR="$(git rev-parse --git-common-dir 2>/dev/null)"
if [ -z "$COMMON_DIR" ]; then
  echo "Error: Not inside a Git repository." >&2
  exit 1
fi

COMMON_DIR_ABS="$(cd "$COMMON_DIR" && pwd -P)"
CONTAINER_ROOT="$(dirname "$COMMON_DIR_ABS")"

# Locate main worktree
MAIN_WORKTREE="$(git worktree list --porcelain | awk '
  /^worktree / { wt = substr($0, 10) }
  /^branch refs\/heads\/main$/ { print wt }
')"

if [ -z "$MAIN_WORKTREE" ] || [ ! -d "$MAIN_WORKTREE" ]; then
  if [ -d "${CONTAINER_ROOT}/main" ]; then
    MAIN_WORKTREE="${CONTAINER_ROOT}/main"
  else
    echo "Error: Could not locate 'main' worktree checkout." >&2
    exit 1
  fi
fi

# Ensure main worktree is clean
DIRTY="$(git -C "$MAIN_WORKTREE" status --porcelain)"
if [ -n "$DIRTY" ]; then
  echo "Error: 'main' worktree has uncommitted modifications. Please stash or commit them before syncing." >&2
  echo "$DIRTY" >&2
  exit 1
fi

# Determine upstream remote (prefer upstream, fallback to origin)
REMOTE_UPSTREAM="upstream"
if ! git remote | grep -q '^upstream$'; then
  REMOTE_UPSTREAM="origin"
fi

# Determine origin remote for pushing
REMOTE_ORIGIN="origin"
HAS_ORIGIN=false
if git remote | grep -q '^origin$'; then
  HAS_ORIGIN=true
fi

# Fetch upstream (proceed with cached ref if network is restricted or offline)
if ! git fetch "$REMOTE_UPSTREAM" --prune; then
  if git rev-parse --verify --quiet "${REMOTE_UPSTREAM}/main" >/dev/null 2>&1; then
    echo "Warning: git fetch failed (offline or network restricted). Proceeding with cached ${REMOTE_UPSTREAM}/main." >&2
  else
    echo "Error: git fetch failed and '${REMOTE_UPSTREAM}/main' does not exist locally." >&2
    exit 1
  fi
fi

# Fast-forward main
git -C "$MAIN_WORKTREE" merge --ff-only "${REMOTE_UPSTREAM}/main"

# Push to origin/main if origin exists and differs from upstream
if [ "$HAS_ORIGIN" = true ] && [ "$REMOTE_ORIGIN" != "$REMOTE_UPSTREAM" ]; then
  if ! git -C "$MAIN_WORKTREE" push "$REMOTE_ORIGIN" main; then
    echo "Warning: git push to ${REMOTE_ORIGIN} failed (offline or network restricted)." >&2
  fi
fi

echo ""
echo "=== Main Synchronized Successfully ==="
echo "Worktree:    ${MAIN_WORKTREE}"
echo "Synced with: ${REMOTE_UPSTREAM}/main"
