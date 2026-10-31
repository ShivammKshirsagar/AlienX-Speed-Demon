import subprocess
import re
import os
import sys
import hashlib

DATA_DIR = os.path.abspath("data/d").replace("\\", "/")
OUT_DIR = os.path.abspath("out").replace("\\", "/")
EXPECTED_HASH = "a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e"

variants = [
    ("Baseline (N=1, std edges)", 1, 0),
    ("Interleaved (N=2, std edges)", 2, 0),
    ("Interleaved (N=4, std edges)", 4, 0),
    ("Packed 4-byte edges (N=1)", 1, 1),
    ("Combined (N=2, packed edges)", 2, 1),
]

ROUNDS = 9
metrics = ["build", "query", "total", "peak_anon", "settled"]
results = {name: {m: [] for m in metrics} for name, _, _ in variants}

print(f"=== 9-ROUND INTERLEAVED A/B BENCHMARK: INTERLEAVED & PACKED EDGES ===", flush=True)
print(f"Workers: 12 | Rounds: {ROUNDS} | Expected Hash: {EXPECTED_HASH[:16]}...", flush=True)

for r in range(1, ROUNDS + 1):
    print(f"\n--- Round {r}/{ROUNDS} ---", flush=True)
    for name, n_val, packed_val in variants:
        out_file = f"/out/ab_{n_val}_{packed_val}.txt"
        local_out = os.path.join("out", f"ab_{n_val}_{packed_val}.txt")
        cmd = [
            "docker", "run", "--rm",
            "-v", f"{DATA_DIR}:/data:ro",
            "-v", f"{OUT_DIR}:/out",
            "speeddemon-interleaved",
            "/data/graph.txt", "/data/queries.txt", out_file,
            str(n_val), str(packed_val), "12"
        ]
        res = subprocess.run(cmd, capture_output=True, text=True)
        out = res.stdout + res.stderr
        pattern = r"RESULT: N=\d+ packed=\d+ workers=\d+ build=([0-9.]+)ms query=([0-9.]+)ms total=([0-9.]+)ms peak_anon=([0-9.]+)MB settled=(\d+) ns_per_node=([0-9.]+)"
        m = re.search(pattern, out)
        if m:
            build_ms = float(m.group(1))
            query_ms = float(m.group(2))
            total_ms = float(m.group(3))
            peak_anon = float(m.group(4))
            settled = int(m.group(5))

            # Verify hash
            with open(local_out, "rb") as f:
                h = hashlib.sha256(f.read()).hexdigest()
            assert h == EXPECTED_HASH, f"Hash mismatch in round {r} for {name}: {h}"

            results[name]["build"].append(build_ms)
            results[name]["query"].append(query_ms)
            results[name]["total"].append(total_ms)
            results[name]["peak_anon"].append(peak_anon)
            results[name]["settled"].append(settled)

            print(f"  {name:<30}: build={build_ms:7.2f}ms query={query_ms:7.2f}ms total={total_ms:7.2f}ms peak_anon={peak_anon:5.2f}MB [HASH OK]", flush=True)
        else:
            print(f"  {name:<30}: FAILED! Output:\n{out[:300]}", flush=True)

def stats(vals):
    s = sorted(vals)
    n = len(s)
    med = s[n // 2]
    spread = s[-1] - s[0]
    return med, s[0], s[-1], spread

print("\n" + "=" * 110, flush=True)
print("=== FINAL 9-ROUND INTERLEAVED A/B BENCHMARK SUMMARY (MEDIAN AND SPREAD) ===", flush=True)
header = f"{'Variant':<30} | {'Total Build(ms)':<16} | {'Query(ms)':<22} | {'Total(ms)':<22} | {'Peak Anon(MB)':<14}"
print(header, flush=True)
print("-" * 110, flush=True)

for name, _, _ in variants:
    b_med, _, _, b_sp = stats(results[name]["build"])
    q_med, q_min, q_max, q_sp = stats(results[name]["query"])
    t_med, t_min, t_max, t_sp = stats(results[name]["total"])
    p_med, _, _, p_sp = stats(results[name]["peak_anon"])

    b_str = f"{b_med:.2f} (±{b_sp/2:.1f})"
    q_str = f"{q_med:.2f} [{q_min:.1f}..{q_max:.1f}]"
    t_str = f"{t_med:.2f} [{t_min:.1f}..{t_max:.1f}]"
    p_str = f"{p_med:.2f}"

    print(f"{name:<30} | {b_str:<16} | {q_str:<22} | {t_str:<22} | {p_str:<14}", flush=True)

print("=" * 110, flush=True)
