#!/bin/sh
set -e
set -x

# Helper script to audit active worktrees, check status, and identify merged PRs.
# Adheres to .agents/skills/.canons/ standards:
# - POSIX Bourne Shell
# - Command echoing via set -x
# - Unadorned plain text output
# - Output shunting when output exceeds 8KB

run_audit() {
  # 1. Active worktrees and their tracking branches
  git worktree list -v

  # 2. Check for dangling/stale worktrees
  git worktree prune --dry-run

  # 3. Status of each active worktree
  for wt in $(git worktree list --porcelain | awk '/^worktree / && !/bare/ { print $2 }'); do
    echo "--- status: $wt ---"
    git -C "$wt" status --short
  done

  # 4. Merged pull requests (reapable worktrees)
  REPO="$(git config --get remote.upstream.url 2>/dev/null | sed -E 's#(git@github\.com:|https://github\.com/)([^/]+/[^/.]+)(\.git)?$#\2#' || true)"
  if [ -z "$REPO" ]; then
    REPO="$(git config --get remote.origin.url 2>/dev/null | sed -E 's#(git@github\.com:|https://github\.com/)([^/]+/[^/.]+)(\.git)?$#\2#' || true)"
  fi

  if [ -n "$REPO" ] && command -v gh >/dev/null 2>&1; then
    gh pr list --repo "$REPO" --state merged --limit 20
  fi
}

# If output exceeds 8KB, shunt to temporary file per canon
TMP_OUT="$(mktemp /tmp/worktree-doctor-out.XXXXXX)"
run_audit > "$TMP_OUT" 2>&1

SIZE="$(wc -c < "$TMP_OUT" | tr -d ' ')"
MAX_BYTES=8192

if [ "$SIZE" -gt "$MAX_BYTES" ]; then
  SHUNTED_LOG="/tmp/worktree-doctor-$(date +%s).log"
  mv "$TMP_OUT" "$SHUNTED_LOG"
  head -n 60 "$SHUNTED_LOG"
  echo ""
  echo "[OUTPUT TRUNCATED: Output size (${SIZE} bytes) exceeds 8KB limit]"
  echo "Complete audit report written to: ${SHUNTED_LOG}"
else
  cat "$TMP_OUT"
  rm -f "$TMP_OUT"
fi
