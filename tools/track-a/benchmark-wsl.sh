#!/usr/bin/env bash
set -euo pipefail

repo='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
work="$HOME/track-a-bench"
mkdir -p /data /out "$work/baseline/src" "$work/candidate/src" "$work/profiler/src"

cp "$repo/data/a/requests.log" /data/requests.log
cat /data/requests.log >/dev/null
cp "$repo/tools/track-a/baseline/Cargo.toml" "$work/baseline/Cargo.toml"
cp -a "$repo/tools/track-a/baseline/src/." "$work/baseline/src/"
cp "$repo/track-a/Cargo.toml" "$work/candidate/Cargo.toml"
cp -a "$repo/track-a/src/." "$work/candidate/src/"
cp "$repo/tools/track-a/profiler/Cargo.toml" "$work/profiler/Cargo.toml"
cp -a "$repo/tools/track-a/profiler/src/." "$work/profiler/src/"

cargo build --release --offline --manifest-path "$work/baseline/Cargo.toml"
cargo build --release --offline --manifest-path "$work/candidate/Cargo.toml"
cargo build --release --offline --manifest-path "$work/profiler/Cargo.toml"

expected='495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3'
printf 'variant\trun\twall_s\tuser_s\tsys_s\tmajor_faults\tminor_faults\tmax_rss_kb\tsha256\n' | tee "$work/totals.tsv"
for run in $(seq 1 10); do
  for variant in baseline candidate; do
    binary="$work/$variant/target/release/solution"
    output="/out/${variant}-${run}.txt"
    metrics="$work/${variant}-${run}.time"
    taskset -c 0-11 /usr/bin/time -f '%e\t%U\t%S\t%F\t%R\t%M' -o "$metrics" \
      "$binary" /data/requests.log "$output"
    hash=$(sha256sum "$output" | cut -d' ' -f1)
    [[ "$hash" == "$expected" ]] || { echo "$variant run $run hash mismatch: $hash" >&2; exit 1; }
    printf '%s\t%s\t%s\t%s\n' "$variant" "$run" "$(cat "$metrics")" "$hash" | tee -a "$work/totals.tsv"
  done
done

printf 'variant\trun\twall_s\tuser_s\tsys_s\tmajor_faults\tminor_faults\tmax_rss_kb\tphase_log\n' | tee "$work/phases.tsv"
for run in $(seq 1 10); do
  for variant in mmap pread; do
    output="/out/profile-${variant}-${run}.txt"
    metrics="$work/profile-${variant}-${run}.time"
    phases="$work/profile-${variant}-${run}.log"
    if [[ "$variant" == pread ]]; then
      PROFILE_THREADS=12 PROFILE_PREAD=1 \
        /usr/bin/time -f '%e\t%U\t%S\t%F\t%R\t%M' -o "$metrics" \
        taskset -c 0-11 "$work/profiler/target/release/solution-profile" /data/requests.log "$output" \
        2>"$phases"
    else
      PROFILE_THREADS=12 \
        /usr/bin/time -f '%e\t%U\t%S\t%F\t%R\t%M' -o "$metrics" \
        taskset -c 0-11 "$work/profiler/target/release/solution-profile" /data/requests.log "$output" \
        2>"$phases"
    fi
    hash=$(sha256sum "$output" | cut -d' ' -f1)
    [[ "$hash" == "$expected" ]] || { echo "$variant profile run $run hash mismatch: $hash" >&2; exit 1; }
    printf '%s\t%s\t%s\t%s\n' "$variant" "$run" "$(cat "$metrics")" "$phases" | tee -a "$work/phases.tsv"
  done
done

median_column() {
  awk -F '\t' -v variant="$1" -v column="$2" 'NR > 1 && $1 == variant { print $column }' "$work/totals.tsv" \
    | sort -n | awk '{ values[NR] = $1 } END { printf "%.3f", (values[5] + values[6]) / 2 }'
}
printf 'variant\twall_s\tuser_s\tsys_s\tmajor_faults\tminor_faults\tmax_rss_kb\n' | tee "$work/medians.tsv"
for variant in baseline candidate; do
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$variant" \
    "$(median_column "$variant" 3)" "$(median_column "$variant" 4)" \
    "$(median_column "$variant" 5)" "$(median_column "$variant" 6)" \
    "$(median_column "$variant" 7)" "$(median_column "$variant" 8)" | tee -a "$work/medians.tsv"
done

echo "Results: $work/totals.tsv and $work/phases.tsv; detailed phase logs: $work/profile-*.log"
