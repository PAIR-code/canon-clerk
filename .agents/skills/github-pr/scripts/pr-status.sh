#!/bin/sh
set -e
set -x

# Helper script to inspect pull request status and CI checks.
# Adheres to .agents/skills/.canons/ standards:
# - POSIX Bourne Shell
# - Command echoing via set -x
# - Unadorned plain text output
# - Output shunting when output exceeds 8KB

run_status() {
  if ! command -v gh >/dev/null 2>&1; then
    echo "Error: GitHub CLI ('gh') is required but not installed." >&2
    exit 1
  fi

  # 1. Resolve repository
  REPO="$(git config --get remote.upstream.url 2>/dev/null | sed -E 's#(git@github\.com:|https://github\.com/)([^/]+/[^/.]+)(\.git)?$#\2#' || true)"
  if [ -z "$REPO" ]; then
    REPO="$(git config --get remote.origin.url 2>/dev/null | sed -E 's#(git@github\.com:|https://github\.com/)([^/]+/[^/.]+)(\.git)?$#\2#' || true)"
  fi
  if [ -z "$REPO" ]; then
    REPO="PAIR-code/canon-clerk"
  fi

  # 2. Resolve PR number
  PR_NUMBER=""
  if [ "$#" -ge 1 ]; then
    PR_ARG="${1#\#}"
    case "$PR_ARG" in
      ''|*[!0-9]*)
        echo "Error: PR argument must be a valid PR number (got: '$1')." >&2
        echo "Usage: $0 [pr-number]" >&2
        exit 1
        ;;
      *)
        PR_NUMBER="$PR_ARG"
        ;;
    esac
  else
    BRANCH="$(git branch --show-current 2>/dev/null || git rev-parse --abbrev-ref HEAD 2>/dev/null || true)"
    if [ -z "$BRANCH" ] || [ "$BRANCH" = "HEAD" ] || [ "$BRANCH" = "main" ] || [ "$BRANCH" = "master" ]; then
      echo "Error: Cannot auto-detect PR for branch '$BRANCH'. Please specify a PR number." >&2
      echo "Usage: $0 [pr-number]" >&2
      exit 1
    fi

    PR_NUMBER="$(gh pr list --repo "$REPO" --head "$BRANCH" --state open --json number --jq '.[0].number' 2>/dev/null || true)"
    if [ -z "$PR_NUMBER" ] || [ "$PR_NUMBER" = "null" ]; then
      PR_NUMBER="$(gh pr list --repo "$REPO" --head "$BRANCH" --state all --limit 1 --json number --jq '.[0].number' 2>/dev/null || true)"
    fi

    if [ -z "$PR_NUMBER" ] || [ "$PR_NUMBER" = "null" ]; then
      echo "Error: No pull request found for branch '$BRANCH' in repository '$REPO'." >&2
      echo "Usage: $0 [pr-number]" >&2
      exit 1
    fi
  fi

  # 3. Direct output of standard gh commands
  gh pr view "$PR_NUMBER" --repo "$REPO"
  echo ""
  RC=0
  gh pr checks "$PR_NUMBER" --repo "$REPO" || RC=$?
  if [ "$RC" -eq 8 ]; then
    RC=0
  fi
  return "$RC"
}

# Output shunting when exceeding 8KB per canon
TMP_OUT="$(mktemp /tmp/pr-status-out.XXXXXX)"
set +e
( run_status "$@" ) > "$TMP_OUT" 2>&1
EXIT_CODE=$?
set -e

SIZE="$(wc -c < "$TMP_OUT" | tr -d ' ')"
MAX_BYTES=8192

if [ "$SIZE" -gt "$MAX_BYTES" ]; then
  SHUNTED_LOG="/tmp/pr-status-$(date +%s).log"
  mv "$TMP_OUT" "$SHUNTED_LOG"
  head -n 60 "$SHUNTED_LOG"
  echo ""
  echo "[OUTPUT TRUNCATED: Output size (${SIZE} bytes) exceeds 8KB limit]"
  echo "Complete status report written to: ${SHUNTED_LOG}"
else
  cat "$TMP_OUT"
  rm -f "$TMP_OUT"
fi

exit "$EXIT_CODE"
