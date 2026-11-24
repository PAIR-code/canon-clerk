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

# Unified local check script providing parity with GitHub Actions CI.

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$REPO_ROOT"

# Ensure standard Rust toolchain paths are included if available
if [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

# Auto-enable optional local build accelerators when installed and not explicitly configured.
# Pass RUSTC_WRAPPER="" or RUSTFLAGS="" to opt out.
if [ -z "${RUSTC_WRAPPER+set}" ] && command -v sccache >/dev/null 2>&1; then
    export RUSTC_WRAPPER=sccache
fi
if [ -z "${RUSTFLAGS+set}" ] && command -v mold >/dev/null 2>&1; then
    export RUSTFLAGS="-C link-arg=-fuse-ld=mold"
fi

echo "=== 1/7: Checking License Headers ==="
./scripts/check-license.sh

echo "=== 2/7: Checking File Size Bounds ==="
./scripts/check-file-size.sh

echo "=== 3/7: Checking Canon Corpus Taxonomy ==="
./scripts/check-canons.sh

echo "=== 4/7: Checking Code Formatting ==="
cargo fmt --check

echo "=== 5/7: Running Clippy Lints ==="
cargo clippy --all-targets --all-features -- -D warnings

echo "=== 6/7: Running Test Suite ==="
cargo test

echo "=== 7/7: Validating Repository Canons ==="
cargo run --locked -- validate --all-canons --max-warnings 0

echo ""
echo "=== All Checks Passed Successfully ==="

