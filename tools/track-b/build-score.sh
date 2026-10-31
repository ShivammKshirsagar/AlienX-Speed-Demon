#!/usr/bin/env bash
set -euo pipefail
repo='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
cd "$repo/track-b"
RUSTFLAGS='-C target-cpu=x86-64-v3' cargo build --release --offline
cd "$repo"
bash tools/track-b/score.sh
