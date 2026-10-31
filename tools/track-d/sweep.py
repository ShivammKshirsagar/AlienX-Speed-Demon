import subprocess
import re
import os
import sys

def run_cmd(cmd):
    res = subprocess.run(cmd, capture_output=True, text=True)
    return res.stdout + res.stderr

data_dir = os.path.abspath("data/d").replace("\\", "/")
out_dir = os.path.abspath("out").replace("\\", "/")

init_caps = [
    ("16k", 16384),
    ("32k", 32768),
    ("64k", 65536),
    ("128k", 131072),
]

shrink_threshs = [
    ("never", 999999999),
    ("256k", 262144),
    ("512k", 524288),
]

print("=== SWEEPING INIT_CAP & SHRINK_THRESHOLD ===", flush=True)
print(f"{'INIT_CAP':<10} | {'SHRINK':<10} | {'12w Query (ms)':<15} | {'12w Total (ms)':<15} | {'1w ns/node':<12} | {'Peak Anon (MB)':<15}", flush=True)
print("-" * 85, flush=True)

results_table = []

for cap_name, cap_val in init_caps:
    for shrink_name, shrink_val in shrink_threshs:
        sys.stderr.write(f"Testing {cap_name} / {shrink_name}...\n")
        sys.stderr.flush()
        # Base 12-worker command
        cmd_12w = [
            "docker", "run", "--rm",
            "-e", "PROFILE_WORKERS=12",
            "-e", f"INIT_CAP={cap_val}",
            "-e", f"SHRINK_THRESH={shrink_val}",
            "-v", f"{data_dir}:/data:ro",
            "-v", f"{out_dir}:/out",
            "speeddemon-check/track-d-profiler",
            "/data/graph.txt", "/data/queries.txt", "/out/distances.txt"
        ]

        # 5 runs for 12-worker
        q_times = []
        tot_times = []
        anons = []
        for r in range(5):
            out = run_cmd(cmd_12w)
            m = re.search(r"queries=([0-9.]+)ms.*total=([0-9.]+)ms", out)
            m_anon = re.search(r"peak_anon=([0-9.]+)MB", out)
            if m:
                q_times.append(float(m.group(1)))
                tot_times.append(float(m.group(2)))
            if m_anon:
                anons.append(float(m_anon.group(1)))

        if len(q_times) < 3:
            print(f"Error in {cap_name} {shrink_name}: not enough runs succeeded!", flush=True)
            continue

        q_times.sort()
        tot_times.sort()
        anons.sort()
        q_med = q_times[len(q_times)//2]
        tot_med = tot_times[len(tot_times)//2]
        anon_med = anons[len(anons)//2] if anons else 0.0

        # 1-worker run for ns/settled node
        cmd_1w = [
            "docker", "run", "--rm",
            "-e", "PROFILE_WORKERS=1",
            "-e", f"INIT_CAP={cap_val}",
            "-e", f"SHRINK_THRESH={shrink_val}",
            "-v", f"{data_dir}:/data:ro",
            "-v", f"{out_dir}:/out",
            "speeddemon-check/track-d-profiler",
            "/data/graph.txt", "/data/queries.txt", "/out/distances.txt"
        ]
        out_1w = run_cmd(cmd_1w)
        m_1w = re.search(r"\((\d+\.?\d*) ns/settled", out_1w)
        ns_node = float(m_1w.group(1)) if m_1w else 0.0

        print(f"{cap_name:<10} | {shrink_name:<10} | {q_med:<15.2f} | {tot_med:<15.2f} | {ns_node:<12.1f} | {anon_med:<15.2f}", flush=True)
        results_table.append({
            "cap": cap_name,
            "shrink": shrink_name,
            "q_med": q_med,
            "tot_med": tot_med,
            "ns_node": ns_node,
            "anon_med": anon_med,
        })

print("\n=== SUMMARY OF SWEEP RESULTS ===", flush=True)
# Find best setting with anon <= 95 MB
valid = [r for r in results_table if r["anon_med"] <= 95.0]
if valid:
    valid.sort(key=lambda x: x["tot_med"])
    best = valid[0]
    print(f"Recommended setting (best time with anon <= 95MB): INIT_CAP={best['cap']}, SHRINK={best['shrink']}", flush=True)
    print(f"  -> 12-worker Query: {best['q_med']:.2f} ms ({best['q_med']/1000.0:.3f} s)")
    print(f"  -> 12-worker Total: {best['tot_med']:.2f} ms ({best['tot_med']/1000.0:.3f} s)")
    print(f"  -> 1-worker Rate:   {best['ns_node']:.1f} ns/settled node")
    print(f"  -> Peak Anon:       {best['anon_med']:.2f} MB")
