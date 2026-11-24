#!/bin/sh
set -e
set -x

# Copyright 2026 Google LLC
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Composite shift-left pre-flight runner for Canon Clerk pull requests.

export NO_COLOR=1
export CARGO_TERM_COLOR=never
export CARGO_TERM_PROGRESS_WHEN=never
export TERM=dumb

if [ -d "$HOME/.cargo/bin" ]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

# Match check.sh build accelerator flags so cargo run reuses the same fingerprint
if [ -z "${RUSTC_WRAPPER+set}" ] && command -v sccache >/dev/null 2>&1; then
  export RUSTC_WRAPPER=sccache
fi
if [ -z "${RUSTFLAGS+set}" ] && command -v mold >/dev/null 2>&1; then
  export RUSTFLAGS="-C link-arg=-fuse-ld=mold"
fi

ACTIVE_TMP=""
DIFF_FILE=""
cleanup_tmp() {
  rm -f "$ACTIVE_TMP" "$DIFF_FILE"
}
trap cleanup_tmp EXIT
trap 'cleanup_tmp; exit 130' INT
trap 'cleanup_tmp; exit 143' TERM

flush_shunted() {
  LOG_PATH="$1"
  STEP_NAME="$2"
  SIZE="$(wc -c < "$LOG_PATH" | tr -d ' ')"
  if [ "$SIZE" -gt 8192 ]; then
    SHUNTED_LOG="/tmp/pr-preflight-${STEP_NAME}-$(date +%s).log"
    mv "$LOG_PATH" "$SHUNTED_LOG"
    ACTIVE_TMP=""
    tail -n 60 "$SHUNTED_LOG"
    echo ""
    echo "[OUTPUT TRUNCATED: ${STEP_NAME} output (${SIZE} bytes) exceeds 8KB limit]"
    echo "Complete ${STEP_NAME} log written to: ${SHUNTED_LOG}"
  else
    cat "$LOG_PATH"
    rm -f "$LOG_PATH"
    ACTIVE_TMP=""
  fi
}

print_help() {
  cat <<EOF
Usage: $0 [--pr-title <title>] [--pr-body <text> | --pr-body-file <path | ->] [--audit | --no-audit] [-- <extra audit args>]

Runs Canon Clerk shift-left pre-flight checks in fail-fast order: PR title
validation, offline CI parity checks, conditional metadata/workflow checks,
and live canon-clerk audit against the upstream/main merge-base.

Options:
  --pr-title <title>          Validate PR title against pr-title-lint.yml and forward to audit
  --pr-body <text>            Forward PR body text to canon-clerk audit
  --pr-body-file <path | ->   Forward PR body file path (or '-' for stdin) to canon-clerk audit
  --audit                     Run live canon-clerk audit after deterministic checks (default)
  --no-audit                  Disable live canon-clerk audit (run deterministic checks only)
  -h, --help                  Show this help message and exit

Examples:
  ./.agents/skills/pr-preflight/scripts/preflight.sh --pr-title "feat(cli): add output flag (#123)"
  ./.agents/skills/pr-preflight/scripts/preflight.sh --pr-title "feat(cli): add output flag (#123)" --pr-body-file /tmp/pr-body.md
  ./.agents/skills/pr-preflight/scripts/preflight.sh --pr-title "chore(agents): update preflight (#428)" --no-audit
  ./.agents/skills/pr-preflight/scripts/preflight.sh --pr-title "feat(cli): add output flag (#123)" -- --provider google
EOF
}

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || true)"
if [ -z "$REPO_ROOT" ] || [ ! -f "$REPO_ROOT/Cargo.toml" ]; then
  echo "Error: Must be executed from inside a Canon Clerk worktree checkout. Run 'cd <worktree>' (for example, 'cd main' or 'cd <issue>-<slug>') and retry." >&2
  exit 1
fi
cd "$REPO_ROOT"

PR_TITLE=""
PR_BODY=""
PR_BODY_FILE=""
RUN_AUDIT=true

while [ "$#" -gt 0 ]; do
  case "$1" in
    --pr-title)
      if [ "$#" -lt 2 ]; then
        echo "Error: --pr-title requires a value. Pass '--pr-title \"type(scope): subject\"' or run '$0 --help'." >&2
        exit 2
      fi
      PR_TITLE="$2"
      shift 2
      ;;
    --pr-body)
      if [ "$#" -lt 2 ]; then
        echo "Error: --pr-body requires a value. Pass '--pr-body \"<markdown>\"' or '--pr-body-file <path>', or run '$0 --help'." >&2
        exit 2
      fi
      PR_BODY="$2"
      shift 2
      ;;
    --pr-body-file)
      if [ "$#" -lt 2 ]; then
        echo "Error: --pr-body-file requires a value. Pass '--pr-body-file <path>' or '--pr-body-file -', or run '$0 --help'." >&2
        exit 2
      fi
      PR_BODY_FILE="$2"
      shift 2
      ;;
    --audit)
      RUN_AUDIT=true
      shift
      ;;
    --no-audit)
      RUN_AUDIT=false
      shift
      ;;
    --)
      shift
      break
      ;;
    -h|--help)
      print_help
      exit 0
      ;;
    *)
      echo "Error: Unknown argument '$1'. Run '$0 --help' for valid options." >&2
      print_help >&2
      exit 2
      ;;
  esac
