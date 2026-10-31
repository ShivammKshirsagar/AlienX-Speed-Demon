#!/usr/bin/env bash
set -euo pipefail
solution='/mnt/c/GitHub Desktop/AlienX-Speed-Demon/track-b/target/release/solution'
cd '/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
for fixture in tools/track-b/phrase-boundary tools/track-b/example; do
  name=${fixture##*/}
  output="/data/track-b-test-$name"
  taskset -c 0-11 "$solution" "$fixture" "$output" > /dev/null
  cmp "$fixture/expected.txt" "$output/results.txt"
done
data='/data/track-b-sample-2'
reference='/data/track-b-reference-sample-2'
taskset -c 0-11 "$solution" "$data" "$data/rust-out" > /dev/null
cmp "$reference/results.txt" "$data/rust-out/results.txt"
sha256sum "$data/rust-out/results.txt"
