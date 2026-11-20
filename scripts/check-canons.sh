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

# Deterministically enforces .canons/ directory hygiene, domain allowlist,
# controlled tags vocabulary, internal/ coupling, and README.md domain parity.

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
CANONS_DIR="$REPO_ROOT/.canons"
README_FILE="$REPO_ROOT/README.md"

# Canonical allowlist of exportable .canons/<domain>/ directories (1-to-1 with builtin:<domain>/<slug>)
ALLOWED_DOMAINS="
agent-orientation
agent-scripts
agent-skills
api-design
architecture
build-systems
canon-authoring
cli
code-quality
concurrency
configuration
conventional-commits
git-workflow
openspec
parser-design
readme-authoring
repo-governance
security
testing
versioning
visual-identity
"

# Curated cross-cutting / stack tags permitted in addition to domain tags and 'internal'
ALLOWED_CROSS_TAGS="
ci
monorepo
typescript
"

ALLOWED_TAGS="$ALLOWED_DOMAINS
internal
$ALLOWED_CROSS_TAGS"

FAILED=0
CHECKED=0

in_list() {
    needle="$1"
    haystack="$2"
    for item in $haystack; do
        if [ "$item" = "$needle" ]; then
            return 0
        fi
    done
    return 1
}

# Extract normalized tags from YAML frontmatter (supports inline [a, b], block list, or scalar).
extract_tags() {
    awk '
        BEGIN { in_fm = 0; in_tags = 0; fm_done = 0 }
        NR == 1 && /^---[[:space:]]*$/ { in_fm = 1; next }
        in_fm && /^---[[:space:]]*$/ { fm_done = 1; exit }
        !in_fm { exit }

        in_tags {
            if (/^[[:space:]]*-[[:space:]]*/) {
                val = $0
                sub(/^[[:space:]]*-[[:space:]]*/, "", val)
                sub(/[[:space:]]*#.*$/, "", val)
                gsub(/^[[:space:]"'"'"']+|[[:space:]"'"'"']+$/, "", val)
                if (val != "") print val
                next
            } else if (/^[[:space:]]*$/) {
                next
            } else {
                in_tags = 0
            }
        }

        /^tags:[[:space:]]*/ {
            rest = $0
            sub(/^tags:[[:space:]]*/, "", rest)
            sub(/[[:space:]]*#.*$/, "", rest)
            gsub(/^[[:space:]]+|[[:space:]]+$/, "", rest)
            if (rest == "") {
                in_tags = 1
            } else if (substr(rest, 1, 1) == "[") {
                gsub(/^\[|\][[:space:]]*$/, "", rest)
                n = split(rest, arr, ",")
                for (i = 1; i <= n; i++) {
                    val = arr[i]
                    gsub(/^[[:space:]"'"'"']+|[[:space:]"'"'"']+$/, "", val)
                    if (val != "") print val
                }
            } else {
                gsub(/^["'"'"']+|["'"'"']+$/, "", rest)
                if (rest != "") print rest
            }
            next
        }
    ' "$1"
}

if [ ! -d "$CANONS_DIR" ]; then
    echo "FAIL: .canons/ directory does not exist at $CANONS_DIR" >&2
    exit 1
fi

# 1. Check for stray files directly under .canons/
for entry in "$CANONS_DIR"/* "$CANONS_DIR"/.*; do
    base="$(basename "$entry")"
    case "$base" in
        .|..|'*'|'.*') continue ;;
    esac
    if [ ! -e "$entry" ]; then
        continue
    fi
    if [ ! -d "$entry" ]; then
        echo "FAIL: Stray file at .canons/$base (only allowlisted domain directories and internal/ are permitted)" >&2
        FAILED=1
        continue
    fi
    if [ "$base" != "internal" ] && ! in_list "$base" "$ALLOWED_DOMAINS"; then
        echo "FAIL: Unregistered directory .canons/$base/ (add to ALLOWED_DOMAINS in scripts/check-canons.sh or move canons to an existing domain)" >&2
        FAILED=1
    fi
done

# 2. Verify every allowlisted domain (and internal) exists and contains at least one .md canon
for dir in $ALLOWED_DOMAINS internal; do
    if [ ! -d "$CANONS_DIR/$dir" ]; then
        echo "FAIL: Allowlisted directory .canons/$dir/ is missing" >&2
        FAILED=1
        continue
    fi
    md_count="$(find "$CANONS_DIR/$dir" -maxdepth 1 -type f -name '*.md' | wc -l | tr -d ' ')"
    if [ "$md_count" -eq 0 ]; then
        echo "FAIL: Directory .canons/$dir/ contains no .md canon files" >&2
        FAILED=1
    fi
done

# 3. Verify no nested subdirectories and no non-.md files under .canons/<dir>/
for dir_path in "$CANONS_DIR"/*; do
    if [ ! -d "$dir_path" ]; then
        continue
    fi
    dir="$(basename "$dir_path")"
    for entry in "$dir_path"/* "$dir_path"/.*; do
        base="$(basename "$entry")"
        case "$base" in
            .|..|'*'|'.*') continue ;;
        esac
        if [ ! -e "$entry" ]; then
            continue
        fi
        if [ -d "$entry" ]; then
            echo "FAIL: Nested subdirectory .canons/$dir/$base/ is forbidden (corpus must be flat one level beneath .canons/)" >&2
            FAILED=1
            continue
        fi
        case "$base" in
            *.md) ;;
            *)
                echo "FAIL: Non-Markdown file .canons/$dir/$base is forbidden under .canons/" >&2
                FAILED=1
                continue
                ;;
        esac

        # 4. Validate frontmatter tags for this canon
        CHECKED=$((CHECKED + 1))
        rel=".canons/$dir/$base"
        tags="$(extract_tags "$entry")"

        if [ -z "$tags" ]; then
            echo "FAIL: $rel declares no tags in frontmatter" >&2
            FAILED=1
            continue
        fi

        has_domain=0
        has_internal=0
        for tag in $tags; do
            if ! in_list "$tag" "$ALLOWED_TAGS"; then
                echo "FAIL: $rel uses unregistered tag '$tag'" >&2
                FAILED=1
            fi
            if [ "$tag" = "$dir" ]; then
                has_domain=1
            fi
            if [ "$tag" = "internal" ]; then
                has_internal=1
            fi
        done

        if [ "$dir" = "internal" ]; then
            if [ "$has_internal" -eq 0 ]; then
                echo "FAIL: $rel resides in .canons/internal/ but is missing the 'internal' tag" >&2
                FAILED=1
            fi
        else
            if [ "$has_internal" -ne 0 ]; then
                echo "FAIL: $rel carries 'internal' tag outside .canons/internal/" >&2
                FAILED=1
            fi
            if [ "$has_domain" -eq 0 ]; then
                echo "FAIL: $rel resides in .canons/$dir/ but is missing the '$dir' tag" >&2
                FAILED=1
            fi
        fi
    done
done

# 5. Verify README.md corpus table domain-set parity against .canons/
if [ ! -f "$README_FILE" ]; then
    echo "FAIL: README.md not found at $README_FILE" >&2
    FAILED=1
else
    readme_dirs="$(awk '
        /^## The Canon Corpus/ { in_section = 1; next }
        in_section && /^## / { exit }
        in_section && /^\|/ {
            line = $0
            while (match(line, /\(\.canons\/[a-z0-9-]+\/\)/)) {
                tok = substr(line, RSTART + 9, RLENGTH - 11)
                print tok
                line = substr(line, RSTART + RLENGTH)
            }
        }
    ' "$README_FILE" | sort -u)"

    for dir in $ALLOWED_DOMAINS internal; do
        if ! in_list "$dir" "$readme_dirs"; then
            echo "FAIL: README.md corpus table is missing domain '.canons/$dir/'" >&2
            FAILED=1
        fi
    done

    for rdir in $readme_dirs; do
        if [ "$rdir" != "internal" ] && ! in_list "$rdir" "$ALLOWED_DOMAINS"; then
            echo "FAIL: README.md corpus table lists unregistered domain '.canons/$rdir/'" >&2
            FAILED=1
        fi
    done
fi

if [ "$FAILED" -ne 0 ]; then
    echo "" >&2
    echo "Canon corpus taxonomy check failed." >&2
    exit 1
fi

echo "Canon corpus taxonomy check passed: $CHECKED canon(s) across $(printf '%s\n' $ALLOWED_DOMAINS internal | wc -l | tr -d ' ') directories."
exit 0
