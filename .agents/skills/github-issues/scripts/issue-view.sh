#!/bin/sh
set -e
set -x

# Helper script to inspect a specific issue in Canon Clerk.
# Adheres to .agents/skills/.canons/ standards:
# - POSIX Bourne Shell
# - Command echoing via set -x
# - Unadorned plain text output
# - Output shunting when output exceeds 8KB

run_view() {
  if ! command -v gh >/dev/null 2>&1; then
    echo "Error: GitHub CLI ('gh') is required but not installed." >&2
    exit 1
  fi

  ISSUE_NUMBER=""
  INCLUDE_COMMENTS=false
  FORMAT="markdown"
  REPO=""

  while [ "$#" -gt 0 ]; do
    case "$1" in
      -c|--comments)
        INCLUDE_COMMENTS=true
        shift
        ;;
      --json)
        FORMAT="json"
        shift
        ;;
      -R|--repo)
        REPO="$2"
        shift 2
        ;;
      -h|--help)
        echo "Usage: $0 <issue-number> [options]"
        echo "Options:"
        echo "  -c, --comments            Include issue comments"
        echo "  --json                    Output raw JSON"
        echo "  -R, --repo <owner/repo>   Override repository target"
        echo "  -h, --help                Show this help message"
        exit 0
        ;;
      -*)
        echo "Error: Unknown option '$1'." >&2
        echo "Run '$0 --help' for usage." >&2
        exit 1
        ;;
      *)
        if [ -z "$ISSUE_NUMBER" ]; then
          ISSUE_NUMBER="${1#\#}"
        else
          echo "Error: Unexpected argument '$1'." >&2
          exit 1
        fi
        shift
        ;;
    esac
  done

  if [ -z "$ISSUE_NUMBER" ]; then
    echo "Error: Issue number is required." >&2
    echo "Usage: $0 <issue-number> [options]" >&2
    exit 1
  fi

  case "$ISSUE_NUMBER" in
    ''|*[!0-9]*)
      echo "Error: Issue number must be digits only (got: '$ISSUE_NUMBER')." >&2
      exit 1
      ;;
  esac

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

  # 2. Output
  if [ "$FORMAT" = "json" ]; then
    if [ "$INCLUDE_COMMENTS" = true ]; then
      gh issue view "$ISSUE_NUMBER" --repo "$REPO" --json number,title,state,author,labels,assignees,milestone,createdAt,updatedAt,url,body,comments,closedByPullRequestsReferences
    else
      gh issue view "$ISSUE_NUMBER" --repo "$REPO" --json number,title,state,author,labels,assignees,milestone,createdAt,updatedAt,url,body,closedByPullRequestsReferences
    fi
  else
    if [ "$INCLUDE_COMMENTS" = true ]; then
      gh issue view "$ISSUE_NUMBER" --repo "$REPO" --json number,title,state,author,labels,assignees,milestone,createdAt,updatedAt,url,body,closedByPullRequestsReferences,comments --template 'number:	#{{.number}}
title:	{{.title}}
state:	{{.state}}
author:	{{.author.login}}
url:	{{.url}}
labels:	{{range $i, $l := .labels}}{{if $i}}, {{end}}{{$l.name}}{{end}}
assignees:	{{range $i, $a := .assignees}}{{if $i}}, {{end}}{{$a.login}}{{end}}
milestone:	{{if .milestone}}{{.milestone.title}}{{else}}none{{end}}
created:	{{.createdAt}}
updated:	{{.updatedAt}}
{{if .closedByPullRequestsReferences}}closed_by_pr:	{{range $i, $pr := .closedByPullRequestsReferences}}{{if $i}}, {{end}}#{{$pr.number}} ({{$pr.url}}){{end}}
{{end}}
-- body --
{{.body}}
{{if .comments}}
-- comments --
{{range .comments}}
[{{.author.login}} @ {{.createdAt}}]
{{.body}}
{{end}}{{end}}'
    else
      gh issue view "$ISSUE_NUMBER" --repo "$REPO" --json number,title,state,author,labels,assignees,milestone,createdAt,updatedAt,url,body,closedByPullRequestsReferences --template 'number:	#{{.number}}
title:	{{.title}}
state:	{{.state}}
author:	{{.author.login}}
url:	{{.url}}
labels:	{{range $i, $l := .labels}}{{if $i}}, {{end}}{{$l.name}}{{end}}
assignees:	{{range $i, $a := .assignees}}{{if $i}}, {{end}}{{$a.login}}{{end}}
milestone:	{{if .milestone}}{{.milestone.title}}{{else}}none{{end}}
created:	{{.createdAt}}
updated:	{{.updatedAt}}
{{if .closedByPullRequestsReferences}}closed_by_pr:	{{range $i, $pr := .closedByPullRequestsReferences}}{{if $i}}, {{end}}#{{$pr.number}} ({{$pr.url}}){{end}}
{{end}}
-- body --
{{.body}}'
    fi
  fi
}

# Output shunting when exceeding 8KB per canon
TMP_OUT="$(mktemp /tmp/issue-view-out.XXXXXX)"
set +e
( run_view "$@" ) > "$TMP_OUT" 2>&1
EXIT_CODE=$?
set -e

SIZE="$(wc -c < "$TMP_OUT" | tr -d ' ')"
MAX_BYTES=8192

# Extract issue number for filename if present in args
TARGET_ID="view"
for arg in "$@"; do
  case "$arg" in
    [0-9]*|\#[0-9]*)
      TARGET_ID="${arg#\#}"
      break
      ;;
  esac
done

if [ "$SIZE" -gt "$MAX_BYTES" ]; then
  SHUNTED_FILE="/tmp/issue-${TARGET_ID}-$(date +%s).md"
  mv "$TMP_OUT" "$SHUNTED_FILE"
  head -n 50 "$SHUNTED_FILE"
  echo ""
  echo "[OUTPUT TRUNCATED: Issue content (${SIZE} bytes) exceeds 8KB limit]"
  echo "Complete issue content written to: ${SHUNTED_FILE}"
else
  cat "$TMP_OUT"
  rm -f "$TMP_OUT"
fi

exit "$EXIT_CODE"
