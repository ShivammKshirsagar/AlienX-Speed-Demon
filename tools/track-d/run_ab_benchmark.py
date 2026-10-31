import subprocess
import re
import os
import sys

DATA_DIR = os.path.abspath("data/d").replace("\\", "/")
OUT_DIR = os.path.abspath("out").replace("\\", "/")
EXPECTED_HASH = "a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e"

binaries = [
    ("Step 4 current", "/app/step4_current"),
    ("Step 4 without prefetch", "/app/step4_no_prefetch"),
    ("Step 4 prefetch offset[v]", "/app/step4_prefetch_offset"),
    ("Step 4 without shrinking", "/app/step4_no_shrink"),
    ("Step 4 without fallback lock", "/app/step4_no_fallback_lock"),
]

ROUNDS = 9
results = {name: {"query": [], "total": []} for name, _ in binaries}

print(f"=== INTERLEAVED SAME-SESSION A/B BENCHMARK (12 WORKERS, {ROUNDS} ROUNDS) ===", flush=True)

for r in range(1, ROUNDS + 1):
    print(f"\n--- Round {r}/{ROUNDS} ---", flush=True)
    for name, bin_path in binaries:
        cmd = [
            "docker", "run", "--rm",
            "-e", "WORKERS=12",
            "-v", f"{DATA_DIR}:/data:ro",
            "-v", f"{OUT_DIR}:/out",
            "speeddemon-ab-bench",
            bin_path, "/data/graph.txt", "/data/queries.txt", "/out/ab_out.txt"
        ]
        res = subprocess.run(cmd, capture_output=True, text=True)
        out = res.stdout + res.stderr
        m = re.search(r"TIMING: query=([0-9.]+)ms total=([0-9.]+)ms", out)
        if m:
            q_ms = float(m.group(1))
            t_ms = float(m.group(2))
            results[name]["query"].append(q_ms)
            results[name]["total"].append(t_ms)
            print(f"  {name:<30}: query={q_ms:7.2f} ms | total={t_ms:7.2f} ms", flush=True)
        else:
            print(f"  {name:<30}: FAILED! Output:\n{out[:200]}", flush=True)

print("\n" + "=" * 95, flush=True)
print("=== FINAL A/B BENCHMARK SUMMARY (MEDIAN AND SPREAD ACROSS 9 ROUNDS) ===", flush=True)
print(f"{'Variant':<30} | {'Query Median':<12} | {'Query Spread':<18} | {'Total Median':<12} | {'Total Spread':<18}", flush=True)
print("-" * 95, flush=True)

for name, _ in binaries:
    q_list = sorted(results[name]["query"])
    t_list = sorted(results[name]["total"])
    if not q_list:
        continue
    q_med = q_list[len(q_list) // 2]
    q_min = q_list[0]
    q_max = q_list[-1]
    q_spread = f"[{q_min:.1f} - {q_max:.1f}] (+{q_max - q_min:.1f})"

    t_med = t_list[len(t_list) // 2]
    t_min = t_list[0]
    t_max = t_list[-1]
    t_spread = f"[{t_min:.1f} - {t_max:.1f}] (+{t_max - t_min:.1f})"

    print(f"{name:<30} | {q_med:8.2f} ms | {q_spread:<18} | {t_med:8.2f} ms | {t_spread:<18}", flush=True)
print("=" * 95, flush=True)
