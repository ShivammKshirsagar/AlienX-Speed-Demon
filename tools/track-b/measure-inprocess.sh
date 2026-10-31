#!/usr/bin/env bash
set -euo pipefail
label=${1:?variant label required}
variant=${2:-A}
solution='/mnt/c/GitHub Desktop/AlienX-Speed-Demon/track-b/target/release/solution'
expected='f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927'
mkdir -p "/data/sd-$label"
cat /data/docs.txt >/dev/null
for run in 1 2 3 4 5; do
  log="/data/sd-$label/$run.log"
  if [[ $run == 1 ]]; then
    before=$(date +%s%N)
    SD_VARIANT="$variant" SD_TIMING=1 taskset -c 0-11 "$solution" /data "/data/sd-$label/out" >/dev/null 2>"$log"
    after=$(date +%s%N)
    internal=$(sed -n 's/.*total=\([0-9]*\).*/\1/p' "$log")
    echo "external_us=$(((after-before)/1000)) internal_us=$internal gap_us=$(((after-before)/1000-internal))" | tee "/data/sd-$label/external-gap.txt"
  else
    SD_VARIANT="$variant" SD_TIMING=1 taskset -c 0-11 "$solution" /data "/data/sd-$label/out" >/dev/null 2>"$log"
  fi
  actual=$(sha256sum "/data/sd-$label/out/results.txt" | cut -d' ' -f1)
  [[ $actual == "$expected" ]]
  echo "seed1_hash=$actual"
  cat "$log"
done
