#!/usr/bin/env bash
set -euo pipefail
repo='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
harness='/mnt/c/GitHub Desktop/speed-demon-2026'
bin="$repo/track-a/target/release/solution"
work=/data/track-a-final-check
mkdir -p "$work"

compare() {
  local name="$1" data="$2"; shift 2
  "$@" "$bin" "$data/requests.log" "$work/$name-rust.txt"
  python3 "$harness/reference/a_ingest.py" --data "$data" --out "$work/$name-ref"
  cmp "$work/$name-rust.txt" "$work/$name-ref/summary.txt"
  echo "$name: PASS"
}

for seed in 2 3 7; do
  compare "seed$seed-sample" "$repo/tools/track-a/data/seed$seed" env
done
python3 "$harness/generators/a_ingest.py" --seed 2 --scale full --out "$work/seed2-full"
compare seed2-full "$work/seed2-full" env

python3 - "$work" <<'PY'
from pathlib import Path
import sys
root = Path(sys.argv[1])
rows = []
for i in range(1200):
    absolute = 18 + (i % 72)
    month, day = [(10, 30), (10, 31), (11, 1), (11, 2)][absolute // 24]
    hour = absolute % 24
    endpoint = f"/boundary/{i % 71:03d}"
    status = "503" if i % 7 == 0 else "200"
    rows.append(f"2026-{month:02d}-{day:02d}T{hour:02d}:00:00.000000Z GET {endpoint} {status} {i % 100003}\n")
(root / "boundary").mkdir(exist_ok=True)
(root / "boundary/requests.log").write_text("".join(rows), encoding="ascii")
(root / "single").mkdir(exist_ok=True)
(root / "single/requests.log").write_text(rows[0], encoding="ascii")
rows = [f"2026-10-30T18:00:00.000000Z GET /endpoint/{i:04d} 200 {i % 1000}\n" for i in range(5000)]
(root / "ecap").mkdir(exist_ok=True)
(root / "ecap/requests.log").write_text("".join(rows), encoding="ascii")
PY
compare line-boundaries "$work/boundary" env
compare single-line "$work/single" env
compare ecap-growth "$work/ecap" taskset -c 0
