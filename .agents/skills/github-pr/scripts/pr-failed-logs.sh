#!/bin/sh
set -e
set -x

# Helper script to retrieve and inspect failing CI logs on GitHub pull requests.
# Adheres to .agents/skills/.canons/ standards:
# - POSIX Bourne Shell
# - Command echoing via set -x
# - Unadorned plain text output
# - Output shunting when output exceeds 8KB

run_failed_logs() {
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

  # 2. Direct Job ID mode (--job <job-id>)
  if [ "$1" = "--job" ] && [ -n "$2" ]; then
    dump_job_log "$REPO" "$2" "manual" "Direct Job Inspection"
    return 0
  fi

  # 3. Resolve PR number
  PR_NUMBER=""
  if [ "$#" -ge 1 ]; then
    PR_ARG="${1#\#}"
    case "$PR_ARG" in
      ''|*[!0-9]*)
        echo "Error: PR argument must be a valid PR number (got: '$1')." >&2
        echo "Usage: $0 [pr-number]  OR  $0 --job <job-id>" >&2
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
      echo "Usage: $0 [pr-number]  OR  $0 --job <job-id>" >&2
      exit 1
    fi

    PR_NUMBER="$(gh pr list --repo "$REPO" --head "$BRANCH" --state open --json number --jq '.[0].number' 2>/dev/null || true)"
    if [ -z "$PR_NUMBER" ] || [ "$PR_NUMBER" = "null" ]; then
      PR_NUMBER="$(gh pr list --repo "$REPO" --head "$BRANCH" --state all --limit 1 --json number --jq '.[0].number' 2>/dev/null || true)"
    fi

    if [ -z "$PR_NUMBER" ] || [ "$PR_NUMBER" = "null" ]; then
      echo "Error: No pull request found for branch '$BRANCH' in repository '$REPO'." >&2
      echo "Usage: $0 [pr-number]  OR  $0 --job <job-id>" >&2
      exit 1
    fi
  fi

  # 4. Query failing checks using gh pr checks
  FAILING_CHECKS="$(gh pr checks "$PR_NUMBER" --repo "$REPO" --json name,state,bucket,link --jq '.[] | select(.bucket == "fail") | [.name, .state, .link] | @tsv')"

  if [ -z "$FAILING_CHECKS" ]; then
    echo "No failing checks found for PR #${PR_NUMBER}."
    return 0
  fi

  echo "=== Failing Checks for PR #${PR_NUMBER} ==="
  echo "$FAILING_CHECKS"
  echo ""

  # 5. Retrieve and dump logs for each failing check
  echo "$FAILING_CHECKS" | while IFS="$(printf '\t')" read -r NAME STATE LINK; do
    JOB_ID="$(echo "$LINK" | sed -E -n 's#.*/actions/runs/[0-9]+/job/([0-9]+).*#\1#p')"

    if [ -z "$JOB_ID" ]; then
      echo "--- Check: ${NAME} (${STATE}) ---"
      echo "External check (no GitHub Actions job ID): ${LINK}"
      echo ""
      continue
    fi

    dump_job_log "$REPO" "$JOB_ID" "$PR_NUMBER" "$NAME"
  done
}

dump_job_log() {
  REPO="$1"
  JOB_ID="$2"
  PR_NUMBER="$3"
  CHECK_NAME="$4"

  LOG_FILE="/tmp/pr-${PR_NUMBER}-job-${JOB_ID}.log"

  echo "--- Job: ${CHECK_NAME} (ID: ${JOB_ID}) ---"
  if ! gh api --allow-escape-sequences "repos/${REPO}/actions/jobs/${JOB_ID}/logs" > "$LOG_FILE" 2>&1; then
    echo "Warning: Failed to fetch logs for job ID ${JOB_ID} via gh api."
    rm -f "$LOG_FILE"
    echo ""
    return 0
  fi

  # Strip terminal escapes from the saved log
  sed -i -e 's/\x1b\[[0-9;]*[a-zA-Z]//g' -e 's/\r//g' "$LOG_FILE"
  LOG_SIZE="$(wc -c < "$LOG_FILE" | tr -d ' ')"

  echo "Saved raw log to: ${LOG_FILE} (${LOG_SIZE} bytes)"
  echo ""

  MAX_BYTES=8192
  if [ "$LOG_SIZE" -gt "$MAX_BYTES" ]; then
    echo "=== Last 60 lines of log ==="
    tail -n 60 "$LOG_FILE"
    echo "=== End of tail ==="
    echo "View full log with: view_file ${LOG_FILE}"
  else
    echo "=== Complete Job Log ==="
    cat "$LOG_FILE"
    echo "=== End of log ==="
  fi
  echo ""
}

# Output shunting when exceeding 8KB per canon
TMP_OUT="$(mktemp /tmp/pr-failed-logs-out.XXXXXX)"
set +e
( run_failed_logs "$@" ) > "$TMP_OUT" 2>&1
EXIT_CODE=$?
set -e

SIZE="$(wc -c < "$TMP_OUT" | tr -d ' ')"
MAX_BYTES=8192

if [ "$SIZE" -gt "$MAX_BYTES" ]; then
  SHUNTED_LOG="/tmp/pr-failed-logs-$(date +%s).log"
  mv "$TMP_OUT" "$SHUNTED_LOG"
  head -n 60 "$SHUNTED_LOG"
  echo ""
  echo "[OUTPUT TRUNCATED: Output size (${SIZE} bytes) exceeds 8KB limit]"
  echo "Complete log written to: ${SHUNTED_LOG}"
else
  cat "$TMP_OUT"
  rm -f "$TMP_OUT"
fi

exit "$EXIT_CODE"
