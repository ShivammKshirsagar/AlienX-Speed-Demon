#!/usr/bin/env bash
set -euo pipefail

repo='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
work="$HOME/track-a-bench/current-release"
mkdir -p /data /out "$work/src"
cp "$repo/data/a/requests.log" /data/requests.log
cat /data/requests.log >/dev/null
cp "$repo/track-a/Cargo.toml" "$work/Cargo.toml"
cp -a "$repo/track-a/src/." "$work/src/"
RUSTFLAGS='-C target-cpu=x86-64-v3' cargo build --release --offline --manifest-path "$work/Cargo.toml"

expected='495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3'
printf 'run\twall_s\tuser_s\tsystem_s\tmajor_faults\tminor_faults\tmax_rss_kb\tsha256\n' >"$HOME/track-a-bench/candidate-release.tsv"
for run in $(seq 1 10); do
  output="/out/candidate-release-${run}.txt"
  metrics="$HOME/track-a-bench/candidate-release-${run}.time"
  taskset -c 0-11 /usr/bin/time -f '%e\t%U\t%S\t%F\t%R\t%M' -o "$metrics" \
    "$work/target/release/solution" /data/requests.log "$output"
  hash=$(sha256sum "$output" | cut -d' ' -f1)
  [[ "$hash" == "$expected" ]] || { echo "run $run hash mismatch: $hash" >&2; exit 1; }
  printf '%s\t%s\t%s\n' "$run" "$(cat "$metrics")" "$hash" >>"$HOME/track-a-bench/candidate-release.tsv"
done
awk -F '\t' 'NR > 1 { for (i = 2; i <= 7; i++) a[i, NR-1] = $i } END { for (i = 2; i <= 7; i++) { for (j = 1; j <= 10; j++) b[j] = a[i,j]; for (j = 1; j <= 10; j++) for (k = j + 1; k <= 10; k++) if (b[j] > b[k]) { t=b[j]; b[j]=b[k]; b[k]=t } printf "%s%s", i == 2 ? "medians" : "", i == 2 ? "" : "\t"; printf "%.3f", (b[5] + b[6]) / 2 } print "" }' "$HOME/track-a-bench/candidate-release.tsv"
