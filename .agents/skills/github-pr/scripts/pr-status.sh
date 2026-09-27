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

  # 2. Parse flags and PR number
  WATCH=false
  PR_NUMBER=""

  for arg in "$@"; do
    case "$arg" in
      -w|--watch)
        WATCH=true
        ;;
      *)
        PR_ARG="${arg#\#}"
        case "$PR_ARG" in
          ''|*[!0-9]*)
            echo "Error: Unexpected or invalid argument '$arg'." >&2
            echo "Usage: $0 [--watch] [pr-number]" >&2
            exit 1
            ;;
          *)
            if [ -n "$PR_NUMBER" ]; then
              echo "Error: Unexpected additional argument '$arg'." >&2
              echo "Usage: $0 [--watch] [pr-number]" >&2
              exit 1
            fi
            PR_NUMBER="$PR_ARG"
            ;;
        esac
        ;;
    esac
  done

  if [ -z "$PR_NUMBER" ]; then
    BRANCH="$(git branch --show-current 2>/dev/null || git rev-parse --abbrev-ref HEAD 2>/dev/null || true)"
    if [ -z "$BRANCH" ] || [ "$BRANCH" = "HEAD" ] || [ "$BRANCH" = "main" ] || [ "$BRANCH" = "master" ]; then
      echo "Error: Cannot auto-detect PR for branch '$BRANCH'. Please specify a PR number." >&2
      echo "Usage: $0 [--watch] [pr-number]" >&2
      exit 1
    fi

    PR_NUMBER="$(gh pr list --repo "$REPO" --head "$BRANCH" --state open --json number --jq '.[0].number' 2>/dev/null || true)"
    if [ -z "$PR_NUMBER" ] || [ "$PR_NUMBER" = "null" ]; then
      PR_NUMBER="$(gh pr list --repo "$REPO" --head "$BRANCH" --state all --limit 1 --json number --jq '.[0].number' 2>/dev/null || true)"
    fi

    if [ -z "$PR_NUMBER" ] || [ "$PR_NUMBER" = "null" ]; then
      echo "Error: No pull request found for branch '$BRANCH' in repository '$REPO'." >&2
      echo "Usage: $0 [--watch] [pr-number]" >&2
      exit 1
    fi
  fi

  # 3. Direct output of standard gh commands
  gh pr view "$PR_NUMBER" --repo "$REPO"
  echo ""
  RC=0
  if [ "$WATCH" = true ]; then
    gh pr checks "$PR_NUMBER" --repo "$REPO" --watch || RC=$?
  else
    gh pr checks "$PR_NUMBER" --repo "$REPO" || RC=$?
    if [ "$RC" -eq 8 ]; then
      echo ""
      echo "Notice: Checks are still in progress."
      echo "To watch until all checks complete, run:"
      echo "  ./.agents/skills/github-pr/scripts/pr-status.sh --watch $PR_NUMBER"
      RC=0
    fi
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
  tail -n 60 "$SHUNTED_LOG"
  echo ""
  echo "[OUTPUT TRUNCATED: Output size (${SIZE} bytes) exceeds 8KB limit]"
  echo "Complete status report written to: ${SHUNTED_LOG}"
else
  cat "$TMP_OUT"
  rm -f "$TMP_OUT"
fi

exit "$EXIT_CODE"
