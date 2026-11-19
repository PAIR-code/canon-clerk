#!/bin/sh
set -eu

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

# Shift-left local WIP audit script: diffs the current worktree against the
# upstream base branch merge-base and runs `canon-clerk audit --diff -`.
# Kept separate from `./scripts/check.sh` so routine commit-stage checks
# remain strictly offline and hermetic.

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$REPO_ROOT"

if [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

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

DIFF_FILE="$(mktemp)"
cleanup() {
    rm -f "$DIFF_FILE"
}
trap cleanup EXIT
trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM

git diff "$BASE_COMMIT" > "$DIFF_FILE"

if [ ! -s "$DIFF_FILE" ]; then
    echo "No tracked changes found relative to $BASE_REF (merge-base $BASE_COMMIT). Nothing to audit." >&2
    exit 0
fi

if [ -n "${CANON_CLERK_BIN:-}" ]; then
    "$CANON_CLERK_BIN" audit --diff - "$@" < "$DIFF_FILE"
else
    cargo run --locked -- audit --diff - "$@" < "$DIFF_FILE"
fi
