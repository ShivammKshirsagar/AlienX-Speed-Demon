import os
import sys
import subprocess
import hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
GENERATOR = ROOT.parent / "speed-demon-2026" / "generators" / "d_traverse.py"
REFERENCE = ROOT / "tools" / "track-d" / "reference" / "d_traverse.py"
DATA_BASE = ROOT / "data" / "generated"
OUT_DIR = ROOT / ".out"
OUT_DIR.mkdir(exist_ok=True)
OUT_FILE = OUT_DIR / "distances.txt"

def sha256_file(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()

def run_rust(data_path, workers=None):
    if OUT_FILE.exists():
        OUT_FILE.unlink()
    cmd = ["docker", "run", "--rm"]
    if workers == 1:
        cmd.extend(["--cpuset-cpus", "0"])
    elif workers == 4:
        cmd.extend(["--cpuset-cpus", "0-3"])
    cmd.extend([
        "-v", f"{data_path.resolve()}:/data:ro",
        "-v", f"{OUT_DIR.resolve()}:/out",
        "speeddemon-check/track-d:latest"
    ])
    res = subprocess.run(cmd, capture_output=True, text=True)
    if res.returncode != 0:
        print(f"Rust execution failed:\n{res.stderr}")
        sys.exit(1)
    return sha256_file(OUT_FILE)

SEEDS = [2, 3, 5]
results = {}

for seed in SEEDS:
    seed_dir = DATA_BASE / f"full-seed{seed}"
    seed_dir.mkdir(parents=True, exist_ok=True)
    graph_file = seed_dir / "graph.txt"
    queries_file = seed_dir / "queries.txt"

    # 1. Generate if needed
    if not (graph_file.exists() and queries_file.exists()):
        print(f"Generating full-scale seed {seed}...", flush=True)
        cmd_gen = [
            sys.executable, str(GENERATOR),
            "--seed", str(seed),
            "--scale", "full",
            "--out", str(seed_dir)
        ]
        res = subprocess.run(cmd_gen, capture_output=True, text=True)
        if res.returncode != 0:
            print(f"Generator failed for seed {seed}:\n{res.stderr}")
            sys.exit(1)
        print(f"Generated full seed {seed} ({graph_file.stat().st_size / 1024 / 1024:.1f} MB)", flush=True)
    else:
        print(f"Full-scale seed {seed} already present.", flush=True)

    # 2. Run reference
    ref_out_dir = seed_dir / "ref_out"
    ref_out_dir.mkdir(exist_ok=True)
    ref_file = ref_out_dir / "distances.txt"
    if not ref_file.exists():
        print(f"Running Python reference on full seed {seed} (takes ~70-80s)...", flush=True)
        cmd_ref = [
            sys.executable, str(REFERENCE),
            "--data", str(seed_dir),
            "--out", str(ref_out_dir)
        ]
        res = subprocess.run(cmd_ref, capture_output=True, text=True)
        if res.returncode != 0:
            print(f"Reference failed on seed {seed}:\n{res.stderr}")
            sys.exit(1)
    
    ref_hash = sha256_file(ref_file)
    print(f"Seed {seed} reference SHA-256: {ref_hash}", flush=True)
    results[seed] = {"ref_hash": ref_hash, "repeats": [], "workers": {}}

    # 3. 5 Repeated runs
    print(f"Running 5 repeated Rust runs on seed {seed}...", flush=True)
    for rep in range(1, 6):
        h = run_rust(seed_dir)
        assert h == ref_hash, f"Seed {seed} repeat {rep} mismatch: {h} != {ref_hash}"
        results[seed]["repeats"].append(h)
        print(f"  Seed {seed} rep {rep}: PASS ({h[:16]}...)", flush=True)

    # 4. Worker counts: 1, 4, 12 workers
    print(f"Testing worker counts on seed {seed} (1, 4, 12 workers)...", flush=True)
    for w in [1, 4, 12]:
        h = run_rust(seed_dir, workers=w)
        assert h == ref_hash, f"Seed {seed} @ {w} workers mismatch: {h} != {ref_hash}"
        results[seed]["workers"][w] = h
        print(f"  Seed {seed} @ {w:2d} workers: PASS ({h[:16]}...)", flush=True)

print("\n" + "=" * 80)
print("=== FULL-SCALE SEEDS 2, 3, 5 VERIFICATION SUMMARY ===")
print(f"{'Seed':<6} | {'Reference SHA-256':<64} | {'5 Repeats':<10} | {'1/4/12 Workers':<15}")
print("-" * 80)
for seed in SEEDS:
    ref_h = results[seed]["ref_hash"]
    reps_ok = "5/5 PASS" if len(results[seed]["repeats"]) == 5 else "FAIL"
    workers_ok = "ALL PASS" if len(results[seed]["workers"]) == 3 else "FAIL"
    print(f"{seed:<6} | {ref_h} | {reps_ok:<10} | {workers_ok:<15}")
print("=" * 80)
