import subprocess
import os
import sys

TEST_DIR = os.path.abspath("out/test_correctness").replace("\\", "/")
os.makedirs("out/test_correctness", exist_ok=True)

N_NODES = 2500000 # > 2^21 (2,097,152)

# Specific test edges
# (u, v, w)
edges = [
    (0, 1, 2046),
    (1, 2, 2047),
    (2, 2499999, 4294967295),
    (0, 2499999, 4294967295),
    (3, 4, 500),
    (4, 5, 2046),
    (5, 6, 2047),
    (6, 7, 4294967295),
    (7, 8, 10),
    (10, 2499998, 1000),
    (2499998, 2499999, 2046),
]

queries = [
    (0, 1),
    (1, 2),
    (0, 2),
    (2, 2499999),
    (0, 2499999),
    (3, 4),
    (3, 5),
    (3, 6),
    (3, 7),
    (3, 8),
    (10, 2499999),
    (2499999, 0),
    (2499999, 2499999),
    (1, 0),
]

expected = [
    2046,
    2047,
    4093,
    4294967295,
    4294967295,
    500,
    2546,
    4593,
    4294971888,
    4294971898,
    3046,
    -1,
    0,
    -1,
]

# Write graph.txt
with open("out/test_correctness/graph.txt", "w") as f:
    f.write(f"{N_NODES} {len(edges)}\n")
    for u, v, w in edges:
        f.write(f"{u} {v} {w}\n")

# Write queries.txt
with open("out/test_correctness/queries.txt", "w") as f:
    for s, t in queries:
        f.write(f"{s} {t}\n")

configs = [
    ("Baseline (N=1, std edges)", 1, 0),
    ("Interleaved (N=2, std edges)", 2, 0),
    ("Interleaved (N=4, std edges)", 4, 0),
    ("Interleaved (N=8, std edges)", 8, 0),
    ("Packed edges (N=1, packed)", 1, 1),
    ("Combined (N=2, packed)", 2, 1),
    ("Combined (N=4, packed)", 4, 1),
    ("Combined (N=8, packed)", 8, 1),
]

print("=== RUNNING CORRECTNESS TESTS (n=2,500,000 > 2^21, weights 2046, 2047, 4294967295) ===")

all_passed = True
for name, n_val, packed_val in configs:
    out_file = f"/test/out_{n_val}_{packed_val}.txt"
    local_out = f"out/test_correctness/out_{n_val}_{packed_val}.txt"
    cmd = [
        "docker", "run", "--rm",
        "-v", f"{TEST_DIR}:/test",
        "speeddemon-interleaved",
        "/test/graph.txt", "/test/queries.txt", out_file,
        str(n_val), str(packed_val), "4"
    ]
    res = subprocess.run(cmd, capture_output=True, text=True)
    if res.returncode != 0:
        print(f"FAILED to run {name}: {res.stderr}")
        all_passed = False
        continue

    with open(local_out, "r") as f:
        actual = [int(line.strip()) for line in f if line.strip()]

    if actual == expected:
        print(f"  {name:<32}: PASS (all {len(expected)} queries matched exactly!)")
    else:
        print(f"  {name:<32}: MISMATCH!")
        for i, (exp, act) in enumerate(zip(expected, actual)):
            if exp != act:
                print(f"    query {i} ({queries[i]}): expected {exp}, got {act}")
        all_passed = False

if all_passed:
    print("\nALL CORRECTNESS TESTS PASSED SUCCESSFULLY!")
else:
    sys.exit(1)
