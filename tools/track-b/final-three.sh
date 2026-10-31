#!/usr/bin/env bash
set -euo pipefail
solution='/mnt/c/GitHub Desktop/AlienX-Speed-Demon/track-b/target/release/solution'
mkdir -p /data/track-b-profiles
cat /data/docs.txt >/dev/null
for run in 1 2 3; do
  /usr/bin/time -v taskset -c 0-11 "$solution" /data "/data/track-b-profiles/final-${run}-dir" \
    > /dev/null \
    2> "/data/track-b-profiles/final-${run}.log"
  sha256sum "/data/track-b-profiles/final-${run}-dir/results.txt"
done
