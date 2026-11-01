#!/usr/bin/env bash
set -euo pipefail
repo='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
work="$HOME/track-a-ab"
expected='495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3'
if (($# < 2)); then echo "usage: $0 name=source-dir name=source-dir [...]" >&2; exit 2; fi
mkdir -p /data /out "$work"
cp "$repo/data/a/requests.log" /data/requests.log
cat /data/requests.log >/dev/null
input_kb=$(($(stat -c %s /data/requests.log) / 1024))
rustc -O "$repo/tools/track-a/time-wrapper.rs" -o "$work/time-wrapper"
names=(); dirs=()
for spec in "$@"; do
  name=${spec%%=*}; source=${spec#*=}; names+=("$name"); dirs+=("$source")
  mkdir -p "$work/$name/src"
  cp "$source/Cargo.toml" "$work/$name/Cargo.toml"
  cp -a "$source/src/." "$work/$name/src/"
  RUSTFLAGS='-C target-cpu=x86-64-v3' cargo build --release --offline --manifest-path "$work/$name/Cargo.toml"
done
: >"$work/results.tsv"
for run in $(seq 1 20); do
  shift_order=0
  if (( run % 2 == 0 )); then shift_order=1; fi
  for step in $(seq 0 $((${#names[@]} - 1))); do
    i=$(((step + shift_order) % ${#names[@]})); name=${names[$i]}
    output="/out/ab-${name}-${run}.txt"; log="$work/${name}-${run}.log"; rss="$work/${name}-${run}.rss"
    /usr/bin/time -f '%M' -o "$rss" taskset -c 0-11 "$work/time-wrapper" \
      "$work/$name/target/release/solution" /data/requests.log "$output" 2>"$log"
    ns=$(sed -n 's/^wrapper elapsed_ns=//p' "$log" | tail -1)
    [[ -n $ns ]] || { echo "missing nanosecond timer for $name run $run" >&2; cat "$log" >&2; exit 1; }
    hash=$(sha256sum "$output" | cut -d' ' -f1)
    [[ $hash == "$expected" ]] || { echo "$name run $run hash mismatch: $hash" >&2; exit 1; }
    maxrss=$(cat "$rss")
    printf '%s\t%s\t%s\t%s\t%s\n' "$name" "$run" "$ns" "$maxrss" "$((maxrss-input_kb))" >>"$work/results.tsv"
  done
done
python3 - "$work/results.tsv" <<'PY'
import statistics, sys
rows={}
for line in open(sys.argv[1]):
    name,run,ns,rss,proxy=line.rstrip().split('\t')
    rows.setdefault(name,[]).append((int(ns)/1e6,int(proxy)))
for name,values in rows.items():
    times=sorted(x[0] for x in values)
    print(f"{name}: min={times[0]:.3f}ms p25={statistics.quantiles(times,n=4,method='inclusive')[0]:.3f}ms median={statistics.median(times):.3f}ms max_RSS_proxy={max(x[1] for x in values)}KB")
PY
echo "raw results: $work/results.tsv"
