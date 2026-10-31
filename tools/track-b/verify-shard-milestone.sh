#!/usr/bin/env bash
set -euo pipefail
solution=/tmp/track-b-shard
for fixture in tools/track-b/example tools/track-b/phrase-boundary; do
  name=${fixture##*/}
  out="/data/shard-$name"
  "$solution" "$fixture" "$out" >/dev/null
  cmp "$fixture/expected.txt" "$out/results.txt"
done
"$solution" /data/track-b-sample-2 /data/shard-sample-2 >/dev/null
cmp /data/track-b-reference-sample-2/results.txt /data/shard-sample-2/results.txt
sha256sum /data/shard-sample-2/results.txt
