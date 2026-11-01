#!/usr/bin/env bash
set -euo pipefail
repo='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
work="$HOME/track-a-rewrite-bench"
mkdir -p /data /out "$work/baseline/src" "$work/candidate/src"
cp "$repo/data/a/requests.log" /data/requests.log
cat /data/requests.log >/dev/null
for variant in baseline candidate; do
  if [[ $variant == baseline ]]; then source="$repo/tools/track-a/baseline-snapshot"; else source="$repo/track-a"; fi
  cp "$source/Cargo.toml" "$work/$variant/Cargo.toml"
  cp -a "$source/src/." "$work/$variant/src/"
done
cp "$repo/tools/track-a/time-wrapper.rs" "$work/time-wrapper.rs"
rustc -O "$work/time-wrapper.rs" -o "$work/time-wrapper"
for variant in baseline candidate; do
  RUSTFLAGS='-C target-cpu=x86-64-v3' cargo build --release --offline --manifest-path "$work/$variant/Cargo.toml"
done
expected='495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3'
printf 'variant\trun\twall_ns\tmax_rss_kb\tinput_subtracted_kb\tsha256\n' >"$work/results.tsv"
for run in $(seq 1 15); do
  if (( run % 2 )); then variants=(baseline candidate); else variants=(candidate baseline); fi
  for variant in "${variants[@]}"; do
    out="/out/${variant}-${run}.txt"; log="$work/${variant}-${run}.log"; rss="$work/${variant}-${run}.rss"
    /usr/bin/time -f '%M' -o "$rss" taskset -c 0-11 "$work/time-wrapper" \
      "$work/$variant/target/release/solution" /data/requests.log "$out" 2>"$log"
    ns=$(sed -n 's/^wrapper elapsed_ns=//p' "$log" | tail -1)
    [[ -n $ns ]] || { echo "missing nanosecond time for $variant $run" >&2; cat "$log" >&2; exit 1; }
    hash=$(sha256sum "$out" | cut -d' ' -f1)
    [[ $hash == "$expected" ]] || { echo "$variant $run hash mismatch: $hash" >&2; exit 1; }
    maxrss=$(cat "$rss")
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$variant" "$run" "$ns" "$maxrss" "$((maxrss - 233239))" "$hash" >>"$work/results.tsv"
  done
done
awk -F '\t' 'NR>1 {n[$1]++; t[$1,n[$1]]=$3; r[$1,n[$1]]=$5} END {for(v in n){for(i=1;i<=n[v];i++){for(j=i+1;j<=n[v];j++)if(t[v,i]>t[v,j]){x=t[v,i];t[v,i]=t[v,j];t[v,j]=x};if(r[v,i]>r[v,j]){x=r[v,i];r[v,i]=r[v,j];r[v,j]=x}}} printf "%s min_ns=%s median_ns=%s max_rss_proxy_kb=%s\n",v,t[v,1],t[v,8],r[v,n[v]]}}' "$work/results.tsv"
echo "raw results: $work/results.tsv"
