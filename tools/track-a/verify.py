"""Run local byte-for-byte checks against Track A's worked example and reference."""
import argparse
import hashlib
import pathlib
import subprocess
import tempfile

HARNESS = pathlib.Path(r"C:\GitHub Desktop\speed-demon-2026")
EXAMPLE = """2026-10-30T19:00:00.000000Z GET /v2/orders/{id} 200 1500
2026-10-30T18:59:59.999999Z GET /v2/orders/{id} 200 1841
2026-10-30T18:00:00.000000Z POST /v2/checkout 503 204117
2026-10-30T18:00:00.000000Z POST /v2/checkout 201 30000
2026-10-30T18:00:00.115332Z GET /health 200 62
2026-10-30T18:00:00.115332Z GET /health 200 62
2026-10-30T18:42:17.000001Z HEAD /health 200 4294967295
2026-10-30T18:30:00.000001Z OPTIONS / 200 0
2026-10-30T18:12:00.500000Z POST /v2/checkout 500 0
2026-10-30T18:12:00.600000Z POST /v2/checkout 499 1500
2026-10-30T18:12:00.700000Z POST /v2/checkout 599 2500
2026-10-30T18:20:00.000000Z GET /v2/orders/{id} 500 900
2026-10-30T18:21:00.000000Z DELETE /v2/orders/{id} 404 77
2026-10-30T19:05:00.000000Z GET /v2/orders_legacy 200 60
2026-10-30T19:05:00.000001Z POST /v2/orders-export 202 20
2026-10-30T19:05:00.000002Z GET /v2/orders 200 10
2026-10-31T23:59:59.999999Z POST /v2/checkout 502 7
2026-11-01T00:00:00.000000Z POST /v2/checkout 503 8
"""
EXPECTED = """2026-10-30T18:00:00Z / 1 0 0 0 0 0
2026-10-30T18:00:00Z /health 3 62 4294967295 4294967295 0 0
2026-10-30T18:00:00Z /v2/checkout 5 2500 204117 204117 3 600000
2026-10-30T18:00:00Z /v2/orders/{id} 3 900 1841 1841 1 333333
2026-10-30T19:00:00Z /v2/orders 1 10 10 10 0 0
2026-10-30T19:00:00Z /v2/orders-export 1 20 20 20 0 0
2026-10-30T19:00:00Z /v2/orders/{id} 1 1500 1500 1500 0 0
2026-10-30T19:00:00Z /v2/orders_legacy 1 60 60 60 0 0
2026-10-31T23:00:00Z /v2/checkout 1 7 7 7 1 1000000
2026-11-01T00:00:00Z /v2/checkout 1 8 8 8 1 1000000
"""


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("solution", type=pathlib.Path)
    ap.add_argument("datasets", nargs="*", type=pathlib.Path,
                    help="dataset directories to compare with the Python reference")
    args = ap.parse_args()
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        data, out = root / "data", root / "out"
        data.mkdir()
        (data / "requests.log").write_text(EXAMPLE, encoding="ascii", newline="")
        subprocess.run([args.solution, data / "requests.log", out / "summary.txt"], check=True)
        actual = (out / "summary.txt").read_text(encoding="ascii")
        assert actual == EXPECTED, "worked example differs"
        print("worked example: PASS")

        empty = root / "empty.log"
        empty.write_bytes(b"")
        empty_out = root / "empty-summary.txt"
        subprocess.run([args.solution, empty, empty_out], check=True)
        assert empty_out.read_bytes() == b"", "empty input must produce empty output"
        print("empty input: PASS")

        for dataset in args.datasets:
            rust_out = root / (dataset.name + "-rust.txt")
            ref_out = root / (dataset.name + "-reference")
            subprocess.run([args.solution, dataset / "requests.log", rust_out], check=True)
            subprocess.run(["python", HARNESS / "reference" / "a_ingest.py",
                            "--data", dataset, "--out", ref_out], check=True)
            rust_hash = digest(rust_out)
            ref_hash = digest(ref_out / "summary.txt")
            print(f"{dataset}: Rust {rust_hash} Python {ref_hash}")
            assert rust_hash == ref_hash, f"reference mismatch: {dataset}"
    print("all checks: PASS")


if __name__ == "__main__":
    main()
