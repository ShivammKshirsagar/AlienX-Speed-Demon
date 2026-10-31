#!/usr/bin/env python3
import pathlib, re, statistics, sys
root = pathlib.Path(sys.argv[1])
for label in sys.argv[2:]:
    rows = []
    files = sorted(root.glob(f"{label}-*.log"))
    for log_path in files:
        s = log_path.read_text()
        def number(pattern, cast=float):
            m = re.search(pattern, s, re.M)
            return cast(m.group(1)) if m else float('nan')
        m = re.search(r"Elapsed \(wall clock\) time \(h:mm:ss or m:ss\): ([^\n]+)", s)
        elapsed = m.group(1).split(':')
        wall = float(elapsed[-1]) + (int(elapsed[-2]) * 60 if len(elapsed) > 1 else 0) + (int(elapsed[-3]) * 3600 if len(elapsed) > 2 else 0)
        rows.append({"wall": wall, "user": number(r"User time \(seconds\): ([0-9.]+)"), "sys": number(r"System time \(seconds\): ([0-9.]+)"), "rss_kb": number(r"Maximum resident set size \(kbytes\): (\d+)", int), "major": number(r"Major \(requiring I/O\) page faults: (\d+)", int), "minor": number(r"Minor \(reclaiming a frame\) page faults: (\d+)", int), "parse_ms": number(r"parse_queries_ms=(\d+)", int), "pass_a_ms": number(r"pass_a_ms=(\d+)", int), "pass_b_ms": number(r"pass_b_ms=(\d+)", int), "query_ms": number(r"query_phase_ms=(\d+)", int), "output_ms": number(r"output_ms=(\d+)", int)})
    print(label, "median", {k: statistics.median(r[k] for r in rows) for k in rows[0]})