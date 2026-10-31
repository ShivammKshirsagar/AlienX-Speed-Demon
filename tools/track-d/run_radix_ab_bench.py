import subprocess
import re
import os
import sys
import hashlib

DATA_DIR = os.path.abspath("data/d").replace("\\", "/")
OUT_DIR = os.path.abspath("out").replace("\\", "/")
EXPECTED_HASH = "a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e"

variants = [
    ("QuadHeap (Integrated Baseline)", 0),
    ("RadixHeap (Line-Trimmed Everywhere)", 1),
]

ROUNDS = 9
metrics = ["build", "query", "total", "peak_anon", "settled", "ns_per_node"]
results = {name: {m: [] for m in metrics} for name, _ in variants}

print(f"=== 9-ROUND INTERLEAVED A/B BENCHMARK: QUADHEAP VS RADIXHEAP ===", flush=True)
print(f"Workers: 12 | Rounds: {ROUNDS} | Expected Hash: {EXPECTED_HASH[:16]}...", flush=True)

for r in range(1, ROUNDS + 1):
    print(f"\n--- Round {r}/{ROUNDS} ---", flush=True)
    for name, heap_type in variants:
        out_file = f"/out/ab_radix_{heap_type}.txt"
        local_out = os.path.join("out", f"ab_radix_{heap_type}.txt")
        cmd = [
            "docker", "run", "--rm",
            "-v", f"{DATA_DIR}:/data:ro",
            "-v", f"{OUT_DIR}:/out",
            "speeddemon-radix",
            "/data/graph.txt", "/data/queries.txt", out_file,
            str(heap_type), "12"
        ]
        res = subprocess.run(cmd, capture_output=True, text=True)
        out = res.stdout + res.stderr
        pattern = r"RESULT: heap_type=\d+ workers=\d+ build=([0-9.]+)ms query=([0-9.]+)ms total=([0-9.]+)ms peak_anon=([0-9.]+)MB settled=(\d+) ns_per_node=([0-9.]+)"
        m = re.search(pattern, out)
        if m:
            build_ms = float(m.group(1))
            query_ms = float(m.group(2))
            total_ms = float(m.group(3))
            peak_anon = float(m.group(4))
            settled = int(m.group(5))
            ns_per_node = float(m.group(6))

            # Verify hash
            with open(local_out, "rb") as f:
                h = hashlib.sha256(f.read()).hexdigest()
            assert h == EXPECTED_HASH, f"Hash mismatch in round {r} for {name}: {h}"

            results[name]["build"].append(build_ms)
            results[name]["query"].append(query_ms)
            results[name]["total"].append(total_ms)
            results[name]["peak_anon"].append(peak_anon)
            results[name]["settled"].append(settled)
            results[name]["ns_per_node"].append(ns_per_node)

            print(f"  {name:<36}: build={build_ms:7.2f}ms query={query_ms:7.2f}ms total={total_ms:7.2f}ms peak_anon={peak_anon:5.2f}MB ns_per_node={ns_per_node:5.2f} [HASH OK]", flush=True)
        else:
            print(f"  {name:<36}: FAILED! Output:\n{out[:300]}", flush=True)

def stats(vals):
    s = sorted(vals)
    n = len(s)
    med = s[n // 2]
    spread = s[-1] - s[0]
    return med, s[0], s[-1], spread

print("\n" + "="*80)
print("FINAL SUMMARY (9 Interleaved Rounds, 12 Workers, Shipped Flags):")
print("="*80)

base_name = variants[0][0]
cand_name = variants[1][0]

for m in ["build", "query", "total", "peak_anon", "ns_per_node"]:
    unit = "MB" if m == "peak_anon" else ("ns" if m == "ns_per_node" else "ms")
    print(f"\nMetric: {m.upper()} ({unit})")
    for name, _ in variants:
        med, vmin, vmax, spread = stats(results[name][m])
        print(f"  {name:<36}: median={med:7.2f}{unit} (min={vmin:7.2f}, max={vmax:7.2f}, spread={spread:6.2f}{unit})")

# Overlap check on total time
b_med, b_min, b_max, b_sp = stats(results[base_name]["total"])
c_med, c_min, c_max, c_sp = stats(results[cand_name]["total"])

overlap = max(0.0, min(b_max, c_max) - max(b_min, c_min))
print(f"\nTotal Time Overlap Analysis:")
print(f"  Baseline range: [{b_min:.2f}, {b_max:.2f}] ms")
print(f"  Radix    range: [{c_min:.2f}, {c_max:.2f}] ms")
if c_max < b_min:
    print(f"  Zero overlap: YES! Candidate max ({c_max:.2f} ms) is strictly less than baseline min ({b_min:.2f} ms). Gap = {b_min - c_max:.2f} ms")
else:
    print(f"  Overlap: {overlap:.2f} ms")

# Net K calculation
# K formula: T^-0.5 * M^-0.2 * S^-0.15
# Relative K: (T_b / T_c)^0.5 * (M_b / M_c)^0.2 * (S_b / S_c)^0.15
S_b = 568  # code lines
S_c = 599  # code lines
T_b = b_med / 1000.0  # seconds
T_c = c_med / 1000.0
M_b = stats(results[base_name]["peak_anon"])[0]
M_c = stats(results[cand_name]["peak_anon"])[0]

time_factor = (T_b / T_c) ** 0.5
mem_factor = (M_b / M_c) ** 0.2
size_factor = (S_b / S_c) ** 0.15
rel_k = time_factor * mem_factor * size_factor
net_k_pct = (rel_k - 1.0) * 100.0

print(f"\nNet K Improvement Calculation:")
print(f"  Time factor (T^-0.5): ({T_b:.4f}s / {T_c:.4f}s)^0.5 = {time_factor:.4f} ({(time_factor - 1)*100:+.2f}%)")
print(f"  Memory factor (M^-0.2): ({M_b:.2f}MB / {M_c:.2f}MB)^0.2 = {mem_factor:.4f} ({(mem_factor - 1)*100:+.2f}%)")
print(f"  Size factor (S^-0.15): ({S_b} / {S_c})^0.15 = {size_factor:.4f} ({(size_factor - 1)*100:+.2f}%)")
print(f"  Net K Relative: {rel_k:.4f}")
print(f"  Net K Delta: {net_k_pct:+.2f}%")

passed_gate = (S_c - S_b <= 50) and (c_max < b_min) and (net_k_pct >= 3.0)
print(f"\nGate Assessment (<= +50 lines, zero overlap, net K >= +3%):")
print(f"  Lines Delta <= +50: {S_c - S_b:+} lines -> {'PASS' if S_c - S_b <= 50 else 'FAIL'}")
print(f"  Zero Overlap: {'PASS' if c_max < b_min else 'FAIL'}")
print(f"  Net K >= +3.0%: {net_k_pct:+.2f}% -> {'PASS' if net_k_pct >= 3.0 else 'FAIL'}")
print(f"  Overall Gate Status: {'PASSED' if passed_gate else 'FAILED'}")
