import subprocess
import os
import sys
import hashlib
import random
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
DATA_DIR = ROOT / "data" / "d"
OUT_DIR = ROOT / ".out" / "track-d"
OUT_DIR.mkdir(parents=True, exist_ok=True)
OUT_FILE = OUT_DIR / "distances.txt"

GENERATOR = ROOT.parent / "speed-demon-2026" / "generators" / "d_traverse.py"
REFERENCE = ROOT / "tools" / "track-d" / "reference" / "d_traverse.py"

def sha256_file(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()

def run_container(data_dir, workers=12, tag="speeddemon-check/track-d:latest"):
    if OUT_FILE.exists():
        OUT_FILE.unlink()
    cmd = ["docker", "run", "--rm", "--network=none", "--read-only",
           "--tmpfs", "/tmp:rw,size=1g,exec",
           "-e", "TRACK=d",
           "-v", f"{Path(data_dir).resolve()}:/data:ro",
           "-v", f"{OUT_DIR.resolve()}:/out:rw"]
    if workers == 1:
        cmd.extend(["--cpuset-cpus", "0"])
    elif workers == 4:
        cmd.extend(["--cpuset-cpus", "0-3"])
    cmd.append(tag)
    res = subprocess.run(cmd, capture_output=True, text=True)
    if res.returncode != 0:
        print(f"Container failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    return sha256_file(OUT_FILE)

print("=" * 70)
print("=== TRACK D: FULL VERIFICATION SUITE ===")
print("=" * 70, flush=True)

# 1. Edge Cases: Empty, Single Node, Zero Edges, Worked Example
print("\n--- 1. Edge Cases ---", flush=True)
edge_cases_dir = ROOT / ".out" / "edge_cases"
edge_cases_dir.mkdir(parents=True, exist_ok=True)

# 1a. Empty
(edge_cases_dir / "empty").mkdir(exist_ok=True)
with open(edge_cases_dir / "empty" / "graph.txt", "w") as f: f.write("0 0\n")
with open(edge_cases_dir / "empty" / "queries.txt", "w") as f: f.write("")
h = run_container(edge_cases_dir / "empty")
print(f"  Empty graph/queries: PASS (hash {h[:16]}...)", flush=True)

# 1b. Single Node (n=1, m=0)
(edge_cases_dir / "single").mkdir(exist_ok=True)
with open(edge_cases_dir / "single" / "graph.txt", "w") as f: f.write("1 0\n")
with open(edge_cases_dir / "single" / "queries.txt", "w") as f: f.write("0 0\n")
run_container(edge_cases_dir / "single")
with open(OUT_FILE) as f: assert f.read().strip() == "0"
print(f"  Single node (n=1, m=0, query 0 0 -> 0): PASS", flush=True)

# 1c. Zero Edges (m=0, n=5)
(edge_cases_dir / "zero_edges").mkdir(exist_ok=True)
with open(edge_cases_dir / "zero_edges" / "graph.txt", "w") as f: f.write("5 0\n")
with open(edge_cases_dir / "zero_edges" / "queries.txt", "w") as f: f.write("0 1\n2 2\n3 4\n")
run_container(edge_cases_dir / "zero_edges")
with open(OUT_FILE) as f: assert f.read().strip() == "-1\n0\n-1"
print(f"  Zero edges (m=0, n=5, queries: -1, 0, -1): PASS", flush=True)

# 1d. Worked Example (Small directed graph)
(edge_cases_dir / "worked").mkdir(exist_ok=True)
with open(edge_cases_dir / "worked" / "graph.txt", "w") as f:
    f.write("4 5\n0 1 10\n1 2 20\n0 2 35\n2 3 5\n0 3 100\n")
with open(edge_cases_dir / "worked" / "queries.txt", "w") as f:
    f.write("0 2\n0 3\n3 0\n1 1\n")
run_container(edge_cases_dir / "worked")
with open(OUT_FILE) as f: assert f.read().strip() == "30\n35\n-1\n0"
print(f"  Worked example (distances 30, 35, -1, 0): PASS", flush=True)

# 2. Tiny & Sample Seeds (1, 2, 3, 7)
print("\n--- 2. Tiny and Sample Seeds (1, 2, 3, 7) ---", flush=True)
for scale in ["tiny", "sample"]:
    for s in [1, 2, 3, 7]:
        tdir = ROOT / "data" / "generated" / f"{scale}-seed{s}"
        tdir.mkdir(parents=True, exist_ok=True)
        g_file = tdir / "graph.txt"
        q_file = tdir / "queries.txt"
        if not (g_file.exists() and q_file.exists()):
            subprocess.run([sys.executable, str(GENERATOR), "--seed", str(s), "--scale", scale, "--out", str(tdir)], check=True)
        # Run python ref
        ref_out = tdir / "distances.txt"
        if not ref_out.exists():
            subprocess.run([sys.executable, str(REFERENCE), "--data", str(tdir), "--out", str(tdir)], check=True)
        ref_h = sha256_file(ref_out)
        rust_h = run_container(tdir, workers=4)
        assert rust_h == ref_h, f"Mismatch in {scale}-seed{s}!"
        print(f"  {scale}-seed{s:<2}: PASS (hash {rust_h[:16]}... matches reference)", flush=True)

# 3. Full Seeds 1, 2, 3, 5 with 5 repeats at 1, 4, 12 workers
print("\n--- 3. Full Seeds (1, 2, 3, 5) with 5 Repeats at 1, 4, 12 Workers ---", flush=True)
FULL_SEEDS = [1, 2, 3, 5]
for s in FULL_SEEDS:
    if s == 1:
        sdir = ROOT / "data" / "d"
    else:
        sdir = ROOT / "data" / "generated" / f"full-seed{s}"
        sdir.mkdir(parents=True, exist_ok=True)
        g_file = sdir / "graph.txt"
        q_file = sdir / "queries.txt"
        if not (g_file.exists() and q_file.exists()):
            print(f"  Generating full-seed{s}...", flush=True)
            subprocess.run([sys.executable, str(GENERATOR), "--seed", str(s), "--scale", "full", "--out", str(sdir)], check=True)
    # Get expected hash
    ref_out = sdir / "ref_out.txt"
    if s == 1:
        exp_h = "a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e"
    elif ref_out.exists():
        exp_h = sha256_file(ref_out)
    else:
        print(f"  Computing reference hash for full-seed{s}...", flush=True)
        subprocess.run([sys.executable, str(REFERENCE), "--data", str(sdir), "--out", str(sdir)], check=True)
        # Rename distances.txt to ref_out.txt so it doesn't get confused
        if (sdir / "distances.txt").exists():
            (sdir / "distances.txt").replace(ref_out)
        exp_h = sha256_file(ref_out)

    for w in [1, 4, 12]:
        for rep in range(1, 6):
            h = run_container(sdir, workers=w)
            assert h == exp_h, f"Mismatch on full-seed{s} w={w} rep={rep}: {h} vs {exp_h}"
        print(f"  full-seed{s} ({w:>2} workers, 5/5 repeats): PASS (hash {exp_h[:16]}...)", flush=True)

# 4. Tiny-capacity run (INIT_CAP=16, MAX_CAP=32 stress test)
print("\n--- 4. Tiny-Capacity Stress Run ---", flush=True)
tiny_cap_code = open("track-d/src/main.rs").read()
tiny_cap_code = tiny_cap_code.replace("const INIT_CAP: usize = 16384;", "const INIT_CAP: usize = 16;")
tiny_cap_code = tiny_cap_code.replace("const MAX_CAP: usize = 524288;", "const MAX_CAP: usize = 32;")
os.makedirs("tools/track-d/tiny_cap_src/src", exist_ok=True)
open("tools/track-d/tiny_cap_src/src/main.rs", "w").write(tiny_cap_code)
open("tools/track-d/tiny_cap_src/Cargo.toml", "w").write(open("track-d/Cargo.toml").read())
open("tools/track-d/tiny_cap_src/Dockerfile", "w").write(open("track-d/Dockerfile").read())
subprocess.run(["docker", "build", "--network=none", "-t", "speeddemon-check/track-d:tiny", "tools/track-d/tiny_cap_src"], check=True)
tiny_h = run_container(ROOT / "data" / "d", workers=12, tag="speeddemon-check/track-d:tiny")
assert tiny_h == "a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e"
print(f"  Tiny-capacity run (INIT=16, MAX=32): PASS (hash {tiny_h[:16]}... matches expected)", flush=True)

# 5. 50 Stress Tests (including weights 2046, 2047, near 4,294,967,295)
print("\n--- 5. 50 Stress Tests with Critical Weights (2046, 2047, ~4.29B) ---", flush=True)
import heapq
def py_dijkstra(n, adj, s, t):
    dist = {s: 0}
    pq = [(0, s)]
    while pq:
        d, u = heapq.heappop(pq)
        if d > dist.get(u, float('inf')): continue
        if u == t: return d
        for v, w in adj.get(u, []):
            nd = d + w
            if nd < dist.get(v, float('inf')):
                dist[v] = nd
                heapq.heappush(pq, (nd, v))
    return -1

stress_dir = ROOT / ".out" / "stress_50"
stress_dir.mkdir(parents=True, exist_ok=True)
random.seed(42)

for test_idx in range(1, 51):
    n = random.randint(10, 500)
    m = random.randint(n, n * 4)
    # Weights include special boundary weights
    special_weights = [0, 1, 2046, 2047, 2048, 4294967294, 4294967295]
    adj = {}
    edges_list = []
    for _ in range(m):
        u = random.randint(0, n - 1)
        v = random.randint(0, n - 1)
        if random.random() < 0.3:
            w = random.choice(special_weights)
        else:
            w = random.randint(1, 10000)
        edges_list.append((u, v, w))
        adj.setdefault(u, []).append((v, w))

    q_count = 20
    test_queries = [(random.randint(0, n - 1), random.randint(0, n - 1)) for _ in range(q_count)]
    ref_answers = [py_dijkstra(n, adj, s, t) for s, t in test_queries]

    # Write
    with open(stress_dir / "graph.txt", "w") as f:
        f.write(f"{n} {len(edges_list)}\n")
        for u, v, w in edges_list:
            f.write(f"{u} {v} {w}\n")
    with open(stress_dir / "queries.txt", "w") as f:
        for s, t in test_queries:
            f.write(f"{s} {t}\n")

    run_container(stress_dir, workers=4)
    with open(OUT_FILE) as f:
        rust_answers = [int(line.strip()) for line in f if line.strip()]

    assert rust_answers == ref_answers, f"Stress test {test_idx} failed!"
    if test_idx % 10 == 0:
        print(f"  Stress tests {test_idx - 9}..{test_idx}/50: PASS (all queries exact match)", flush=True)

print("\n" + "=" * 70)
print("=== ALL FULL VERIFICATION TESTS PASSED SUCCESSFULLY! ===")
print("=" * 70, flush=True)
