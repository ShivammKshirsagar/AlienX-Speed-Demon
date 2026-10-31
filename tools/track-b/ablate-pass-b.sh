#!/usr/bin/env bash
set -euo pipefail
solution='/mnt/c/GitHub Desktop/AlienX-Speed-Demon/track-b/target/release/solution'
cat /data/docs.txt >/dev/null
for mode in a b c; do
  log="/data/ablate-$mode.log"
  SD_VARIANT=A SD_ABLATE="$mode" SD_TIMING=1 taskset -c 0-11 "$solution" /data "/data/ablate-$mode-out" >/dev/null 2>"$log"
  cat "$log"
done
