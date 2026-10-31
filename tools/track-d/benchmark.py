import subprocess
import time
import os

cmd = [
    "docker", "run", "--rm",
    "--network=none",
    "-v", f"{os.path.abspath('data/d')}:/data:ro",
    "-v", f"{os.path.abspath('out')}:/out",
    "--entrypoint", "/bin/sh",
    "speeddemon-check/track-d",
    "-c", "/app/solution && cat /sys/fs/cgroup/memory.peak"
]

# warm run
print("Warming page cache...")
subprocess.run(cmd, capture_output=True, text=True)

times = []
peaks = []
for i in range(5):
    t0 = time.perf_counter()
    res = subprocess.run(cmd, capture_output=True, text=True)
    t = time.perf_counter() - t0
    peak = int(res.stdout.strip())
    times.append(t)
    peaks.append(peak)
    print(f"Run {i+1}: {t:.3f} s, peak: {peak / (1024*1024):.2f} MB")

times.sort()
peaks.sort()
print(f"Median time: {times[2]:.3f} s (including docker run overhead)")
print(f"Median peak: {peaks[2] / (1024*1024):.2f} MB ({peaks[2]} bytes)")
