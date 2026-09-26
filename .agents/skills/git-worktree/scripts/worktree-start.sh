#!/bin/sh
set -e
set -x

# Helper script to scaffold a new feature worktree in a triangular Git layout.
# Adheres to .agents/skills/.canons/ standards:
# - POSIX Bourne Shell
# - Command echoing via set -x
# - Unadorned plain text output

# Parse arguments
ISSUE=""
SLUG=""

if [ "$#" -eq 2 ]; then
  ISSUE="$1"
  SLUG="$2"
elif [ "$#" -eq 1 ]; then
  case "$1" in
    *-*)
      ISSUE="${1%%-*}"
      SLUG="${1#*-}"
      ;;
    *)
      echo "Error: Single argument must be in '<issue-number>-<slug>' format." >&2
      echo "Usage: $0 <issue-number> <slug>  OR  $0 <issue-number>-<slug>" >&2
      exit 1
      ;;
  esac
else
  echo "Usage: $0 <issue-number> <slug>  OR  $0 <issue-number>-<slug>" >&2
  exit 1
fi

# Validate issue number (digits only)
case "$ISSUE" in
  ''|*[!0-9]*)
    echo "Error: Issue number must contain only digits (got: '$ISSUE')." >&2
    exit 1
    ;;
esac

# Validate slug (lowercase alphanumeric and hyphens)
case "$SLUG" in
  ''|*[!a-z0-9-]*)
    echo "Error: Slug must contain only lowercase letters, digits, and hyphens (got: '$SLUG')." >&2
    exit 1
    ;;
esac

BRANCH="${ISSUE}-${SLUG}"

# Determine common git dir and container root
COMMON_DIR="$(git rev-parse --git-common-dir 2>/dev/null)"
if [ -z "$COMMON_DIR" ]; then
  echo "Error: Not inside a Git repository." >&2
  exit 1
fi

COMMON_DIR_ABS="$(cd "$COMMON_DIR" && pwd -P)"
CONTAINER_ROOT="$(dirname "$COMMON_DIR_ABS")"

TARGET_DIR="${CONTAINER_ROOT}/${BRANCH}"

if [ -d "$TARGET_DIR" ]; then
  echo "Error: Worktree directory '$TARGET_DIR' already exists." >&2
  exit 1
fi

if git rev-parse --verify --quiet "refs/heads/${BRANCH}" >/dev/null 2>&1; then
  echo "Error: Local branch '${BRANCH}' already exists." >&2
  exit 1
fi

# Determine upstream remote (prefer upstream, fall back to origin)
REMOTE="upstream"
if ! git remote | grep -q '^upstream$'; then
  REMOTE="origin"
fi

# Fetch latest refs from remote (proceed with cached ref if network/offline fails)
if ! git fetch "$REMOTE" --prune; then
  if git rev-parse --verify --quiet "${REMOTE}/main" >/dev/null 2>&1; then
    echo "Warning: git fetch failed (offline or network restricted). Proceeding with cached ${REMOTE}/main." >&2
  else
    echo "Error: git fetch failed and '${REMOTE}/main' does not exist locally." >&2
    exit 1
  fi
fi

# Create the worktree and branch
git -C "$CONTAINER_ROOT" worktree add -b "$BRANCH" "$BRANCH" "${REMOTE}/main"

echo ""
echo "=== Worktree Created Successfully ==="
echo "Branch:    ${BRANCH}"
echo "Directory: ${TARGET_DIR}"
echo "Next step: Set your working directory context to the newly created worktree:"
echo "cd ${TARGET_DIR}"
