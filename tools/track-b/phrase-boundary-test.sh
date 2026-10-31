#!/usr/bin/env bash
set -euo pipefail
solution=${1:-track-b/target/release/solution}
cd "$(dirname "$0")/../.."
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for fixture in tools/track-b/phrase-boundary tools/track-b/example; do
  "$solution" "$fixture" "$tmp/out" > /dev/null
  cmp "$fixture/expected.txt" "$tmp/out/results.txt"
done
