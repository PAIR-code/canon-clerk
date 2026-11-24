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

  # 2. Direct inspection modes (--job <job-id> | --check-run <check-run-id>)
  if [ "$1" = "--job" ]; then
    case "$2" in
      ''|*[!0-9]*)
        echo "Error: --job requires a numeric job ID (got: '${2:-}')." >&2
        echo "Usage: $0 [pr-number]  OR  $0 --job <job-id>  OR  $0 --check-run <check-run-id>" >&2
        exit 1
        ;;
    esac
    dump_job_log "$REPO" "$2" "manual" "Direct Job Inspection"
    return 0
  fi

  if [ "$1" = "--check-run" ]; then
    case "$2" in
      ''|*[!0-9]*)
        echo "Error: --check-run requires a numeric check run ID (got: '${2:-}')." >&2
        echo "Usage: $0 [pr-number]  OR  $0 --job <job-id>  OR  $0 --check-run <check-run-id>" >&2
        exit 1
        ;;
    esac
    dump_check_run "$REPO" "$2" "manual" "Direct Check Run Inspection"
    return 0
  fi

  # 3. Resolve PR number
  PR_NUMBER=""
  if [ "$#" -ge 1 ]; then
    PR_ARG="${1#\#}"
    case "$PR_ARG" in
      ''|*[!0-9]*)
        echo "Error: PR argument must be a valid PR number (got: '$1')." >&2
        echo "Usage: $0 [pr-number]  OR  $0 --job <job-id>  OR  $0 --check-run <check-run-id>" >&2
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
      echo "Usage: $0 [pr-number]  OR  $0 --job <job-id>  OR  $0 --check-run <check-run-id>" >&2
      exit 1
    fi

    PR_NUMBER="$(gh pr list --repo "$REPO" --head "$BRANCH" --state open --json number --jq '.[0].number' 2>/dev/null || true)"
    if [ -z "$PR_NUMBER" ] || [ "$PR_NUMBER" = "null" ]; then
      PR_NUMBER="$(gh pr list --repo "$REPO" --head "$BRANCH" --state all --limit 1 --json number --jq '.[0].number' 2>/dev/null || true)"
    fi

    if [ -z "$PR_NUMBER" ] || [ "$PR_NUMBER" = "null" ]; then
      echo "Error: No pull request found for branch '$BRANCH' in repository '$REPO'." >&2
      echo "Usage: $0 [pr-number]  OR  $0 --job <job-id>  OR  $0 --check-run <check-run-id>" >&2
      exit 1
    fi
  fi

  # 4. Query failing checks using gh pr checks
  CHECKS_RC=0
  FAILING_CHECKS="$(gh pr checks "$PR_NUMBER" --repo "$REPO" --json name,state,bucket,link --jq '.[] | select(.bucket == "fail") | [.name, .state, .link] | @tsv')" || CHECKS_RC=$?
  if [ "$CHECKS_RC" -ne 0 ]; then
    echo "Error: Failed to query checks for PR #${PR_NUMBER} in repository '${REPO}' (gh exit code ${CHECKS_RC})." >&2
    return "$CHECKS_RC"
  fi

  if [ -z "$FAILING_CHECKS" ]; then
    echo "No failing checks found for PR #${PR_NUMBER}."
    return 0
  fi

  echo "=== Failing Checks for PR #${PR_NUMBER} ==="
  echo "$FAILING_CHECKS"
  echo ""

  # 5. Retrieve and dump logs or check run details for each failing check
  echo "$FAILING_CHECKS" | while IFS="$(printf '\t')" read -r NAME STATE LINK; do
    JOB_ID="$(printf '%s\n' "$LINK" | sed -E -n 's#.*/actions/runs/[0-9]+/job/([0-9]+).*#\1#p')"
    if [ -n "$JOB_ID" ]; then
      dump_job_log "$REPO" "$JOB_ID" "$PR_NUMBER" "$NAME"
      continue
    fi

    CHECK_RUN_ID=""
    case "$LINK" in
      */actions/runs/*)
        ;;
      *)
        CHECK_RUN_ID="$(printf '%s\n' "$LINK" | sed -E -n 's#.*/runs/([0-9]+)([/?#].*|$)#\1#p')"
        ;;
    esac

    if [ -n "$CHECK_RUN_ID" ]; then
      dump_check_run "$REPO" "$CHECK_RUN_ID" "$PR_NUMBER" "$NAME"
      continue
    fi

    echo "--- Check: ${NAME} (${STATE}) ---"
    echo "External check (no GitHub Actions job ID or Check Run ID): ${LINK}"
    echo ""
  done
}

dump_check_run() {
  REPO="$1"
  CHECK_RUN_ID="$2"
  PR_NUMBER="$3"
  CHECK_NAME="$4"

  REPORT_FILE="/tmp/pr-${PR_NUMBER}-check-run-${CHECK_RUN_ID}.md"

  echo "--- Check Run: ${CHECK_NAME} (ID: ${CHECK_RUN_ID}) ---"
  if ! gh api "repos/${REPO}/check-runs/${CHECK_RUN_ID}" --jq '
    "Status: " + (.status // "unknown") + " | Conclusion: " + (.conclusion // "unknown"),
    (if (.output.title // "") != "" then "\nTitle: " + .output.title else empty end),
    (if (.output.summary // "") != "" then "\n=== Summary ===\n" + .output.summary else empty end),
    (if (.output.text // "") != "" then "\n=== Details ===\n" + .output.text else empty end)
  ' > "$REPORT_FILE" 2>&1; then
    echo "Warning: Failed to fetch check run ${CHECK_RUN_ID} via gh api."
    cat "$REPORT_FILE"
    rm -f "$REPORT_FILE"
    echo ""
    return 0
  fi

  ANNOTATIONS_FILE="$(mktemp /tmp/pr-check-run-annotations.XXXXXX)"
  if gh api --paginate "repos/${REPO}/check-runs/${CHECK_RUN_ID}/annotations" --jq '
    .[] |
    "[" + (.annotation_level // "notice") + "] " +
    (.path // "<unknown>") + ":" + ((.start_line // 0) | tostring) +
    (if (.end_line != null and .end_line != .start_line) then "-" + (.end_line | tostring) else "" end) +
    (if (.title // "") != "" then " (" + .title + ")" else "" end) +
    "\n" + (.message // "") + "\n"
  ' > "$ANNOTATIONS_FILE" 2>&1; then
    if [ -s "$ANNOTATIONS_FILE" ]; then
      {
        echo ""
        echo "=== Annotations ==="
        cat "$ANNOTATIONS_FILE"
      } >> "$REPORT_FILE"
    fi
  else
    echo "Warning: Failed to fetch annotations for check run ${CHECK_RUN_ID} via gh api." >> "$REPORT_FILE"
  fi
  rm -f "$ANNOTATIONS_FILE"

  REPORT_SIZE="$(wc -c < "$REPORT_FILE" | tr -d ' ')"
  echo "Saved check run report to: ${REPORT_FILE} (${REPORT_SIZE} bytes)"
  echo ""

  MAX_BYTES=8192
  if [ "$REPORT_SIZE" -gt "$MAX_BYTES" ]; then
    echo "=== First 60 lines of check run report ==="
    head -n 60 "$REPORT_FILE"
    echo "=== End of preview ==="
    echo "View full report with: view_file ${REPORT_FILE}"
  else
    echo "=== Complete Check Run Report ==="
    cat "$REPORT_FILE"
    echo "=== End of check run report ==="
  fi
  echo ""
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
( set -e; run_failed_logs "$@" ) > "$TMP_OUT" 2>&1
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
