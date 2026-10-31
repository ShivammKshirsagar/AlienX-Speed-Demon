#!/usr/bin/env bash
set -euo pipefail
solution=${1:-track-b/target/release/solution}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
"$solution" tools/track-b/phrase-boundary "$tmp" > /dev/null
cmp tools/track-b/phrase-boundary/expected.txt "$tmp/results.txt"
