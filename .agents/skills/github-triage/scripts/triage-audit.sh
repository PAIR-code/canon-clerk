#!/bin/sh
set -e
set -x

# Helper script to audit Conventional Commit label coverage in Canon Clerk.
# Adheres to .agents/skills/.canons/ standards:
# - POSIX Bourne Shell
# - Command echoing via set -x
# - Unadorned plain text output (TSV)
# - Composite deterministic extraction (title -> type/scope) cross-referenced
#   against .github/labels.yml
# - Output shunting when output exceeds 8KB
#
# Deterministic extraction only: this script REPORTS candidate labels. The agent
# resolves semantic ambiguity (unrecognized scopes, -private packages, SemVer)
# and applies reviewed changes via `gh issue edit` / `gh pr edit`.

# Recognized type/scope vocabulary fallbacks, overridden by .github/labels.yml.
DEFAULT_TYPES="feat fix spec chore docs ci build test refactor perf revert"
DEFAULT_SCOPES="core schema configuration cli action canons agents spec deps deps-dev readme"

resolve_repo() {
  REPO="$(git config --get remote.upstream.url 2>/dev/null | sed -E 's#(git@github\.com:|https://github\.com/)([^/]+/[^/.]+)(\.git)?$#\2#' || true)"
  if [ -z "$REPO" ]; then
    REPO="$(git config --get remote.origin.url 2>/dev/null | sed -E 's#(git@github\.com:|https://github\.com/)([^/]+/[^/.]+)(\.git)?$#\2#' || true)"
  fi
  if [ -z "$REPO" ]; then
    REPO="PAIR-code/canon-clerk"
  fi
}

load_vocabulary() {
  RECOG_TYPES="$DEFAULT_TYPES"
  RECOG_SCOPES="$DEFAULT_SCOPES"
  if [ -f .github/labels.yml ]; then
    file_types="$(grep -oE 'type: [a-z]+' .github/labels.yml 2>/dev/null | sed 's/type: //' | sort -u | tr '\n' ' ' || true)"
    file_scopes="$(grep -oE 'scope: [a-z-]+' .github/labels.yml 2>/dev/null | sed 's/scope: //' | sort -u | tr '\n' ' ' || true)"
    [ -n "$file_types" ] && RECOG_TYPES="$file_types"
    [ -n "$file_scopes" ] && RECOG_SCOPES="$file_scopes"
  fi
}

