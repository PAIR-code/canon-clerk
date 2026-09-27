#!/bin/sh
set -e
set -x

# Helper script to list and filter issues in Canon Clerk.
# Adheres to .agents/skills/.canons/ standards:
# - POSIX Bourne Shell
# - Command echoing via set -x
# - Unadorned plain text output (TSV)
# - Output shunting when output exceeds 8KB

run_list() {
  if ! command -v gh >/dev/null 2>&1; then
    echo "Error: GitHub CLI ('gh') is required but not installed." >&2
    exit 1
  fi

  STATE="open"
  LIMIT="50"
  LABEL=""
  AUTHOR=""
  ASSIGNEE=""
  MILESTONE=""
  SEARCH=""
  REPO=""
  FORMAT="tsv"

  while [ "$#" -gt 0 ]; do
    case "$1" in
      -s|--state)
        STATE="$2"
        shift 2
        ;;
      -l|--label)
        LABEL="$2"
        shift 2
        ;;
      -L|--limit)
        LIMIT="$2"
        shift 2
        ;;
      -a|--author)
        AUTHOR="$2"
        shift 2
        ;;
      -A|--assignee)
        ASSIGNEE="$2"
        shift 2
        ;;
      -m|--milestone)
        MILESTONE="$2"
        shift 2
        ;;
      -S|--search)
        SEARCH="$2"
        shift 2
        ;;
      -R|--repo)
        REPO="$2"
        shift 2
        ;;
      --json)
        FORMAT="json"
        shift
        ;;
      -h|--help)
        echo "Usage: $0 [options] [search-query]"
        echo "Options:"
        echo "  -s, --state <state>       Filter by state: open, closed, all (default: open)"
        echo "  -l, --label <label>       Filter by label"
        echo "  -L, --limit <num>         Maximum issues to fetch (default: 50)"
        echo "  -a, --author <login>      Filter by author"
        echo "  -A, --assignee <login>    Filter by assignee"
        echo "  -m, --milestone <title>   Filter by milestone"
        echo "  -S, --search <query>      Search issues with query"
        echo "  -R, --repo <owner/repo>   Override repository target"
        echo "  --json                    Output raw JSON"
        echo "  -h, --help                Show this help message"
        exit 0
        ;;
      -*)
        echo "Error: Unknown option '$1'." >&2
        echo "Run '$0 --help' for usage." >&2
        exit 1
        ;;
      *)
        if [ -z "$SEARCH" ]; then
          SEARCH="$1"
        else
          SEARCH="$SEARCH $1"
        fi
        shift
        ;;
    esac
  done

  # 1. Resolve repository
  if [ -z "$REPO" ]; then
    REPO="$(git config --get remote.upstream.url 2>/dev/null | sed -E 's#(git@github\.com:|https://github\.com/)([^/]+/[^/.]+)(\.git)?$#\2#' || true)"
    if [ -z "$REPO" ]; then
      REPO="$(git config --get remote.origin.url 2>/dev/null | sed -E 's#(git@github\.com:|https://github\.com/)([^/]+/[^/.]+)(\.git)?$#\2#' || true)"
    fi
    if [ -z "$REPO" ]; then
      REPO="PAIR-code/canon-clerk"
    fi
  fi

  # 2. Build gh arguments
  set -- --repo "$REPO" --state "$STATE" --limit "$LIMIT"
  if [ -n "$LABEL" ]; then
    set -- "$@" --label "$LABEL"
  fi
  if [ -n "$AUTHOR" ]; then
    set -- "$@" --author "$AUTHOR"
  fi
  if [ -n "$ASSIGNEE" ]; then
    set -- "$@" --assignee "$ASSIGNEE"
  fi
  if [ -n "$MILESTONE" ]; then
    set -- "$@" --milestone "$MILESTONE"
  fi
  if [ -n "$SEARCH" ]; then
    set -- "$@" --search "$SEARCH"
  fi

  # 3. Output formatting
  if [ "$FORMAT" = "json" ]; then
    gh issue list "$@" --json number,title,labels,state,author,assignees,milestone,createdAt,updatedAt,url
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
