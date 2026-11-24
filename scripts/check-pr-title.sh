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

# Validates a pull request title against the Conventional Commits types, scopes,
# and header pattern configured in .github/workflows/pr-title-lint.yml.
#
# Architectural constraint: Canon Clerk's local development setup requires only
# the Rust toolchain and POSIX shell utilities (no Node/commitlint, yq, or
# PyYAML prerequisites), and `amannn/action-semantic-pull-request` has no
# standalone CLI. This script therefore uses zero-dependency POSIX `awk` to
# extract the allowlists directly from the workflow file without introducing
# external runtime dependencies or duplicating the allowlist.

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
WORKFLOW_FILE="$REPO_ROOT/.github/workflows/pr-title-lint.yml"

print_help() {
    cat <<EOF
Usage: $0 <pr-title | ->

Validates a candidate pull request title against the permitted types, scopes,
and Conventional Commits header pattern in .github/workflows/pr-title-lint.yml.

Arguments:
  <pr-title>   Pull request title string to validate
  -            Read pull request title from standard input

Examples:
  $0 "feat(cli): add output format flag (#123)"
  $0 "chore(agents,canons): align preflight checks (#428)"
EOF
}

if [ ! -f "$WORKFLOW_FILE" ]; then
    echo "Error: Workflow file not found at $WORKFLOW_FILE" >&2
    exit 1
fi

if [ "$#" -eq 1 ] && { [ "$1" = "-h" ] || [ "$1" = "--help" ]; }; then
    print_help
    exit 0
fi

if [ "$#" -ne 1 ]; then
    print_help >&2
    exit 2
fi

if [ "$1" = "-" ]; then
    TITLE="$(cat)"
else
    TITLE="$1"
fi

if [ -z "$TITLE" ]; then
    echo "Error: Pull request title is empty." >&2
    exit 1
fi

# Zero-dependency POSIX `awk` parser: docs/development-setup.md requires only
# Rust and POSIX shell (no yq, PyYAML, or Node/commitlint), and the upstream
# GitHub Action (`amannn/action-semantic-pull-request`) provides no standalone CLI.
awk -v title="$TITLE" '
function trim(s) {
    sub(/^[[:space:]]+/, "", s)
    sub(/[[:space:]]+$/, "", s)
    return s
}

# Parse YAML block scalar lists under `types: |` and `scopes: |`
{
    raw = $0
    sub(/\r$/, "", raw)

    match(raw, /^[ ]*/)
    indent = RLENGTH
    line = substr(raw, indent + 1)

    if (in_block != "") {
        if (line == "" || substr(line, 1, 1) == "#") {
            next
        }
        if (indent <= block_indent) {
            in_block = ""
        } else {
            val = trim(line)
            if (in_block == "types") {
                allowed_types[val] = 1
                type_list = (type_list == "" ? val : type_list ", " val)
            } else if (in_block == "scopes") {
                allowed_scopes[val] = 1
                scope_list = (scope_list == "" ? val : scope_list ", " val)
            }
            next
        }
    }

    if (line ~ /^types:[[:space:]]*\|/) {
        in_block = "types"
        block_indent = indent
    } else if (line ~ /^scopes:[[:space:]]*\|/) {
        in_block = "scopes"
        block_indent = indent
    } else if (line ~ /^requireScope:[[:space:]]*true/) {
        require_scope = 1
    }
}

END {
    if (type_list == "" || scope_list == "") {
        print "Error: Failed to parse types or scopes from .github/workflows/pr-title-lint.yml" > "/dev/stderr"
        exit 1
    }

    if (index(title, "\n") > 0 || index(title, "\r") > 0) {
        print "Error: Pull request title must be a single line." > "/dev/stderr"
        exit 1
    }

    # Match headerPattern: ^(\w+)(?:\(([\w\$\.\-\*\s,]+)\))?\!?:\s([^\n\r]+)$
    if (!match(title, /^[A-Za-z0-9_]+/)) {
        printf "Error: No release type found in pull request title \"%s\". Use \"type(scope): subject\" where type is one of: %s.\n", title, type_list > "/dev/stderr"
        exit 1
    }
    pr_type = substr(title, 1, RLENGTH)
    rest = substr(title, RLENGTH + 1)

    has_scope = 0
    scope_str = ""
    if (substr(rest, 1, 1) == "(") {
        close_idx = index(rest, ")")
        if (close_idx == 0) {
            printf "Error: Unclosed scope parenthesis in pull request title \"%s\". Use \"type(scope): subject\" where scope is one of: %s.\n", title, scope_list > "/dev/stderr"
            exit 1
        }
        has_scope = 1
        scope_str = substr(rest, 2, close_idx - 2)
        rest = substr(rest, close_idx + 1)
        if (scope_str == "" || scope_str !~ /^[A-Za-z0-9_$.*\t ,-]+$/) {
            printf "Error: Invalid scope syntax \"%s\" in pull request title \"%s\". Use \"type(scope): subject\" where scope is one of: %s.\n", scope_str, title, scope_list > "/dev/stderr"
            exit 1
        }
    }

    if (substr(rest, 1, 1) == "!") {
        rest = substr(rest, 2)
    }

    if (rest !~ /^:[[:space:]]+[^[:space:]]/) {
        printf "Error: Pull request title \"%s\" must match \"type(scope): subject\" with a non-empty subject.\n", title > "/dev/stderr"
        exit 1
    }

    if (!(pr_type in allowed_types)) {
        printf "Error: Unknown release type \"%s\" found in pull request title \"%s\". Type must match one of: %s.\n", pr_type, title, type_list > "/dev/stderr"
        exit 1
    }

    if (require_scope && !has_scope) {
        printf "Error: No scope found in pull request title \"%s\". Scope must match one of: %s.\n", title, scope_list > "/dev/stderr"
        exit 1
    }

    if (has_scope) {
        n = split(scope_str, parts, ",")
        for (i = 1; i <= n; i++) {
            sc = trim(parts[i])
            if (sc == "") {
                printf "Error: Empty scope entry in pull request title \"%s\". Scope must match one of: %s.\n", title, scope_list > "/dev/stderr"
                exit 1
            }
            if (!(sc in allowed_scopes)) {
                printf "Error: Unknown scope \"%s\" found in pull request title \"%s\". Scope must match one of: %s.\n", sc, title, scope_list > "/dev/stderr"
                exit 1
            }
        }
    }

    printf "PR title check passed: \"%s\"\n", title
}
' "$WORKFLOW_FILE"
