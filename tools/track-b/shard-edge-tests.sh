#!/usr/bin/env bash
set -euo pipefail
repo='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
candidate=/tmp/track-b-shard
baseline="$repo/track-b/target/release/solution"
mkdir -p /data/shard-edge-13 /data/shard-edge-3
cat > /data/shard-edge-13/docs.txt <<'EOF'
0	alpha edge
1	token next alpha
2	
3	misc beta
4	alpha token
5	edge
6	token
7	alpha edge token
8	
9	next token
10	beta alpha
11	z
12	alpha token
EOF
cat > /data/shard-edge-13/queries.txt <<'EOF'
BOOL alpha
BOOL alpha OR beta AND token
PHRASE "edge token"
PHRASE "token next"
PHRASE "alpha"
EOF
cat > /data/shard-edge-13/expected.txt <<'EOF'
6 34 0 1 4 7 10 12
4 24 1 4 7 12
1 7 7
1 1 1
6 34 0 1 4 7 10 12
EOF
cat > /data/shard-edge-3/docs.txt <<'EOF'
0	alpha
1	
2	beta alpha
EOF
cat > /data/shard-edge-3/queries.txt <<'EOF'
BOOL alpha
BOOL beta
PHRASE "beta alpha"
EOF
cat > /data/shard-edge-3/expected.txt <<'EOF'
2 2 0 2
1 2 2
1 2 2
EOF
for data in /data/shard-edge-13 /data/shard-edge-3; do
  name=${data##*/}
  "$candidate" "$data" "/data/${name}-candidate" >/dev/null
  "$baseline" "$data" "/data/${name}-baseline" >/dev/null
  cmp "$data/expected.txt" "/data/${name}-candidate/results.txt"
  cmp "$data/expected.txt" "/data/${name}-baseline/results.txt"
  printf '%s PASS\n' "$name"
done
