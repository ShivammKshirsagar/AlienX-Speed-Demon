#!/usr/bin/env bash
set -euo pipefail
label="$1"
binary="$2"
mkdir -p /data/track-b-profiles
for run in 1 2 3 4 5; do
  /usr/bin/time -v taskset -c 0-11 "$binary" /data "/data/track-b-profiles/${label}-${run}-dir" \
    > "/data/track-b-profiles/${label}-${run}.out" \
    2> "/data/track-b-profiles/${label}-${run}.log"
  sha256sum "/data/track-b-profiles/${label}-${run}.out"
done