#!/usr/bin/env python3
import ast
import glob
import re
import statistics
import sys

work = sys.argv[1]
logs = sorted(glob.glob(f"{work}/current-*.log"))
records = []

def median(values):
    return statistics.median(values)

for path in logs:
    record = {"phases": {}, "resources": {}, "rss": {}, "workers": []}
    for line in open(path, encoding="utf-8"):
        if line.startswith("profile mmap_ns="):
            record["phases"] = {
                key: int(value)
                for key, value in re.findall(r"([a-z_]+)=([0-9]+)", line)
            }
        elif line.startswith("profile workers_started_finished_ns="):
            record["workers"] = ast.literal_eval(line.split("=", 1)[1])
        elif line.startswith("profile rusage "):
            fields = dict(re.findall(r"([a-z_]+)=([0-9]+)", line))
            name = re.search(r"phase=([a-z_]+)", line).group(1)
            record["resources"][name] = {key: int(value) for key, value in fields.items()}
        elif line.startswith("profile rss_current_bytes "):
            record["rss"] = {key: int(value) for key, value in re.findall(r"([a-z_]+)=([0-9]+)", line)}
        elif line.startswith("profile lifecycle "):
            record["lifecycle"] = {key: int(value) for key, value in re.findall(r"([a-z_]+)=([0-9]+)", line)}
        elif line.startswith("wrapper lifecycle "):
            record["wrapper"] = {key: int(value) for key, value in re.findall(r"([a-z_]+)=([0-9]+)", line)}
        elif line.startswith("profile rss_peak_bytes="):
            record["rss_peak"] = int(line.split("=", 1)[1])
    records.append(record)

if len(records) != 10:
    raise SystemExit(f"expected 10 profile logs, found {len(records)}")

phase_specs = [
    ("process start", "startup_ns", "process_start"),
    ("input map", "mmap_ns", "mmap"),
    ("boundary planning", "boundary_ns", "boundary"),
    ("parse (includes spawn)", "parse_wall_ns", "parse"),
    ("merge", "merge_ns", "merge"),
    ("scatter", "scatter_ns", None),
    ("selection", "selection_ns", "selection"),
    ("endpoint/output sort", "endpoint_sort_ns", "endpoint_sort"),
    ("output formatting", "output_format_ns", "output_format"),
    ("file write", "write_ns", "write"),
    ("process exit", "exit_ns", "process_exit"),
]

print("Phase medians (10 warm-cache WSL runs; time in ms)")
print("phase\twall_ms\tuser_ms\tsystem_ms\tminor_faults\tmajor_faults\tcumulative_max_rss_kb")
for label, phase_key, resource_key in phase_specs:
    if phase_key in ("startup_ns", "exit_ns"):
        walls = [r["wrapper"][phase_key] for r in records]
    else:
        walls = [r["phases"][phase_key] for r in records]
    wall = median(walls) / 1e6
    if resource_key:
        samples = [r["resources"][resource_key] for r in records]
        user = median([s["user_us"] for s in samples]) / 1000
        system = median([s["system_us"] for s in samples]) / 1000
        minor = median([s["minor_faults"] for s in samples])
        major = median([s["major_faults"] for s in samples])
        hwm = median([s["cumulative_hwm_kb"] for s in samples])
        print(f"{label}\t{wall:.3f}\t{user:.3f}\t{system:.3f}\t{minor:g}\t{major:g}\t{hwm:g}")
    else:
        print(f"{label}\t{wall:.3f}\t—\t—\t—\t—\t—")

users = [sum(x["user_us"] for x in r["resources"].values()) / 1e6 for r in records]
systems = [sum(x["system_us"] for x in r["resources"].values()) / 1e6 for r in records]
minor_faults = [sum(x["minor_faults"] for x in r["resources"].values()) for r in records]
major_faults = [sum(x["major_faults"] for x in r["resources"].values()) for r in records]
maxrss = [r["rss_peak"] / 1024 for r in records]
elapsed = [r["wrapper"]["elapsed_ns"] / 1e9 for r in records]
print(
    "process totals median: "
    f"wall={median(elapsed):.6f}s, profiled user={median(users):.6f}s, "
    f"profiled system={median(systems):.6f}s, minor/major faults="
    f"{median(minor_faults):g}/{median(major_faults):g}, max RSS={median(maxrss):.1f} KiB"
)

worker_counts = [len(r["workers"]) for r in records]
under = []
for r in records:
    parse_ns = r["phases"]["parse_wall_ns"]
    events = []
    for start, finish in r["workers"]:
        events.extend(((start, 1), (finish, -1)))
    events.sort()
    active = 0
    previous = 0
    under_ns = 0
    for at, change in events:
        if active < max(worker_counts):
            under_ns += at - previous
        active += change
        previous = at
    if active < max(worker_counts):
        under_ns += parse_ns - previous
    under.append(under_ns / parse_ns)
print(
    f"parser intervals with fewer than {max(worker_counts)} workers active: "
    f"median {100 * median(under):.1f}% of parse wall"
)
print("worker\tmedian start_ms\tmedian finish_ms")
for worker in range(max(worker_counts)):
    starts = [r["workers"][worker][0] / 1e6 for r in records]
    finishes = [r["workers"][worker][1] / 1e6 for r in records]
    print(f"{worker + 1}\t{median(starts):.3f}\t{median(finishes):.3f}")
