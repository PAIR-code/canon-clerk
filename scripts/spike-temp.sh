#!/bin/sh
set -eu
TMP_FILE="$(mktemp)"
trap 'rm -f "$TMP_FILE"' EXIT
echo "hello" > "$TMP_FILE"
cat "$TMP_FILE"
