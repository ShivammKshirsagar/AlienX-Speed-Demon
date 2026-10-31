#!/usr/bin/env bash
set -euo pipefail
repo='/mnt/c/GitHub Desktop/speed-demon-2026'
work='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
data='/data/track-b-seed-1-sample'
python3 "$repo/generators/b_index.py" --seed 1 --scale sample --out "$data"
rustc -A warnings -O -C target-cpu=x86-64-v3 "$work/tools/track-b/scanner-diff.rs" -o /tmp/track-b-scanner-diff
taskset -c 0 /tmp/track-b-scanner-diff "$data"
