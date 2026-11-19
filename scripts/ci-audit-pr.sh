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

# CI pull request audit entrypoint invoked by `.github/workflows/canon-clerk-audit.yml`.
# Runs against a trusted base-branch checkout, ingesting PR diff and metadata
# strictly as data via the GitHub API and `$GITHUB_EVENT_PATH`.

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$REPO_ROOT"

if [ -z "${GEMINI_API_KEY:-}" ] && [ -z "${TYPESAFE_API_KEY:-}" ]; then
    echo "Skipping live canon-clerk audit: no provider API credentials configured in environment." >&2
    if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
        printf '### Canon Clerk Audit Gate: SKIPPED\n\nLive audit skipped because provider API credentials (`GEMINI_API_KEY` / `TYPESAFE_API_KEY`) are unavailable in this execution context.\n' >> "$GITHUB_STEP_SUMMARY"
    fi
    exit 0
fi

if [ -z "${GITHUB_EVENT_PATH:-}" ] || [ ! -r "$GITHUB_EVENT_PATH" ]; then
    echo "Error: GITHUB_EVENT_PATH is unset or unreadable." >&2
    exit 1
fi

PR_NUMBER="$(jq -r '.pull_request.number // empty' "$GITHUB_EVENT_PATH")"
if [ -z "$PR_NUMBER" ]; then
    echo "Error: Could not extract pull_request.number from $GITHUB_EVENT_PATH." >&2
    exit 1
fi

PR_TITLE="$(jq -r '.pull_request.title // empty' "$GITHUB_EVENT_PATH")"

TMP_DIR="$(mktemp -d)"
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT
trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM

DIFF_FILE="$TMP_DIR/pr.diff"
BODY_FILE="$TMP_DIR/pr-body.md"

jq -r '.pull_request.body // empty' "$GITHUB_EVENT_PATH" > "$BODY_FILE"

if [ -n "${GITHUB_REPOSITORY:-}" ]; then
    gh pr diff "$PR_NUMBER" --repo "$GITHUB_REPOSITORY" > "$DIFF_FILE"
else
    gh pr diff "$PR_NUMBER" > "$DIFF_FILE"
fi

CANON_CLERK_BIN="${CANON_CLERK_BIN:-./target/debug/canon-clerk}"
CASELOAD_OUT="${CANON_CLERK_CASELOAD_OUT:-caseload.json}"

set -- audit --diff - --output-caseload "$CASELOAD_OUT" "$@"

if [ -n "$PR_TITLE" ]; then
    set -- "$@" --pr-title "$PR_TITLE"
fi

if [ -s "$BODY_FILE" ]; then
    set -- "$@" --pr-body-file "$BODY_FILE"
fi

"$CANON_CLERK_BIN" "$@" < "$DIFF_FILE"
