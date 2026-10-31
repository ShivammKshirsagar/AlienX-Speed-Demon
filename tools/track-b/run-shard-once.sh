#!/usr/bin/env bash
set -euo pipefail
cat /data/docs.txt >/dev/null
SD_TIMING=1 taskset -c 0-11 /tmp/track-b-shard /data /data/shard-full >/dev/null 2>/data/shard-full.log
sha256sum /data/shard-full/results.txt
cat /data/shard-full.log
