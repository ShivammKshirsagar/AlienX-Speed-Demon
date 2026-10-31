import subprocess
import hashlib
from pathlib import Path
import sys

SAMPLE_DIR = Path("data/generated/sample-seed1").resolve()
FULL_DIR = Path("data/d").resolve()
OUT_DIR = Path(".out").resolve()
OUT_FILE = OUT_DIR / "distances.txt"
OUT_DIR.mkdir(exist_ok=True)

EXPECTED_SAMPLE = "a4c76ed030bc7e365db33191f6cf74708fba7f25eb48d64c8c4279f2e57046a0"
EXPECTED_FULL = "a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e"

def run_container(data_dir, cpuset=None):
    if OUT_FILE.exists():
        OUT_FILE.unlink()
    cmd = [
        "docker", "run", "--rm",
    ]
    if cpuset is not None:
        cmd.extend(["--cpuset-cpus", str(cpuset)])
    cmd.extend([
        "-v", f"{data_dir}:/data:ro",
        "-v", f"{OUT_DIR}:/out",
        "speeddemon-check/track-d:latest"
    ])
    res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if res.returncode != 0:
        print(f"Error running container: {res.stderr.decode()}")
        sys.exit(1)
    return hashlib.sha256(OUT_FILE.read_bytes()).hexdigest()

print("--- Testing Sample Dataset (20 runs) ---")
for i in range(1, 21):
    h = run_container(SAMPLE_DIR)
    assert h == EXPECTED_SAMPLE, f"Sample run {i} hash mismatch: {h} != {EXPECTED_SAMPLE}"
    print(f"Sample run {i:02d}: PASS ({h[:16]}...)")

print("\n--- Testing Full Dataset (20 runs) ---")
for i in range(1, 21):
    h = run_container(FULL_DIR)
    assert h == EXPECTED_FULL, f"Full run {i} hash mismatch: {h} != {EXPECTED_FULL}"
    print(f"Full run {i:02d}: PASS ({h[:16]}...)")

print("\n--- Testing Worker Counts (1, 4, 12 workers) on Sample ---")
for workers, cpuset in [(1, "0"), (4, "0-3"), (12, "0-11")]:
    h = run_container(SAMPLE_DIR, cpuset=cpuset)
    assert h == EXPECTED_SAMPLE, f"Sample {workers} workers mismatch: {h}"
    print(f"Sample @ {workers:2d} workers: PASS ({h})")

print("\n--- Testing Worker Counts (1, 4, 12 workers) on Full ---")
for workers, cpuset in [(1, "0"), (4, "0-3"), (12, "0-11")]:
    h = run_container(FULL_DIR, cpuset=cpuset)
    assert h == EXPECTED_FULL, f"Full {workers} workers mismatch: {h}"
    print(f"Full @ {workers:2d} workers: PASS ({h})")

print("\nAll 20 sample runs, 20 full runs, and 1/4/12 worker tests PASSED with identical expected hashes!")
