#!/usr/bin/env bash
set -euo pipefail

repo='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
binary="$HOME/track-a-bench/current-release/target/release/solution"
mkdir -p /data/samples /out/references

for seed in 2 3 7; do
  cp "$repo/tools/track-a/data/seed${seed}/requests.log" "/data/samples/seed${seed}.log"
  cp "$repo/tools/track-a/out/reference${seed}/summary.txt" "/out/references/seed${seed}.txt"
done

for seed in 2 3 7; do
  input="/data/samples/seed${seed}.log"
  output="/out/seed${seed}-linux.txt"
  expected="/out/references/seed${seed}.txt"
  taskset -c 0-11 "$binary" "$input" "$output"
  cmp "$output" "$expected"
  printf 'seed %s PASS %s\n' "$seed" "$(sha256sum "$output" | cut -d' ' -f1)"
done
