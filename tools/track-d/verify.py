#!/usr/bin/env python3
"""Comprehensive verification harness for Track D."""
import argparse
import hashlib
import os
import random
import shutil
import subprocess
import sys
import tempfile

WORKED_GRAPH = """8 12
0 1 4
1 2 3
0 2 9
2 0 5
2 1 6
1 3 0
3 4 0
4 4 7
4 5 4294967295
2 1 2
5 6 4294967295
0 1 4
"""

WORKED_QUERIES = """0 2
2 1
1 4
4 4
7 7
0 6
4 5
6 0
0 7
1 0
2 1
"""

WORKED_EXPECTED_HASH = "2e9871bba1d5013144ceff48d806a28da0e3f203b29118a89d98d8f654c3f6a0"
SAMPLE_SEED1_HASH = "a4c76ed030bc7e365db33191f6cf74708fba7f25eb48d64c8c4279f2e57046a0"
FULL_SEED1_HASH = "a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e"

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
REF_PATH = os.path.join(ROOT, "tools", "track-d", "reference", "d_traverse.py")


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def run_rust_docker(data_dir, out_dir, image_tag):
    cmd = [
        "docker", "run", "--rm",
        "--network=none",
        "--read-only",
        "--tmpfs", "/tmp:rw,size=1g,exec",
        "--memory=8g", "--memory-swap=8g",
        "--pids-limit=512",
        "--cap-drop=ALL",
        "--security-opt=no-new-privileges",
        "--user", "65534:65534",
        "-e", "TRACK=d",
        "-v", f"{os.path.abspath(data_dir)}:/data:ro",
        "-v", f"{os.path.abspath(out_dir)}:/out:rw",
        image_tag,
    ]
    res = subprocess.run(cmd, capture_output=True, text=True)
    if res.returncode != 0:
        print(f"Docker run failed (code {res.returncode}):\n{res.stderr}")
        sys.exit(1)


def run_reference(data_dir, out_dir):
    cmd = [sys.executable, REF_PATH, "--data", data_dir, "--out", out_dir]
    res = subprocess.run(cmd, capture_output=True, text=True)
    if res.returncode != 0:
        print(f"Reference failed:\n{res.stderr}")
        sys.exit(1)


def generate_stress_graph(rng):
    n = rng.randint(5, 30)
    edges = []
    
    # 1. Self loops
    for _ in range(rng.randint(1, 3)):
        u = rng.randint(0, n - 1)
        edges.append((u, u, rng.randint(0, 1000)))
        
    # 2. Weight 0 edges
    for _ in range(rng.randint(1, 4)):
        u = rng.randint(0, n - 1)
        v = rng.randint(0, n - 1)
        edges.append((u, v, 0))
        
    # 3. Very large weights
    for _ in range(rng.randint(1, 3)):
        u = rng.randint(0, n - 1)
        v = rng.randint(0, n - 1)
        w = 4294967295 - rng.randint(0, 100)
        edges.append((u, v, w))
        
    # 4. Parallel edges in both orders
    for _ in range(rng.randint(2, 5)):
        u = rng.randint(0, n - 1)
        v = rng.randint(0, n - 1)
        w1 = rng.randint(1, 500)
        w2 = rng.randint(1, 500)
        if rng.random() < 0.5:
            edges.append((u, v, min(w1, w2)))
            edges.append((u, v, max(w1, w2)))
        else:
            edges.append((u, v, max(w1, w2)))
            edges.append((u, v, min(w1, w2)))
            
    # 5. Exact duplicates
    if edges:
        dup = rng.choice(edges)
        edges.append(dup)
        
    # 6. Random edges
    for _ in range(rng.randint(5, 30)):
        u = rng.randint(0, n - 1)
        v = rng.randint(0, n - 1)
        edges.append((u, v, rng.randint(1, 10000)))
        
    # Isolated nodes, no-in, no-out naturally occur when n > node count in edges
    rng.shuffle(edges)
    m = len(edges)
    
    graph_text = f"{n} {m}\n" + "".join(f"{u} {v} {w}\n" for u, v, w in edges)
    
    # Queries: s==t, repeated, unreachable, etc.
    queries = []
    # s == t
    for _ in range(rng.randint(1, 4)):
        u = rng.randint(0, n - 1)
        queries.append((u, u))
    # normal / random queries
    for _ in range(rng.randint(10, 30)):
        u = rng.randint(0, n - 1)
        v = rng.randint(0, n - 1)
        queries.append((u, v))
    # duplicates
    if queries:
        queries.append(queries[0])
        queries.append(queries[-1])
        
    rng.shuffle(queries)
    query_text = "".join(f"{s} {t}\n" for s, t in queries)
    return graph_text, query_text


