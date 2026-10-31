#!/usr/bin/env bash
set -euo pipefail
repo='/mnt/c/GitHub Desktop/speed-demon-2026'
solution='/mnt/c/GitHub Desktop/AlienX-Speed-Demon/track-b/target/release/solution'
for seed in 2 3; do
  data="/data/track-b-seed-${seed}"
  reference="/data/track-b-reference-${seed}"
  python3 "$repo/generators/b_index.py" --seed "$seed" --scale sample --out "$data"
  python3 "$repo/reference/b_index.py" --data "$data" --out "$reference"
  taskset -c 0-11 "$solution" "$data" "$data/rust-out" > "$data/results.txt"
  cmp "$reference/results.txt" "$data/results.txt"
  sha256sum "$data/results.txt"
done