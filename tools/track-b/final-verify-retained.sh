#!/usr/bin/env bash
set -euo pipefail
repo='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
solution="$repo/track-b/target/release/solution"
cd "$repo"
cd track-b
RUSTFLAGS='-C target-cpu=x86-64-v3' cargo build --release --offline
cd "$repo"
cat /data/docs.txt >/dev/null
for fixture in tools/track-b/example tools/track-b/phrase-boundary; do
  name=${fixture##*/}
  output="/data/retained-$name"
  taskset -c 0-11 "$solution" "$fixture" "$output" >/dev/null
  cmp "$fixture/expected.txt" "$output/results.txt"
  printf '%s PASS\n' "$name"
done
for seed in 2 3 4 5; do
  data="/data/track-b-sample-$seed"
  reference="/data/track-b-reference-$seed"
  output="/data/retained-sample-$seed"
  taskset -c 0-11 "$solution" "$data" "$output" >/dev/null
  cmp "$reference/results.txt" "$output/results.txt"
  sha256sum "$output/results.txt"
done
taskset -c 0-11 "$solution" /data /data/retained-full >/dev/null
sha256sum /data/retained-full/results.txt
