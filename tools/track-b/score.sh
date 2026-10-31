#!/usr/bin/env bash
set -euo pipefail
repo='/mnt/c/GitHub Desktop/AlienX-Speed-Demon'
bin="$repo/track-b/target/release/solution"
expected='f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927'
tmp=$(mktemp -d "$repo/tools/track-b/.score.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/src"
cp "$repo/track-b/src/main.rs" "$tmp/src/main.rs"
cp "$repo/track-b/Dockerfile" "$tmp/Dockerfile"
: > "$tmp/rustfmt-defaults.toml"
rustfmt='/mnt/c/Users/kshir/.cargo/bin/rustfmt.exe'
"$rustfmt" --edition 2021 --config-path "$(wslpath -w "$tmp/rustfmt-defaults.toml")" "$(wslpath -w "$tmp/src/main.rs")"
S=$(awk 'BEGIN{n=0; block=0} {x=$0; if(block){if(x~/\*\//){sub(/^.*\*\//,"",x);block=0}else next} while(x~/\/\*/){pre=x;sub(/\/\*.*/,"",pre);rest=x;sub(/^.*\/\*/,"",rest);if(rest~/\*\//){sub(/\*\/.*/,"",rest);x=pre rest}else{x=pre;block=1;break}} sub(/\/\/.*/,"",x);if(x~/[^[:space:]]/)n++} END{print n}' "$tmp/src/main.rs")
S=$((S + $(awk 'NF && $0 !~ /^[[:space:]]*#/ {n++} END{print n+0}' "$tmp/Dockerfile")))
hash_ok=1
cat /data/docs.txt >/dev/null
for run in 1 2 3; do
  log="$tmp/run$run.log"; peaks="$tmp/peaks$run"
  SD_TIMING=1 taskset -c 0-11 "$bin" /data "/data/score-$run" > /dev/null 2>"$log" & pid=$!
  : > "$peaks"
  while kill -0 "$pid" 2>/dev/null; do
    awk '/^RssAnon:/{a=$2}/^VmHWM:/{h=$2}/^RssFile:/{f=$2}END{print a+0,h+0,f+0}' "/proc/$pid/status" >> "$peaks" 2>/dev/null || true
    sleep 0.002
  done
  wait "$pid"
done
actual=$(sha256sum /data/score-3/results.txt | cut -d' ' -f1)
[[ $actual == "$expected" ]] || hash_ok=0
python3 - "$tmp" "$S" "$hash_ok" <<'PY'
import glob, math, re, statistics, sys
root, s, hash_ok = sys.argv[1], int(sys.argv[2]), sys.argv[3] == "1"
logs=[open(p).read() for p in sorted(glob.glob(root+"/run*.log"))]
rows=[]
for log in logs:
    m=re.search(r"SD_TIMING ([^\n]+)",log)
    rows.append(dict((k,int(v)) for k,v in re.findall(r"(\w+)=([0-9]+)",m.group(1))))
phases={k:min(r[k] for r in rows) for k in ("parse","table","setup","scan_b","queries","output","total")}
mem=[]; workers=[]
for log,p in zip(logs,sorted(glob.glob(root+"/peaks*"))):
    for a,h,f in re.findall(r"SD_MEM phase=\S+ anon=(\d+) hwm=(\d+) file=(\d+)",log): mem.append((int(a),int(h),int(f)))
    for a,h,f in (tuple(map(int,x.split())) for x in open(p) if x.strip()): mem.append((a,h,f))
    for m in re.findall(r"workers=\[([^]]*)\]",log): workers += [int(x) for x in m.split(",") if x.strip()]
peak_anon=max((x[0] for x in mem),default=0); vmhwm=max((x[1] for x in mem),default=0); rssfile=max((x[2] for x in mem),default=0)
scan_minor=statistics.median(r["scan_minor"] for r in rows); scan_major=statistics.median(r["scan_major"] for r in rows)
board_t=phases["total"]*0.55/1e6; M=max(1,peak_anon/1024)
K=math.sqrt(56.35/board_t)*(2319/M)**0.2*(108/s)**0.15
points=50*math.log10(K)
print(" ".join([*(f"{k}_ms={phases[k]}" for k in ("parse","table","setup","scan_b","queries","output","total")),f"peak_anon_kb={peak_anon}",f"VmHWM_kb={vmhwm}",f"RssFile_kb={rssfile}",f"scan_faults_minor_major={int(scan_minor)}/{int(scan_major)}",f"worker_us_min_median_max={min(workers)}/{int(statistics.median(workers))}/{max(workers)}",f"S={s}","M_basis=sampled-peak-RssAnon",f"T_board_est={board_t:.4f}",f"K_est={K:.3f}",f"points_est={points:.2f}",f"hash={'OK' if hash_ok else 'FAIL'}"]))
PY
