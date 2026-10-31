#!/usr/bin/env bash
set -euo pipefail

repo='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
work="$HOME/track-a-bench"
printf 'WSL logical CPUs: '; nproc
printf 'Affinity for 12-worker runs: '; taskset -pc $$
mkdir -p /data /out "$work/current/src" "$work/profiler/src"
cp "$repo/data/a/requests.log" /data/requests.log
cat /data/requests.log >/dev/null
cp "$repo/track-a/Cargo.toml" "$work/current/Cargo.toml"
cp -a "$repo/track-a/src/." "$work/current/src/"
cp "$repo/tools/track-a/profiler/Cargo.toml" "$work/profiler/Cargo.toml"
cp -a "$repo/tools/track-a/profiler/src/." "$work/profiler/src/"

RUSTFLAGS='-C target-cpu=x86-64-v3' cargo build --release --offline --manifest-path "$work/current/Cargo.toml"
RUSTFLAGS='-C target-cpu=x86-64-v3' cargo build --release --offline --manifest-path "$work/profiler/Cargo.toml"
rustc -O "$repo/tools/track-a/time-wrapper.rs" -o "$work/time-wrapper"

expected='495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3'
printf 'run\tsha256\n' >"$work/current.tsv"
for run in $(seq 1 10); do
  log="$work/current-${run}.log"
  output="/out/current-${run}.txt"
  PROFILE_THREADS=12 \
    taskset -c 0-11 "$work/time-wrapper" "$work/profiler/target/release/solution-profile" \
    /data/requests.log "$output" 2>"$log"
  hash=$(sha256sum "$output" | cut -d' ' -f1)
  [[ "$hash" == "$expected" ]] || { echo "run $run hash mismatch: $hash" >&2; exit 1; }
  printf '%s\t%s\n' "$run" "$hash" >>"$work/current.tsv"
done
python3 "$repo/tools/track-a/summarize-current-wsl.py" "$work"
