#!/usr/bin/env bash
set -euo pipefail
label=${1:?variant label required}
solution='/mnt/c/GitHub Desktop/AlienX-Speed-Demon/track-b/target/release/solution'
mkdir -p "/data/sd-$label"
cat /data/docs.txt >/dev/null
for run in 1 2 3; do
  log="/data/sd-$label/$run.log"
  if [[ $run == 1 ]]; then
    SD_TIMING=1 taskset -c 0-11 "$solution" /data "/data/sd-$label/out" >/dev/null 2>"$log"
  else
    SD_TIMING=1 taskset -c 0-11 "$solution" /data "/data/sd-$label/out" >/dev/null 2>"$log"
  fi
  cat "$log"
done
actual=$(sha256sum "/data/sd-$label/out/results.txt" | cut -d' ' -f1)
echo "seed1_hash=$actual"