def main():
    parser = argparse.ArgumentParser(description="Track D verification")
    parser.add_argument("--image", default="speeddemon-check/track-d")
    args = parser.parse_args()

    print("=== Track D Verification ===")
    results_table = []

    with tempfile.TemporaryDirectory() as td:
        # 1. Worked example
        w_data = os.path.join(td, "worked_data")
        w_out = os.path.join(td, "worked_out")
        os.makedirs(w_data, exist_ok=True)
        os.makedirs(w_out, exist_ok=True)
        with open(os.path.join(w_data, "graph.txt"), "w", encoding="ascii", newline="\n") as f:
            f.write(WORKED_GRAPH)
        with open(os.path.join(w_data, "queries.txt"), "w", encoding="ascii", newline="\n") as f:
            f.write(WORKED_QUERIES)

        run_rust_docker(w_data, w_out, args.image)
        w_res_file = os.path.join(w_out, "distances.txt")
        w_bytes = open(w_res_file, "rb").read()
        w_hash = hashlib.sha256(w_bytes).hexdigest()
        assert len(w_bytes) == 42, f"Worked example length was {len(w_bytes)}, expected 42"
        assert w_hash == WORKED_EXPECTED_HASH, f"Worked example hash mismatch: {w_hash}"
        print(f"[PASS] Worked example: 42 bytes, SHA-256 {w_hash}")
        results_table.append(("Worked example", w_hash, "PASS (exact 42 bytes)"))

        # 2. Empty queries
        eq_data = os.path.join(td, "eq_data")
        eq_out = os.path.join(td, "eq_out")
        os.makedirs(eq_data, exist_ok=True)
        os.makedirs(eq_out, exist_ok=True)
        with open(os.path.join(eq_data, "graph.txt"), "w", encoding="ascii", newline="\n") as f:
            f.write("8 0\n")
        with open(os.path.join(eq_data, "queries.txt"), "w", encoding="ascii", newline="\n") as f:
            f.write("")
        run_rust_docker(eq_data, eq_out, args.image)
        eq_bytes = open(os.path.join(eq_out, "distances.txt"), "rb").read()
        assert len(eq_bytes) == 0, f"Empty queries gave {len(eq_bytes)} bytes"
        print("[PASS] Empty queries: 0 bytes output")
        results_table.append(("Empty queries", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "PASS (0 bytes)"))

        # 3. Single node graph (1 0)
        sn_data = os.path.join(td, "sn_data")
        sn_out = os.path.join(td, "sn_out")
        os.makedirs(sn_data, exist_ok=True)
        os.makedirs(sn_out, exist_ok=True)
        with open(os.path.join(sn_data, "graph.txt"), "w", encoding="ascii", newline="\n") as f:
            f.write("1 0\n")
        with open(os.path.join(sn_data, "queries.txt"), "w", encoding="ascii", newline="\n") as f:
            f.write("0 0\n")
        run_rust_docker(sn_data, sn_out, args.image)
        sn_bytes = open(os.path.join(sn_out, "distances.txt"), "rb").read()
        assert sn_bytes == b"0\n", f"Single node output: {sn_bytes!r}"
        print("[PASS] Single node (1 0, query 0 0): '0\\n'")
        results_table.append(("Single node (1 0)", hashlib.sha256(sn_bytes).hexdigest(), "PASS"))

        # 4. m = 0 graph with queries
        m0_data = os.path.join(td, "m0_data")
        m0_out_rust = os.path.join(td, "m0_out_rust")
        m0_out_ref = os.path.join(td, "m0_out_ref")
        os.makedirs(m0_data, exist_ok=True)
        os.makedirs(m0_out_rust, exist_ok=True)
        os.makedirs(m0_out_ref, exist_ok=True)
        with open(os.path.join(m0_data, "graph.txt"), "w", encoding="ascii", newline="\n") as f:
            f.write("5 0\n")
        with open(os.path.join(m0_data, "queries.txt"), "w", encoding="ascii", newline="\n") as f:
            f.write("0 0\n0 1\n2 2\n3 4\n")
        run_rust_docker(m0_data, m0_out_rust, args.image)
        run_reference(m0_data, m0_out_ref)
        m0_rust_bytes = open(os.path.join(m0_out_rust, "distances.txt"), "rb").read()
        m0_ref_bytes = open(os.path.join(m0_out_ref, "distances.txt"), "rb").read()
        assert m0_rust_bytes == m0_ref_bytes, f"m=0 mismatch: {m0_rust_bytes!r} vs {m0_ref_bytes!r}"
        print("[PASS] Graph with m=0: matches Python reference")
        results_table.append(("Graph m=0", hashlib.sha256(m0_rust_bytes).hexdigest(), "PASS"))

        # 5. Multi-seed datasets
        scales_seeds = [
            ("tiny", 1), ("tiny", 2), ("tiny", 3), ("tiny", 7),
            ("sample", 1), ("sample", 2), ("sample", 3), ("sample", 7),
        ]
        for scale, seed in scales_seeds:
            data_dir = os.path.join(ROOT, "data", "generated", f"{scale}-seed{seed}")
            rust_out = os.path.join(td, f"rust_{scale}_{seed}")
            ref_out = os.path.join(td, f"ref_{scale}_{seed}")
            os.makedirs(rust_out, exist_ok=True)
            os.makedirs(ref_out, exist_ok=True)

            run_rust_docker(data_dir, rust_out, args.image)
            run_reference(data_dir, ref_out)

            rust_hash = sha256_file(os.path.join(rust_out, "distances.txt"))
            ref_hash = sha256_file(os.path.join(ref_out, "distances.txt"))

            assert rust_hash == ref_hash, f"{scale} seed {seed} mismatch: Rust {rust_hash} vs Ref {ref_hash}"

            if scale == "sample" and seed == 1:
                assert rust_hash == SAMPLE_SEED1_HASH, f"Sample seed 1 expected {SAMPLE_SEED1_HASH}, got {rust_hash}"

            print(f"[PASS] {scale.capitalize()} seed {seed}: {rust_hash}")
            results_table.append((f"{scale.capitalize()} seed {seed}", rust_hash, "PASS"))

        # 6. Full seed 1
        full_data = os.path.join(ROOT, "data", "d")
        full_out = os.path.join(td, "full_out")
        os.makedirs(full_out, exist_ok=True)
        run_rust_docker(full_data, full_out, args.image)
        full_hash = sha256_file(os.path.join(full_out, "distances.txt"))
        assert full_hash == FULL_SEED1_HASH, f"Full seed 1 mismatch: expected {FULL_SEED1_HASH}, got {full_hash}"
        print(f"[PASS] Full seed 1: {full_hash}")
        results_table.append(("Full seed 1", full_hash, "PASS"))

        # 7. Stress test with random graphs
        print("Running 50 random graph stress tests...")
        for test_idx in range(50):
            seed = 42000 + test_idx
            rng = random.Random(seed)
            g_txt, q_txt = generate_stress_graph(rng)

            s_data = os.path.join(td, f"stress_{test_idx}_data")
            s_out_rust = os.path.join(td, f"stress_{test_idx}_rust")
            s_out_ref = os.path.join(td, f"stress_{test_idx}_ref")
            os.makedirs(s_data, exist_ok=True)
            os.makedirs(s_out_rust, exist_ok=True)
            os.makedirs(s_out_ref, exist_ok=True)

            with open(os.path.join(s_data, "graph.txt"), "w", encoding="ascii", newline="\n") as f:
                f.write(g_txt)
            with open(os.path.join(s_data, "queries.txt"), "w", encoding="ascii", newline="\n") as f:
                f.write(q_txt)

            run_rust_docker(s_data, s_out_rust, args.image)
            run_reference(s_data, s_out_ref)

            r_bytes = open(os.path.join(s_out_rust, "distances.txt"), "rb").read()
            ref_bytes = open(os.path.join(s_out_ref, "distances.txt"), "rb").read()

            if r_bytes != ref_bytes:
                print(f"[FAIL] Stress test {test_idx} failed! Seed: {seed}")
                print(f"Graph:\n{g_txt}")
                print(f"Queries:\n{q_txt}")
                print(f"Rust output:\n{r_bytes.decode('ascii')}")
                print(f"Ref output:\n{ref_bytes.decode('ascii')}")
                sys.exit(1)

        print("[PASS] 50/50 randomized stress tests passed!")
        results_table.append(("50 Random Stress Tests", "N/A (all byte-identical to reference)", "PASS"))

    print("\n=== Verification Summary ===")
    for name, h, status in results_table:
        print(f"{name:25} | {status:10} | {h}")


if __name__ == "__main__":
    main()
