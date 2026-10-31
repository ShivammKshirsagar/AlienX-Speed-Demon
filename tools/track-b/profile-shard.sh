#!/usr/bin/env bash
set -euo pipefail
cat /data/docs.txt >/dev/null
export SD_TIMING=1
export SD_QTIMING=1
taskset -c 0-11 /tmp/track-b-shard /data /data/shard-profile-after >/dev/null 2>/data/shard-profile-after.log
grep -E 'SD_QCPU|SD_TIMING' /data/shard-profile-after.log
