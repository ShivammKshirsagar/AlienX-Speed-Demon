import subprocess
import re
import os
import sys
import hashlib

DATA_DIR = os.path.abspath("data/d").replace("\\", "/")
OUT_DIR = os.path.abspath("out").replace("\\", "/")
EXPECTED_HASH = "a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e"

def parse_metrics(out_text):
    pattern = r"RESULT: .*?build=([0-9.]+)ms query=([0-9.]+)ms total=([0-9.]+)ms peak_anon=([0-9.]+)MB settled=(\d+) ns_per_node=([0-9.]+)"
    m = re.search(pattern, out_text)
    if m:
        return {
            "build": float(m.group(1)),
            "query": float(m.group(2)),
            "total": float(m.group(3)),
            "peak_anon": float(m.group(4)),
            "settled": int(m.group(5)),
            "ns_per_node": float(m.group(6)),
        }
    return None

def stats(vals):
    s = sorted(vals)
    n = len(s)
    med = s[n // 2]
    spread = s[-1] - s[0]
    return med, s[0], s[-1], spread

print("=" * 80)
print("=== PART A: 9-ROUND INTERLEAVED A/B: STEP 5 vs INTEGRATED BUILD ===")
print("=" * 80, flush=True)

variants_part_a = [
    ("Step 5 (8-byte edges)", ["docker", "run", "--rm", "-v", f"{DATA_DIR}:/data:ro", "-v", f"{OUT_DIR}:/out", "speeddemon-interleaved", "/data/graph.txt", "/data/queries.txt", "/out/part_a_s5.txt", "1", "0", "12"], os.path.join("out", "part_a_s5.txt")),
    ("Integrated (packed 4-byte edges)", ["docker", "run", "--rm", "-v", f"{DATA_DIR}:/data:ro", "-v", f"{OUT_DIR}:/out", "speeddemon-interleaved", "/data/graph.txt", "/data/queries.txt", "/out/part_a_int.txt", "1", "1", "12"], os.path.join("out", "part_a_int.txt")),
]

results_a = {name: {m: [] for m in ["build", "query", "total", "peak_anon", "settled", "ns_per_node"]} for name, _, _ in variants_part_a}

for r in range(1, 10):
    print(f"\n--- Round {r}/9 (Part A) ---", flush=True)
    for name, cmd, out_path in variants_part_a:
        res = subprocess.run(cmd, capture_output=True, text=True)
        m = parse_metrics(res.stdout + res.stderr)
        assert m is not None, f"Failed output: {res.stderr}\n{res.stdout}"
        with open(out_path, "rb") as f:
            h = hashlib.sha256(f.read()).hexdigest()
        assert h == EXPECTED_HASH, f"Hash mismatch: {h}"
        for k, v in m.items():
            results_a[name][k].append(v)
        print(f"  {name:<32}: build={m['build']:6.1f}ms query={m['query']:6.1f}ms total={m['total']:6.1f}ms peak_anon={m['peak_anon']:5.2f}MB [HASH OK]", flush=True)

print("\n" + "=" * 80)
print("=== PART B: 1-WORKER BENCHMARK: QUADHEAP vs RADIXHEAP PROTOTYPE ===")
print("=" * 80, flush=True)

for heap_type, hname in [(0, "QuadHeap (1 worker)"), (1, "RadixHeap (1 worker)")]:
    out_file = f"/out/1w_heap_{heap_type}.txt"
    cmd = ["docker", "run", "--rm", "-v", f"{DATA_DIR}:/data:ro", "-v", f"{OUT_DIR}:/out", "speeddemon-radix", "/data/graph.txt", "/data/queries.txt", out_file, str(heap_type), "1"]
    res = subprocess.run(cmd, capture_output=True, text=True)
    m = parse_metrics(res.stdout + res.stderr)
    assert m is not None, f"Failed: {res.stderr}\n{res.stdout}"
    with open(os.path.join("out", f"1w_heap_{heap_type}.txt"), "rb") as f:
        h = hashlib.sha256(f.read()).hexdigest()
    assert h == EXPECTED_HASH, f"Hash mismatch: {h}"
    print(f"  {hname:<25}: query={m['query']:7.1f}ms settled={m['settled']} ns/node={m['ns_per_node']:.2f} peak_anon={m['peak_anon']:.2f}MB [HASH OK]", flush=True)

print("\n" + "=" * 80)
print("=== PART C: 9-ROUND INTERLEAVED A/B: QUADHEAP vs RADIXHEAP (12 WORKERS) ===")
print("=" * 80, flush=True)

variants_part_c = [
    ("Integrated QuadHeap", ["docker", "run", "--rm", "-v", f"{DATA_DIR}:/data:ro", "-v", f"{OUT_DIR}:/out", "speeddemon-radix", "/data/graph.txt", "/data/queries.txt", "/out/part_c_quad.txt", "0", "12"], os.path.join("out", "part_c_quad.txt")),
    ("RadixHeap Prototype", ["docker", "run", "--rm", "-v", f"{DATA_DIR}:/data:ro", "-v", f"{OUT_DIR}:/out", "speeddemon-radix", "/data/graph.txt", "/data/queries.txt", "/out/part_c_radix.txt", "1", "12"], os.path.join("out", "part_c_radix.txt")),
]

results_c = {name: {m: [] for m in ["build", "query", "total", "peak_anon", "settled", "ns_per_node"]} for name, _, _ in variants_part_c}

for r in range(1, 10):
    print(f"\n--- Round {r}/9 (Part C) ---", flush=True)
    for name, cmd, out_path in variants_part_c:
        res = subprocess.run(cmd, capture_output=True, text=True)
        m = parse_metrics(res.stdout + res.stderr)
        assert m is not None, f"Failed: {res.stderr}\n{res.stdout}"
        with open(out_path, "rb") as f:
            h = hashlib.sha256(f.read()).hexdigest()
        assert h == EXPECTED_HASH, f"Hash mismatch: {h}"
        for k, v in m.items():
            results_c[name][k].append(v)
        print(f"  {name:<25}: build={m['build']:6.1f}ms query={m['query']:6.1f}ms total={m['total']:6.1f}ms peak_anon={m['peak_anon']:5.2f}MB [HASH OK]", flush=True)

print("\n" + "=" * 105)
print("=== FINAL A/B BENCHMARK SUMMARY (MEDIANS AND SPREADS) ===")
print("=" * 105)
header = f"{'Section / Variant':<32} | {'Build (ms)':<16} | {'Query (ms)':<20} | {'Total (ms)':<20} | {'Peak Anon (MB)':<12}"
print(header)
print("-" * 105)

for name in results_a:
    b_med, _, _, b_sp = stats(results_a[name]["build"])
    q_med, q_min, q_max, q_sp = stats(results_a[name]["query"])
    t_med, t_min, t_max, t_sp = stats(results_a[name]["total"])
    p_med, _, _, p_sp = stats(results_a[name]["peak_anon"])
    print(f"{name:<32} | {b_med:6.1f} (±{b_sp/2:4.1f})   | {q_med:6.1f} [{q_min:5.1f}..{q_max:5.1f}]  | {t_med:6.1f} [{t_min:5.1f}..{t_max:5.1f}]  | {p_med:5.2f} MB")

print("-" * 105)
for name in results_c:
    b_med, _, _, b_sp = stats(results_c[name]["build"])
    q_med, q_min, q_max, q_sp = stats(results_c[name]["query"])
    t_med, t_min, t_max, t_sp = stats(results_c[name]["total"])
    p_med, _, _, p_sp = stats(results_c[name]["peak_anon"])
    print(f"{name:<32} | {b_med:6.1f} (±{b_sp/2:4.1f})   | {q_med:6.1f} [{q_min:5.1f}..{q_max:5.1f}]  | {t_med:6.1f} [{t_min:5.1f}..{t_max:5.1f}]  | {p_med:5.2f} MB")

print("=" * 105, flush=True)
