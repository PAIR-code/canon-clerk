#!/bin/sh
set -e

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

# Enforces a maximum ceiling of 500 production lines (lines preceding #[cfg(test)]) per Rust file.

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
MAX_PRODUCTION_LINES=500
FAILED=0
CHECKED=0

# Collect all tracked or untracked .rs files, ignoring target/ and ignored patterns
if command -v git >/dev/null 2>&1 && [ -e "$REPO_ROOT/.git" ]; then
    RS_FILES="$(git -C "$REPO_ROOT" ls-files --cached --others --exclude-standard '*.rs' 2>/dev/null || true)"
else
    RS_FILES="$(find "$REPO_ROOT/src" "$REPO_ROOT/tests" -name '*.rs' 2>/dev/null || true)"
fi

if [ -z "$RS_FILES" ]; then
    echo "No Rust files found to check."
    exit 0
fi

for file in $RS_FILES; do
    full_path="$REPO_ROOT/$file"
    if [ ! -f "$full_path" ]; then
        continue
    fi

    CHECKED=$((CHECKED + 1))

    prod_lines="$(awk '/^[[:space:]]*#\[cfg\(test\)\]/ { exit } { count++ } END { print count + 0 }' "$full_path")"

    if [ "$prod_lines" -gt "$MAX_PRODUCTION_LINES" ]; then
        echo "FAIL: $file ($prod_lines production lines exceeds $MAX_PRODUCTION_LINES-line limit)" >&2
        FAILED=1
    fi
done

if [ "$FAILED" -ne 0 ]; then
    echo "" >&2
    echo "File size check failed: one or more Rust files exceed $MAX_PRODUCTION_LINES production lines. Decompose large modules into focused submodules." >&2
    exit 1
fi

echo "File size check passed: $CHECKED Rust file(s) within the $MAX_PRODUCTION_LINES-line production ceiling."
exit 0
