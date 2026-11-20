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

# Deterministically enforces builtins/ and .canons/ directory hygiene, domain allowlist,
# controlled tags vocabulary, internal tag coupling, and README.md domain parity.

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
BUILTINS_DIR="$REPO_ROOT/builtins"
CANONS_DIR="$REPO_ROOT/.canons"
README_FILE="$REPO_ROOT/README.md"

# Canonical allowlist of exportable builtins/<domain>/ directories (1-to-1 with builtin:<domain>/<slug>)
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

if [ ! -d "$BUILTINS_DIR" ]; then
    echo "FAIL: builtins/ directory does not exist at $BUILTINS_DIR" >&2
    exit 1
fi

if [ ! -d "$CANONS_DIR" ]; then
    echo "FAIL: .canons/ directory does not exist at $CANONS_DIR" >&2
    exit 1
fi

# 1. Check builtins/ contains only allowlisted domain directories (no stray files)
for entry in "$BUILTINS_DIR"/* "$BUILTINS_DIR"/.*; do
    base="$(basename "$entry")"
    case "$base" in
        .|..|'*'|'.*') continue ;;
    esac
    if [ ! -e "$entry" ]; then
        continue
    fi
    if [ ! -d "$entry" ]; then
        echo "FAIL: Stray file at builtins/$base (only allowlisted domain directories are permitted)" >&2
        FAILED=1
        continue
    fi
    if ! in_list "$base" "$ALLOWED_DOMAINS"; then
        echo "FAIL: Unregistered directory builtins/$base/ (add to ALLOWED_DOMAINS in scripts/check-canons.sh or move canons to an existing domain)" >&2
        FAILED=1
    fi
done

# 2. Verify every allowlisted domain exists under builtins/ and contains at least one .md canon
for dir in $ALLOWED_DOMAINS; do
    if [ ! -d "$BUILTINS_DIR/$dir" ]; then
        echo "FAIL: Allowlisted directory builtins/$dir/ is missing" >&2
        FAILED=1
        continue
    fi
    md_count="$(find "$BUILTINS_DIR/$dir" -maxdepth 1 -type f -name '*.md' | wc -l | tr -d ' ')"
    if [ "$md_count" -eq 0 ]; then
        echo "FAIL: Directory builtins/$dir/ contains no .md canon files" >&2
        FAILED=1
    fi
done

# 3. Verify no nested subdirectories and no non-.md files under builtins/<domain>/, and validate tags
for dir_path in "$BUILTINS_DIR"/*; do
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
            echo "FAIL: Nested subdirectory builtins/$dir/$base/ is forbidden (built-in corpus must be flat one level beneath builtins/)" >&2
            FAILED=1
            continue
        fi
        case "$base" in
            *.md) ;;
            *)
                echo "FAIL: Non-Markdown file builtins/$dir/$base is forbidden under builtins/" >&2
                FAILED=1
                continue
                ;;
        esac

        CHECKED=$((CHECKED + 1))
        rel="builtins/$dir/$base"
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

        if [ "$has_internal" -ne 0 ]; then
            echo "FAIL: $rel carries 'internal' tag inside builtins/ (move repo-local canons to .canons/)" >&2
            FAILED=1
        fi
        if [ "$has_domain" -eq 0 ]; then
            echo "FAIL: $rel resides in builtins/$dir/ but is missing the '$dir' tag" >&2
            FAILED=1
        fi
    done
done

# 4. Verify .canons/ is flat (repo-local dogfood canons only) and every canon carries 'internal'
local_md_count=0
for entry in "$CANONS_DIR"/* "$CANONS_DIR"/.*; do
    base="$(basename "$entry")"
    case "$base" in
        .|..|'*'|'.*') continue ;;
    esac
    if [ ! -e "$entry" ]; then
        continue
    fi
    if [ -d "$entry" ]; then
        echo "FAIL: Subdirectory .canons/$base/ is forbidden (repo-local canons must reside flat under .canons/, exportable domains under builtins/)" >&2
        FAILED=1
        continue
    fi
    case "$base" in
        *.md) ;;
        *)
            echo "FAIL: Non-Markdown file .canons/$base is forbidden under .canons/" >&2
            FAILED=1
            continue
            ;;
    esac

    local_md_count=$((local_md_count + 1))
    CHECKED=$((CHECKED + 1))
    rel=".canons/$base"
    tags="$(extract_tags "$entry")"

    if [ -z "$tags" ]; then
        echo "FAIL: $rel declares no tags in frontmatter" >&2
        FAILED=1
        continue
    fi

    has_internal=0
    for tag in $tags; do
        if ! in_list "$tag" "$ALLOWED_TAGS"; then
            echo "FAIL: $rel uses unregistered tag '$tag'" >&2
            FAILED=1
        fi
        if [ "$tag" = "internal" ]; then
            has_internal=1
        fi
    done

    if [ "$has_internal" -eq 0 ]; then
        echo "FAIL: $rel resides in .canons/ but is missing the 'internal' tag" >&2
        FAILED=1
    fi
done

if [ "$local_md_count" -eq 0 ]; then
    echo "FAIL: Directory .canons/ contains no .md canon files" >&2
    FAILED=1
fi

# 5. Verify README.md corpus table domain-set parity against builtins/
if [ ! -f "$README_FILE" ]; then
    echo "FAIL: README.md not found at $README_FILE" >&2
    FAILED=1
else
    readme_dirs="$(awk '
        /^## The Canon Corpus/ { in_section = 1; next }
        in_section && /^## / { exit }
        in_section && /^\|/ {
            line = $0
            while (match(line, /\(builtins\/[a-z0-9-]+\/\)/)) {
                tok = substr(line, RSTART + 10, RLENGTH - 12)
                print tok
                line = substr(line, RSTART + RLENGTH)
            }
        }
    ' "$README_FILE" | sort -u)"

    for dir in $ALLOWED_DOMAINS; do
        if ! in_list "$dir" "$readme_dirs"; then
            echo "FAIL: README.md corpus table is missing domain 'builtins/$dir/'" >&2
            FAILED=1
        fi
    done

    for rdir in $readme_dirs; do
        if ! in_list "$rdir" "$ALLOWED_DOMAINS"; then
            echo "FAIL: README.md corpus table lists unregistered domain 'builtins/$rdir/'" >&2
            FAILED=1
        fi
    done
fi

if [ "$FAILED" -ne 0 ]; then
    echo "" >&2
    echo "Canon corpus taxonomy check failed." >&2
    exit 1
fi

echo "Canon corpus taxonomy check passed: $CHECKED canon(s) across $(printf '%s\n' $ALLOWED_DOMAINS | wc -l | tr -d ' ') builtin domain(s) and .canons/."
exit 0
