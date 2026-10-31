#!/usr/bin/env bash
set -euo pipefail
repo='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
tag='speeddemon-track-b-peak-once'
out='/data/track-b-docker-peak-out'
mkdir -p "$out"
chmod 777 "$out"
docker build --network=none -t "$tag" "$repo/track-b"
docker run --rm --network=none --memory=8g --memory-swap=8g --cpuset-cpus=0-11 \
  --read-only --tmpfs /tmp:rw,size=1g,exec --pids-limit=512 --cap-drop=ALL \
  --security-opt=no-new-privileges --user 0:0 --ulimit nofile=8192:8192 \
  -e SD_TIMING=1 -v /data:/data:ro -v "$out:/out:rw" "$tag"
