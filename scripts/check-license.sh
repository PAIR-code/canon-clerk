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

# Validates Apache-2.0 license headers across all Rust files in the repository.

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
FAILED=0
CHECKED=0

EXPECTED_LINE_1="// Copyright 2026 Google LLC"
EXPECTED_LINE_3="// Licensed under the Apache License, Version 2.0 (the \"License\");"

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

    line1="$(sed -n '1p' "$full_path")"
    line3="$(sed -n '3p' "$full_path")"

    if [ "$line1" != "$EXPECTED_LINE_1" ]; then
        echo "FAIL: $file" >&2
        echo "  Expected line 1: '$EXPECTED_LINE_1'" >&2
        echo "  Actual line 1:   '$line1'" >&2
        FAILED=1
        continue
    fi

    if [ "$line3" != "$EXPECTED_LINE_3" ]; then
        echo "FAIL: $file" >&2
        echo "  Expected line 3: '$EXPECTED_LINE_3'" >&2
        echo "  Actual line 3:   '$line3'" >&2
        FAILED=1
        continue
    fi
done

if [ "$FAILED" -ne 0 ]; then
    echo "" >&2
    echo "License header validation failed: one or more Rust files are missing required Apache-2.0 headers." >&2
    exit 1
fi

echo "License check passed: $CHECKED Rust file(s) contain valid Apache-2.0 headers."
exit 0

