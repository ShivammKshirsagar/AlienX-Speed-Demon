#!/usr/bin/env bash
set -euo pipefail
repo='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
cd "$repo/track-b"
RUSTFLAGS='-C target-cpu=x86-64-v3' cargo build --release --offline
cat /data/docs.txt >/dev/null
SD_TIMING=1 SD_QTIMING=1 taskset -c 0-11 ./target/release/solution /data /data/track-b-query-profile >/dev/null 2>/data/track-b-query-profile.log
grep -E 'SD_QCPU|SD_TIMING|SD_CGROUP' /data/track-b-query-profile.log
