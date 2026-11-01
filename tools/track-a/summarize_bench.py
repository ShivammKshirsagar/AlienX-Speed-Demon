import csv
import math
from pathlib import Path
import statistics
import sys

rows = list(csv.DictReader(open(sys.argv[1], newline="", encoding="utf-8")))
root = Path(sys.argv[2])
labels = ["baseline", "n1", "n2", "n3"]
data = {}
for label in labels:
    selected = [r for r in rows if r["label"] == label and r["round"] != "warmup"]
    data[label] = {
        "cpu": [int(r["cpu_us"]) for r in selected],
        "wall": [int(r["wall_ns"]) for r in selected],
        "mem": [int(r["memory_peak_bytes"]) for r in selected],
    }

def loc(label):
    if label == "baseline":
        return 497
    folder = root / "tools" / "track-a" / "candidates" / label
    count = 0
    for path in (folder / "src" / "main.rs", folder / "Dockerfile"):
        for line in path.read_text(encoding="utf-8").splitlines():
            stripped = line.strip()
            if stripped and not stripped.startswith(("//", "#")):
                count += 1
    return count

base_cpu = min(data["baseline"]["cpu"])
base_mem = statistics.median(data["baseline"]["mem"])
base_s = 497
print("candidate | CPU us min/median | wall ms min/median | memory.peak MB median | S | est K rel.")
for label in labels:
    d = data[label]
    cpu_min, cpu_med = min(d["cpu"]), statistics.median(d["cpu"])
    wall_min, wall_med = min(d["wall"]), statistics.median(d["wall"])
    mem = statistics.median(d["mem"])
    s = loc(label)
    k = (base_cpu / cpu_min) ** 0.5 * (base_mem / mem) ** 0.2 * (base_s / s) ** 0.15
    print(f"{label} | {cpu_min}/{cpu_med:.0f} | {wall_min/1e6:.3f}/{wall_med/1e6:.3f} | {mem/1e6:.2f} | {s} | {k:.3f}")
