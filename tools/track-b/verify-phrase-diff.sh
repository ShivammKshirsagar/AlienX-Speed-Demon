#!/usr/bin/env bash
set -euo pipefail
work='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
rustc --edition=2021 -A warnings -O -C target-cpu=x86-64-v3 "$work/tools/track-b/phrase-diff.rs" -o /tmp/track-b-phrase-diff
for seed in 2 3; do taskset -c 0-11 /tmp/track-b-phrase-diff "/data/track-b-sample-$seed"; done
