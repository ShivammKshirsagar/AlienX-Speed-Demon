import subprocess
import re
import os
import sys
import hashlib

DATA_DIR = os.path.abspath("data/d").replace("\\", "/")
OUT_DIR = os.path.abspath("out").replace("\\", "/")
EXPECTED_HASH = "a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e"

variants = [
    ("Current code (Baseline, B=0)", 0),
    ("Ball-clustering B=128", 128),
    ("Ball-clustering B=256", 256),
    ("Ball-clustering B=512", 512),
]

ROUNDS = 9
metrics = ["ordering", "remap", "build", "query", "total", "peak_anon", "settled"]
results = {name: {m: [] for m in metrics} for name, _ in variants}

print(f"=== 9-ROUND INTERLEAVED A/B BENCHMARK: BALL-CLUSTERING VS CURRENT CODE ===", flush=True)
print(f"Workers: 12 | Rounds: {ROUNDS} | Expected Hash: {EXPECTED_HASH[:16]}...", flush=True)

for r in range(1, ROUNDS + 1):
    print(f"\n--- Round {r}/{ROUNDS} ---", flush=True)
    for name, b_val in variants:
        out_file = f"/out/ab_ball_{b_val}.txt"
        local_out = os.path.join("out", f"ab_ball_{b_val}.txt")
        cmd = [
            "docker", "run", "--rm",
            "-v", f"{DATA_DIR}:/data:ro",
            "-v", f"{OUT_DIR}:/out",
            "speeddemon-ball",
            "/data/graph.txt", "/data/queries.txt", out_file,
            str(b_val), "12"
        ]
        res = subprocess.run(cmd, capture_output=True, text=True)
        out = res.stdout + res.stderr
        # Parse RESULT: B=128 workers=12 ordering=120.12ms remap=141.58ms build=931.82ms query=1760.31ms total=2695.16ms peak_anon=103.35MB settled=23226954 ns_per_node=75.8
        pattern = r"RESULT: B=\d+ workers=\d+ ordering=([0-9.]+)ms remap=([0-9.]+)ms build=([0-9.]+)ms query=([0-9.]+)ms total=([0-9.]+)ms peak_anon=([0-9.]+)MB settled=(\d+) ns_per_node=([0-9.]+)"
        m = re.search(pattern, out)
        if m:
            ordering_ms = float(m.group(1))
            remap_ms = float(m.group(2))
            build_ms = float(m.group(3))
            query_ms = float(m.group(4))
            total_ms = float(m.group(5))
            peak_anon = float(m.group(6))
            settled = int(m.group(7))

            # Verify output hash
            with open(local_out, "rb") as f:
                h = hashlib.sha256(f.read()).hexdigest()
            assert h == EXPECTED_HASH, f"Hash mismatch in round {r} for {name}: {h}"

            results[name]["ordering"].append(ordering_ms)
            results[name]["remap"].append(remap_ms)
            results[name]["build"].append(build_ms)
            results[name]["query"].append(query_ms)
            results[name]["total"].append(total_ms)
            results[name]["peak_anon"].append(peak_anon)
            results[name]["settled"].append(settled)

            print(f"  {name:<28}: order={ordering_ms:6.2f}ms remap={remap_ms:6.2f}ms build={build_ms:7.2f}ms query={query_ms:7.2f}ms total={total_ms:7.2f}ms peak_anon={peak_anon:5.2f}MB [HASH OK]", flush=True)
        else:
            print(f"  {name:<28}: FAILED! Output:\n{out[:300]}", flush=True)

def stats(vals):
    s = sorted(vals)
    n = len(s)
    med = s[n // 2]
    spread = s[-1] - s[0]
    return med, s[0], s[-1], spread

print("\n" + "=" * 115, flush=True)
print("=== FINAL 9-ROUND INTERLEAVED A/B BENCHMARK SUMMARY (MEDIAN AND SPREAD) ===", flush=True)
header = f"{'Variant':<28} | {'Ordering(ms)':<14} | {'Remap(ms)':<14} | {'Total Build(ms)':<16} | {'Query(ms)':<18} | {'Total(ms)':<18} | {'Peak Anon(MB)':<14}"
print(header, flush=True)
print("-" * 115, flush=True)

for name, _ in variants:
    o_med, _, _, o_sp = stats(results[name]["ordering"])
    r_med, _, _, r_sp = stats(results[name]["remap"])
    b_med, _, _, b_sp = stats(results[name]["build"])
    q_med, q_min, q_max, q_sp = stats(results[name]["query"])
    t_med, t_min, t_max, t_sp = stats(results[name]["total"])
    p_med, _, _, p_sp = stats(results[name]["peak_anon"])

    o_str = f"{o_med:.2f} (±{o_sp/2:.1f})"
    r_str = f"{r_med:.2f} (±{r_sp/2:.1f})"
    b_str = f"{b_med:.2f} (±{b_sp/2:.1f})"
    q_str = f"{q_med:.2f} [{q_min:.1f}..{q_max:.1f}]"
    t_str = f"{t_med:.2f} [{t_min:.1f}..{t_max:.1f}]"
    p_str = f"{p_med:.2f}"

    print(f"{name:<28} | {o_str:<14} | {r_str:<14} | {b_str:<16} | {q_str:<18} | {t_str:<18} | {p_str:<14}", flush=True)

print("=" * 115, flush=True)
