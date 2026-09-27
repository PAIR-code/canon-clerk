#!/bin/sh
set -e
set -x

# Helper script to list and filter issues in Canon Clerk.
# Adheres to .agents/skills/.canons/ standards:
# - POSIX Bourne Shell
# - Command echoing via set -x
# - Unadorned plain text output (TSV)
# - Direct argument passthrough to gh issue list
# - Output shunting when output exceeds 8KB

run_list() {
  if ! command -v gh >/dev/null 2>&1; then
    echo "Error: GitHub CLI ('gh') is required but not installed." >&2
    exit 1
  fi

  # 1. Resolve canonical repository
  REPO="$(git config --get remote.upstream.url 2>/dev/null | sed -E 's#(git@github\.com:|https://github\.com/)([^/]+/[^/.]+)(\.git)?$#\2#' || true)"
  if [ -z "$REPO" ]; then
    REPO="$(git config --get remote.origin.url 2>/dev/null | sed -E 's#(git@github\.com:|https://github\.com/)([^/]+/[^/.]+)(\.git)?$#\2#' || true)"
  fi
  if [ -z "$REPO" ]; then
    REPO="PAIR-code/canon-clerk"
  fi

  # 2. Check for repo, limit, or json overrides in "$@"
  HAS_REPO=false
  HAS_LIMIT=false
  IS_JSON=false

  for arg in "$@"; do
    case "$arg" in
      -R|--repo|--repo=*) HAS_REPO=true ;;
      -L|--limit|--limit=*) HAS_LIMIT=true ;;
      --json) IS_JSON=true ;;
      -h|--help)
        echo "Usage: $0 [query | gh issue list flags...]"
        echo ""
        echo "Plumbing wrapper around 'gh issue list' that auto-resolves triangular remotes,"
        echo "formats unadorned TSV tables, and shunts outputs exceeding 8KB."
        echo ""
        echo "Examples:"
        echo "  $0                          # List open issues (default limit: 50)"
        echo "  $0 worktree                 # Convenience positional search"
        echo "  $0 --label enhancement      # Pass any gh issue list flags directly"
        echo "  $0 --label bug --label ui   # Supports multiple repeating flags"
        echo "  $0 --state all --limit 100  # Custom state and limit"
        echo "  $0 --json                   # Output raw JSON"
        exit 0
        ;;
    esac
  done

  # 3. Convenience: bare positional word treated as --search
  case "$1" in
    ""|-*) ;;
    *)
      QUERY="$1"
      shift
      set -- --search "$QUERY" "$@"
      ;;
  esac

  # 4. Inject defaults if not provided
  if [ "$HAS_REPO" = false ]; then
    set -- --repo "$REPO" "$@"
  fi
  if [ "$HAS_LIMIT" = false ] && [ "$IS_JSON" = false ]; then
    set -- --limit 50 "$@"
  fi

  # 5. Output
  if [ "$IS_JSON" = true ]; then
    gh issue list "$@"
  else
    printf "NUMBER\tSTATE\tLABELS\tUPDATED\tTITLE\n"
    gh issue list "$@" --json number,title,labels,state,updatedAt --jq '.[] | ["#" + (.number|tostring), .state, ([.labels[].name] | join(",")), .updatedAt, .title] | @tsv'
  fi
}

# Output shunting when exceeding 8KB per canon
TMP_OUT="$(mktemp /tmp/issue-list-out.XXXXXX)"
set +e
( run_list "$@" ) > "$TMP_OUT" 2>&1
EXIT_CODE=$?
set -e

SIZE="$(wc -c < "$TMP_OUT" | tr -d ' ')"
MAX_BYTES=8192

if [ "$SIZE" -gt "$MAX_BYTES" ]; then
  SHUNTED_LOG="/tmp/issue-list-$(date +%s).log"
  mv "$TMP_OUT" "$SHUNTED_LOG"
  head -n 60 "$SHUNTED_LOG"
  echo ""
  echo "[OUTPUT TRUNCATED: Output size (${SIZE} bytes) exceeds 8KB limit]"
  echo "Complete issue list written to: ${SHUNTED_LOG}"
else
  cat "$TMP_OUT"
  rm -f "$TMP_OUT"
fi

exit "$EXIT_CODE"
