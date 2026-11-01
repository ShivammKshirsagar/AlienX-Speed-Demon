#!/usr/bin/env bash
set -euo pipefail

# Same-session, rotated Docker comparison. Set BENCH_ROUNDS only for a dry run;
# the requested board-style comparison uses 30 measured runs per image.
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DATA="${ROOT}/data/a"
OUT_ROOT="${ROOT}/.out/track-a-bench"
CSV="${OUT_ROOT}/runs.csv"
ROUNDS="${BENCH_ROUNDS:-30}"
EXPECTED="495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3"
IMAGES=(sd-track-a-base-new sd-track-a-n1 sd-track-a-n2 sd-track-a-n3)
LABELS=(baseline n1 n2 n3)
mkdir -p "$OUT_ROOT"
printf 'label,round,cpu_us,wall_ns,memory_peak_bytes\n' > "$CSV"

docker build --network=none -t "${IMAGES[0]}" "$ROOT/tools/track-a/baseline-snapshot2"
for label in n1 n2 n3; do
    docker build --network=none -t "sd-track-a-${label}" "$ROOT/tools/track-a/candidates/${label}"
done

run_one() {
    local image="$1" label="$2" round="$3"
    local out="$OUT_ROOT/$label"
    mkdir -p "$out"
    local shell='cpu0=$(awk '\''$1=="usage_usec"{print $2}'\'' /sys/fs/cgroup/cpu.stat); t0=$(date +%s%N); /app/solution /data/requests.log /out/summary.txt; rc=$?; t1=$(date +%s%N); cpu1=$(awk '\''$1=="usage_usec"{print $2}'\'' /sys/fs/cgroup/cpu.stat); peak=$(cat /sys/fs/cgroup/memory.peak); echo "METRIC cpu_us=$((cpu1-cpu0)) wall_ns=$((t1-t0)) memory_peak_bytes=$peak rc=$rc"; exit "$rc"'
    local log
    log="$(MSYS_NO_PATHCONV=1 docker run --rm --network=none --cpuset-cpus=0-11 --memory=8g --memory-swap=8g --read-only --tmpfs /tmp:rw,size=1g,exec --user 0:0 -v "$DATA:/data:ro" -v "$out:/out:rw" --entrypoint /bin/sh "$image" -c "$shell")"
    local metric hash cpu wall peak rc
    metric="$(printf '%s\n' "$log" | sed -n 's/^METRIC //p' | tail -n1)"
    [[ -n "$metric" ]] || { printf 'missing metric: %s\n' "$log" >&2; return 1; }
    eval "$metric"
    hash="$(sha256sum "$out/summary.txt" | awk '{print $1}')"
    [[ "$hash" == "$EXPECTED" ]] || { printf '%s round %s hash mismatch: %s\n' "$label" "$round" "$hash" >&2; return 1; }
    printf '%s,%s,%s,%s,%s\n' "$label" "$round" "$cpu_us" "$wall_ns" "$memory_peak_bytes" >> "$CSV"
    printf '%s %s cpu_us=%s wall_ns=%s peak=%s hash=PASS\n' "$label" "$round" "$cpu_us" "$wall_ns" "$memory_peak_bytes"
}

# Touch the full input once before collecting measurements.
for i in "${!IMAGES[@]}"; do run_one "${IMAGES[$i]}" "${LABELS[$i]}" warmup >/dev/null; done
for ((round=1; round<=ROUNDS; round++)); do
    for ((step=0; step<${#IMAGES[@]}; step++)); do
        index=$(((round + step - 1) % ${#IMAGES[@]}))
        run_one "${IMAGES[$index]}" "${LABELS[$index]}" "$round"
    done
done
python "${ROOT}/tools/track-a/summarize_bench.py" "$CSV" "$ROOT"