done

if [ -n "$PR_BODY" ] && [ -n "$PR_BODY_FILE" ]; then
  echo "Error: --pr-body and --pr-body-file are mutually exclusive. Pass either '--pr-body \"<text>\"' or '--pr-body-file <path>', not both." >&2
  exit 2
fi

if [ "$PR_BODY_FILE" = "-" ]; then
  PR_BODY="$(cat)"
  PR_BODY_FILE=""
fi

# 1. PR title validation (fail fast before building/testing)
set -x
if [ -n "$PR_TITLE" ]; then
  ./scripts/check-pr-title.sh "$PR_TITLE"
fi

# 2. Deterministic offline CI parity checks (shunt >8KB output)
ACTIVE_TMP="$(mktemp /tmp/pr-preflight-check.XXXXXX)"
set +e
./scripts/check.sh > "$ACTIVE_TMP" 2>&1
RC=$?
set -e
flush_shunted "$ACTIVE_TMP" check
if [ "$RC" -ne 0 ]; then
  exit "$RC"
fi

# 3. Resolve merge-base for conditional checks and live canon audit
if [ -n "${CANON_CLERK_BASE_REF:-}" ]; then
  BASE_REF="$CANON_CLERK_BASE_REF"
elif git rev-parse --verify --quiet upstream/main >/dev/null 2>&1; then
  BASE_REF="upstream/main"
elif git rev-parse --verify --quiet origin/main >/dev/null 2>&1; then
  BASE_REF="origin/main"
elif git rev-parse --verify --quiet main >/dev/null 2>&1; then
  BASE_REF="main"
else
  echo "Error: Could not resolve base ref (tried upstream/main, origin/main, main). Set CANON_CLERK_BASE_REF explicitly." >&2
  exit 1
fi

BASE_COMMIT="$(git merge-base "$BASE_REF" HEAD)"
CHANGED_FILES="$(git diff --name-only "$BASE_COMMIT")"

# 4. Conditional checks based on modified paths
if printf '%s\n' "$CHANGED_FILES" | grep -Eq '^\.github/(settings\.yml|scripts/repo_settings\.py|workflows/repo-metadata-check\.yml)$'; then
  python3 .github/scripts/repo_settings.py check
fi

WORKFLOW_FILES="$(printf '%s\n' "$CHANGED_FILES" | grep -E '^\.github/workflows/.*\.ya?ml$' || true)"
if [ -n "$WORKFLOW_FILES" ] && command -v zizmor >/dev/null 2>&1; then
  ACTIVE_TMP="$(mktemp /tmp/pr-preflight-zizmor.XXXXXX)"
  set +e
  # shellcheck disable=SC2086
  zizmor --no-progress $WORKFLOW_FILES > "$ACTIVE_TMP" 2>&1
  RC=$?
  set -e
  flush_shunted "$ACTIVE_TMP" zizmor
  if [ "$RC" -ne 0 ]; then
    exit "$RC"
  fi
fi

# 5. Live semantic canon audit against merge-base diff (shunt >8KB output)
if [ "$RUN_AUDIT" = false ]; then
  echo "Live canon-clerk audit disabled (--no-audit)."
  exit 0
fi

DIFF_FILE="$(mktemp /tmp/pr-preflight-diff.XXXXXX)"
git diff "$BASE_COMMIT" > "$DIFF_FILE"

if [ ! -s "$DIFF_FILE" ]; then
  echo "No tracked changes found relative to $BASE_REF (merge-base $BASE_COMMIT). Skipping audit."
  exit 0
fi

# Preserve any extra args passed after `--` while prepending PR metadata flags
set -- --diff - "$@"
if [ -n "$PR_TITLE" ]; then
  set -- --pr-title "$PR_TITLE" "$@"
fi
if [ -n "$PR_BODY" ]; then
  set -- --pr-body "$PR_BODY" "$@"
elif [ -n "$PR_BODY_FILE" ]; then
  set -- --pr-body-file "$PR_BODY_FILE" "$@"
fi

ACTIVE_TMP="$(mktemp /tmp/pr-preflight-audit.XXXXXX)"
set +e
if [ -n "${CANON_CLERK_BIN:-}" ]; then
  "$CANON_CLERK_BIN" audit "$@" < "$DIFF_FILE" > "$ACTIVE_TMP" 2>&1
  RC=$?
else
  cargo run --locked -- audit "$@" < "$DIFF_FILE" > "$ACTIVE_TMP" 2>&1
  RC=$?
fi
set -e
flush_shunted "$ACTIVE_TMP" audit
exit "$RC"
