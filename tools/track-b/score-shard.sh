#!/usr/bin/env bash
set -euo pipefail
repo='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
baseline="$repo/tools/track-b/baseline-src/main.rs"
export SD_BIN=/tmp/track-b-shard
export SD_SRC="$repo/tools/track-b/shard-main.rs"
export SD_S_EXTRA=$(awk 'BEGIN{n=0; block=0} {x=$0; if(block){if(x~/\*\//){sub(/^.*\*\//,"",x);block=0}else next} while(x~/\/\*/){pre=x;sub(/\/\*.*/,"",pre);rest=x;sub(/^.*\/\*/,"",rest);if(rest~/\*\//){sub(/\*\/.*/,"",rest);x=pre rest}else{x=pre;block=1;break}} sub(/\/\/.*/,"",x);if(x~/[^[:space:]]/)n++} END{print n}' "$baseline")
exec bash "$repo/tools/track-b/score.sh"
