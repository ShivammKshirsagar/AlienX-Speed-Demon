import subprocess
import re
import os
import sys

def run_cmd(cmd):
    res = subprocess.run(cmd, capture_output=True, text=True)
    return res.stdout + res.stderr

data_dir = os.path.abspath("data/d")
out_dir = os.path.abspath("out")

warm_cmd = [
    "docker", "run", "--rm",
    "-v", f"{data_dir}:/data:ro",
    "-v", f"{out_dir}:/out",
    "speeddemon-check/track-d-profiler",
    "/data/graph.txt", "/data/queries.txt", "/out/distances.txt"
]

print("=== Benchmarking 12 Workers (5 runs) ===")
q_times_12 = []
total_times_12 = []
peak_anons_12 = []

print("Warming cache...")
out = run_cmd(warm_cmd)
print("Warm output snippet:\n", "\n".join(out.strip().split("\n")[:4]))

for i in range(5):
    out = run_cmd(warm_cmd)
    m = re.search(r"queries=([0-9.]+)ms.*total=([0-9.]+)ms", out)
    m_anon = re.search(r"peak_anon=([0-9.]+)MB", out)
    q_ms = float(m.group(1)) if m else 0.0
    tot_ms = float(m.group(2)) if m else 0.0
    if m:
        q_times_12.append(q_ms)
        total_times_12.append(tot_ms)
    else:
        print("Regex failed to match! Full output:\n", out)
    anon_val = float(m_anon.group(1)) if m_anon else 0.0
    peak_anons_12.append(anon_val)
    print(f"Run {i+1}: queries={q_ms:.2f}ms, total={tot_ms:.2f}ms, peak_anon_during_queries={anon_val:.2f}MB", flush=True)

q_times_12.sort()
total_times_12.sort()
peak_anons_12.sort()

print(f"\n12-worker Median: queries={q_times_12[2]:.2f}ms ({q_times_12[2]/1000.0:.3f}s), total={total_times_12[2]:.2f}ms ({total_times_12[2]/1000.0:.3f}s), peak_anon={peak_anons_12[2]:.2f}MB", flush=True)

print("\n=== Benchmarking 1 Worker (5 runs) ===", flush=True)
q_times_1 = []
total_times_1 = []
ns_per_nodes_1 = []

cmd_1w = [
    "docker", "run", "--rm",
    "--cpuset-cpus=0",
    "-v", f"{data_dir}:/data:ro",
    "-v", f"{out_dir}:/out",
    "speeddemon-check/track-d-profiler",
    "/data/graph.txt", "/data/queries.txt", "/out/distances.txt"
]

for i in range(5):
    out = run_cmd(cmd_1w)
    m = re.search(r"queries=([0-9.]+)ms.*total=([0-9.]+)ms.*\((\d+\.?\d*) ns/settled", out)
    q_ms = float(m.group(1)) if m else 0.0
    tot_ms = float(m.group(2)) if m else 0.0
    ns_node = float(m.group(3)) if m else 0.0
    if m:
        q_times_1.append(q_ms)
        total_times_1.append(tot_ms)
        ns_per_nodes_1.append(ns_node)
        print(f"Run {i+1}: queries={q_ms:.2f}ms, total={tot_ms:.2f}ms, ns/settled={ns_node:.1f}", flush=True)
    else:
        print("Regex failed for 1-worker! Full output:\n", out, flush=True)

q_times_1.sort()
total_times_1.sort()
ns_per_nodes_1.sort()

print(f"\n1-worker Median: queries={q_times_1[2]:.2f}ms ({q_times_1[2]/1000.0:.3f}s), total={total_times_1[2]:.2f}ms ({total_times_1[2]/1000.0:.3f}s), ns/settled={ns_per_nodes_1[2]:.1f}", flush=True)
