#!/usr/bin/env bash
set -euo pipefail
solution='/mnt/c/GitHub Desktop/AlienX-Speed-Demon/track-b/target/release/solution'
for seed in 2 3 4 5; do
  data="/data/track-b-sample-${seed}"
  reference="/data/track-b-reference-${seed}"
  taskset -c 0-11 "$solution" "$data" "$data/rust-out" > /dev/null
  cmp "$reference/results.txt" "$data/rust-out/results.txt"
  sha256sum "$data/rust-out/results.txt"
done