run_audit() {
  if ! command -v gh >/dev/null 2>&1; then
    echo "Error: GitHub CLI ('gh') is required but not installed." >&2
    exit 1
  fi

  load_vocabulary

  ISSUE_TSV="$(mktemp /tmp/triage-audit-issues.XXXXXX)"
  PR_TSV="$(mktemp /tmp/triage-audit-prs.XXXXXX)"

  fetch_tsv() {
    gh issue list --repo "$REPO" --state open --limit 200 \
      --json number,title,labels,state \
      --jq '.[] | [(.number|tostring), .state, .title, ((.labels // []) | map(.name) | join(","))] | @tsv' > "$ISSUE_TSV" 2>/dev/null || true
    if [ "$PRS" = "1" ]; then
      gh pr list --repo "$REPO" --state open --limit 200 \
        --json number,title,labels,state \
        --jq '.[] | [(.number|tostring), .state, .title, ((.labels // []) | map(.name) | join(","))] | @tsv' > "$PR_TSV" 2>/dev/null || true
    fi
  }

  if [ "$JSON" = "1" ]; then
    gh issue list --repo "$REPO" --state open --limit 200 \
      --json number,title,labels,state || true
    if [ "$PRS" = "1" ]; then
      gh pr list --repo "$REPO" --state open --limit 200 \
        --json number,title,labels,state || true
    fi
    rm -f "$ISSUE_TSV" "$PR_TSV"
    return 0
  fi

  fetch_tsv

  # TSV: NUMBER<TAB>STATE<TAB>TITLE<TAB>LABELS. LABELS is a comma-joined string;
  # tab is the only column delimiter, so labels containing spaces stay intact.
  # POSIX awk: all -v assignments precede the program text, then the operands.
  awk -v RECOG_TYPES="$RECOG_TYPES" -v RECOG_SCOPES="$RECOG_SCOPES" -v UNLABELED="$UNLABELED" -v SINCE="$SINCE" '
    BEGIN {
      FS = "\t"
      split(RECOG_TYPES, rt, " ");  for (i in rt)  typeOK[rt[i]]  = 1
      split(RECOG_SCOPES, rs, " "); for (i in rs)  scopeOK[rs[i]] = 1
      printf "NUMBER\tTYPE\tSCOPE\tSTATUS\tMISSING_LABELS\tUNRECOGNIZED_SCOPES\n"
    }
    {
      if (NF < 3) next
      number = $1; state = $2; title = $3; labels = (NF >= 4 ? $4 : "")

      split("", labelset)
      if (labels != "") {
        nl = split(labels, la, ",")
        for (i = 1; i <= nl; i++) {
          gsub(/^[ \t]+|[ \t]+$/, "", la[i])
          if (la[i] != "") labelset[la[i]] = 1
        }
      }

      # Extract Conventional Commit type and optional scope from the title.
      type = ""; scope = ""; rest = title
      if (match(title, /^[A-Za-z]+/)) {
        type = tolower(substr(title, RSTART, RLENGTH))
        rest = substr(title, RSTART + RLENGTH)
      }
      if (rest ~ /^\(/) {
        p = index(rest, ")")
        if (p > 1) scope = substr(rest, 2, p - 2)
      }

      missing = ""; unrec = ""
      if (type != "" && (type in typeOK)) {
        if (!(("type: " type) in labelset))
          missing = missing (missing == "" ? "" : " ") "type: " type
      }
      if (scope != "") {
        if (scope in scopeOK) {
          if (!(("scope: " scope) in labelset))
            missing = missing (missing == "" ? "" : " ") "scope: " scope
        } else {
          unrec = unrec (unrec == "" ? "" : " ") scope
        }
      }

      if (UNLABELED == "1" && missing == "" && unrec == "") next
      if (SINCE != "" && (number + 0) < (SINCE + 0)) next

      printf "%s\t%s\t%s\t%s\t%s\t%s\n", number, type, scope, state, missing, unrec
    }
  ' "$ISSUE_TSV" "$PR_TSV"

  rm -f "$ISSUE_TSV" "$PR_TSV"
}

# Argument parsing
UNLABELED=0
PRS=0
JSON=0
SINCE=""
REPO_OVERRIDE=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --unlabeled) UNLABELED=1; shift ;;
    --since) SINCE="$2"; shift 2 ;;
    --prs) PRS=1; shift ;;
    --json) JSON=1; shift ;;
    -R|--repo) REPO_OVERRIDE="$2"; shift 2 ;;
    -h|--help)
      echo "Usage: $0 [--unlabeled] [--since N] [--prs] [--json] [--repo REPO]"
      echo ""
      echo "Audit Conventional Commit label coverage against .github/labels.yml."
      echo "Emits an unadorned TSV table and shunts to /tmp/ if it exceeds 8KB."
      echo ""
      echo "Options:"
      echo "  --unlabeled   only report items missing a justified label or scope"
      echo "  --since N     only report items with number >= N (backfill pass)"
      echo "  --prs         include pull requests in addition to issues"
      echo "  --json        dump the raw gh JSON instead of the TSV summary"
      echo "  --repo REPO   override the resolved repository"
      exit 0
      ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
  esac
done

resolve_repo
[ -n "$REPO_OVERRIDE" ] && REPO="$REPO_OVERRIDE"

# Output shunting when exceeding 8KB per canon
TMP_OUT="$(mktemp /tmp/triage-audit-out.XXXXXX)"
set +e
( run_audit ) > "$TMP_OUT" 2>&1
EXIT_CODE=$?
set -e

SIZE="$(wc -c < "$TMP_OUT" | tr -d ' ')"
MAX_BYTES=8192

if [ "$SIZE" -gt "$MAX_BYTES" ]; then
  SHUNTED_LOG="/tmp/triage-audit-$(date +%s).log"
  mv "$TMP_OUT" "$SHUNTED_LOG"
  head -n 60 "$SHUNTED_LOG"
  echo ""
  echo "[OUTPUT TRUNCATED: Output size (${SIZE} bytes) exceeds 8KB limit]"
  echo "Complete triage audit written to: ${SHUNTED_LOG}"
else
  cat "$TMP_OUT"
  rm -f "$TMP_OUT"
fi

exit "$EXIT_CODE"
