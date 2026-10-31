#!/usr/bin/env bash
set -euo pipefail
repo='/mnt/c/GitHub Desktop/speed-demon-2026'
solution='/mnt/c/GitHub Desktop/AlienX-Speed-Demon/track-b/target/release/solution'
cd '/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
for fixture in tools/track-b/phrase-boundary tools/track-b/example; do
  name=${fixture##*/}
  output="/data/track-b-test-$name"
  taskset -c 0-11 "$solution" "$fixture" "$output" > /dev/null
  cmp "$fixture/expected.txt" "$output/results.txt"
done
for seed in 2 3 4 5; do
  data="/data/track-b-sample-$seed"
  reference="/data/track-b-reference-$seed"
  python3 "$repo/generators/b_index.py" --seed "$seed" --scale sample --out "$data"
  python3 "$repo/reference/b_index.py" --data "$data" --out "$reference"
  taskset -c 0-11 "$solution" "$data" "$data/rust-out" > /dev/null
  cmp "$reference/results.txt" "$data/rust-out/results.txt"
  sha256sum "$data/rust-out/results.txt"
done
