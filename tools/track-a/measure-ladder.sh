#!/usr/bin/env bash
set -euo pipefail
repo='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
harness="$HOME/track-a-ladder"
mkdir -p /data /out "$harness/baseline/src" "$harness/profiler/src"
cp "$repo/data/a/requests.log" /data/requests.log
cat /data/requests.log >/dev/null
cp "$repo/tools/track-a/baseline-snapshot2/Cargo.toml" "$harness/baseline/Cargo.toml"
cp -a "$repo/tools/track-a/baseline-snapshot2/src/." "$harness/baseline/src/"
cp "$repo/tools/track-a/profiler/Cargo.toml" "$harness/profiler/Cargo.toml"
cp -a "$repo/tools/track-a/profiler/src/." "$harness/profiler/src/"
RUSTFLAGS='-C target-cpu=x86-64-v3' cargo build --release --offline --manifest-path "$harness/baseline/Cargo.toml"
RUSTFLAGS='-C target-cpu=x86-64-v3' cargo build --release --offline --manifest-path "$harness/profiler/Cargo.toml"
rustc -O -C target-cpu=x86-64-v3 "$repo/tools/track-a/time-wrapper.rs" -o "$harness/time-wrapper"
rustc --edition=2021 -O -C target-cpu=x86-64-v3 "$repo/tools/track-a/ablation.rs" -o "$harness/ablation"
expected='495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3'
PROFILE_THREADS=12 taskset -c 0-11 "$harness/time-wrapper" \
  "$harness/profiler/target/release/solution-profile" /data/requests.log /out/baseline-profile.txt \
  2>"$harness/baseline-profile.log"
hash=$(sha256sum /out/baseline-profile.txt | cut -d' ' -f1)
[[ $hash == "$expected" ]] || { echo "baseline phase profile hash mismatch: $hash" >&2; exit 1; }
echo 'Baseline phase timings:'
grep -E '^profile phase=|^profile workers_started_finished_ns=|^profile lifecycle ' "$harness/baseline-profile.log"
echo 'Parse ablation ladder (warm input; CPU ns per line and wall):'
taskset -c 0-11 "$harness/ablation" /data/requests.log | tee "$harness/ablation.txt"
