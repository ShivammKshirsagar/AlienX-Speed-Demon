# Track A performance log

## Earlier local log (superseded)

This is the log that was in `PERF.md` before the Linux profiling pass. Its timings and K estimate are retained for history only; they were collected on Windows and are not comparable to the board or to the later Linux-container measurements below.

| Step | Change | Board run ID | T (s) | M (MB) | Result |
|---|---|---|---:|---:|---|
| 0 | Organiser Python reference, unchanged | run-30c2dd2e0904 | 51.906 | 198.9 | Full seed 1 hash matched |
| 1 | Rust: read-only mapping, parallel parsing, local endpoint interning, parallel group sorting, one deterministic output pass | run-30c2dd2e0904 | 0.427 median of 5 | 275.6 median of 5 | Kept; all five full seed 1 hashes matched |

The earlier local Rust runs were 0.398, 0.413, 0.427, 0.432, and 0.466 seconds. Each wrote SHA-256 `495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3`.

Earlier correctness checks recorded: the worked example in `specs/a-ingest.md` passed; seed 2 full output matched `fe3469ff108066bd7b65f8239bbe27615467b99db182b79919c572c3d2910127`; seed 1 full output matched the hash above. Seeds 2, 3, and 7 sample hashes are retained in the current correctness section below.

The old K estimate used `T=0.427 s`, `M=275.6 MB` Windows working-set proxy, `S=303`, and `D=1`, giving the then-reported indicative `K≈6.48` (about 40.6 points). That estimate is superseded because the Windows timing and memory proxy were not board-equivalent and the earlier line-count method was not the final organizer-style count.

The earlier note said the board-equivalent `check.py` run and offline Docker build were unverified at that time. The subsequent Linux-container check and offline build results are recorded in the current sections below.

## Current WSL2 result

These are the latest measurements and supersede the Docker Desktop numbers below. The input and staged Rust projects were copied to Ubuntu WSL2's ext4 filesystem. The 238,836,234-byte seed 1 input was read once to warm the page cache. Both variants were built with WSL Cargo and run with `taskset -c 0-11`; ten baseline/candidate pairs were alternated. `/usr/bin/time` recorded wall, user, system, major/minor faults, and max RSS. All twenty full seed 1 outputs matched the expected SHA-256. The WSL distribution reports an ext4 root filesystem. Board run ID: TBD.

| Linux binary | Wall median (s) | User median (s) | System median (s) | Major faults median | Minor faults median | Max RSS median (KB) |
|---|---:|---:|---:|---:|---:|---:|
| Preserved baseline mmap | 0.305 | 1.355 | 0.245 | 0 | 21,550.5 | 286,378 |
| Linux-only candidate mmap | 0.290 | 1.345 | 0.245 | 0 | 21,430.5 | 287,424 |

The 15 ms wall-time difference is within run-to-run noise; user and system time and RSS are effectively unchanged. The previous `pread` candidate was also tested here and was slower on warmed ext4, so it was removed from the shipped source. The mmap profile's parse median was 119.911 ms versus 173.821 ms for pread. The pread profile also had higher system CPU and only saved RSS by avoiding resident mapped file pages; those pages are excluded from the board's stated memory score. The retained source is the simpler Linux mmap parser.

| Attempt | Board run ID | Measurement | Decision |
|---|---|---|---|
| Remove Windows implementation and retain one Linux mmap path | TBD | Source count fell to 313 Rust code lines plus 7 Dockerfile lines; ten full WSL run medians are shown above | Kept; no platform fallback remains |
| Replace mmap parsing with 1 MiB buffered pread | TBD | Ten instrumented runs: parse median 119.911 ms mmap vs 173.821 ms pread; profiled total 267.123 ms vs 304.702 ms; user 0.920 s vs 1.195 s; system 0.135 s vs 0.625 s; max RSS 287,550 KB vs 54,942 KB | Reverted; slower with warm ext4 cache, and the RSS reduction is mostly mapped input pages that do not count on the board |

### WSL phase profile

Each tools-only profile variant was run ten times, with hashes checked. Resource usage is measured with `getrusage(RUSAGE_SELF)` at phase boundaries; max RSS is the cumulative high-water mark at each boundary, and `VmRSS` snapshots are also recorded. `parse wall` includes worker creation and parsing. The profiler's instrumented timings are separate from the uninstrumented table above.

| Phase | mmap wall (ms) | mmap user (ms) | mmap system (ms) | mmap minor / major faults | mmap cumulative max RSS (KB) | pread wall (ms) | pread user (ms) | pread system (ms) | pread minor / major faults | pread cumulative max RSS (KB) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| mmap setup | 0.010 | 0.028 | 0.000 | 1 / 0 | 2,304 | 0.009 | 0.029 | 0.000 | 1 / 0 | 2,304 |
| Boundary planning | 0.072 | 0.111 | 0.000 | 12 / 0 | 2,944 | 0.074 | 0.098 | 0.000 | 12 / 0 | 2,944 |
| Parse, including spawn | 119.911 | 825.924 | 91.376 | 15,641.5 / 0 | 272,594 | 173.821 | 1,080.561 | 589.270 | 14,827.5 / 0 | 48,440 |
| Merge | 63.574 | 49.033 | 10.324 | 3,300 / 0 | 279,640 | 66.439 | 47.325 | 7.367 | 3,303.5 / 0 | 48,908 |
| Selection | 29.274 | 38.460 | 0.958 | 0 / 0 | 279,640 | 27.031 | 43.330 | 0.000 | 0 / 0 | 48,908 |
| Endpoint sort | 0.349 | 0.486 | 0.018 | 0.5 / 0 | 279,640 | 0.341 | 0.417 | 0.000 | 0 / 0 | 48,908 |
| Output formatting | 24.469 | 19.235 | 3.937 | 2,470 / 0 | 287,550 | 27.446 | 22.558 | 10.120 | 2,470 / 0 | 54,942 |
| Write | 12.887 | 0.070 | 11.986 | 0 / 0 | 287,550 | 14.335 | 0.000 | 17.264 | 0 / 0 | 54,942 |

`/proc/self/status` RSS snapshots after parse / merge / selection / output / write were 266.1 / 270.8 / 270.8 / 280.3 / 270.8 MiB for mmap and 40.4 / 46.1 / 46.1 / 55.7 / 46.1 MiB for pread. The mmap RSS includes the resident input mapping; the scoring board says preloaded `/data` does not count. Major faults were zero during the warm-cache profiles. The current source performs mmap parsing; pread remains a measured, reverted tools-only comparison.

The largest WSL phase is parse, followed by merge. The warm-cache measurements do not support pread as a speed optimization. Total uninstrumented wall time is still far from the 0.05 s target.

The following Docker Desktop section is retained as historical context only. Its Windows bind mount distorted mmap timing and RSS, so it was not used to choose the current parser.

All measurements in the historical section below were collected in Docker Desktop's Linux VM on the Windows host, not on the scoring machine. Unless noted, each full-data run checked the output against seed 1 SHA-256 `495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3`. Local containers used 12 CPUs, an 8 GiB memory and swap limit, read-only root, a 1 GiB `/tmp` tmpfs, no network, and a read-only `/data` mount. Times below are child-process elapsed time; memory is the container cgroup peak.

## Prior Docker Desktop run (superseded)

The board-provided reference is commit `d8ab7d8`: `T=0.163 s`, `M=65 MB`, `S=394`, `D=1`, `K=13.44`, score `56.422`. These are organizer results supplied in the task brief, not measured here.

| Step | Change | Board run ID | Local T (s) | Local M (MB) | Result |
|---|---|---|---:|---:|---|
| 0 | Preserved mmap baseline snapshot | TBD | 1.54 median of 5 paired runs | 284.6 median of 5 | All five full seed 1 hashes matched |
| 1 | Experimental Unix input parsing used disjoint newline-aligned `pread` ranges and a reusable 1 MiB buffer per worker | TBD | 1.49 median of 5 paired runs; 1.31 median of 5 consecutive candidate runs | 56.2 median paired; 55.7 median consecutive | Reverted after warmed WSL ext4 profiles showed slower parsing |

The paired comparison alternated baseline and candidate to reduce VM drift:

| Pair | Baseline T / M | Candidate T / M |
|---:|---:|---:|
| 1 | 1.47 s / 283.6 MB | 1.54 s / 56.8 MB |
| 2 | 1.71 s / 284.7 MB | 1.49 s / 56.2 MB |
| 3 | 1.62 s / 284.7 MB | 1.52 s / 55.3 MB |
| 4 | 1.50 s / 284.6 MB | 1.28 s / 56.5 MB |
| 5 | 1.54 s / 284.0 MB | 1.32 s / 55.5 MB |

The sequential candidate-only timing series was 1.21, 1.36, 1.31, 1.23, and 1.48 s. It measured 56.1, 55.7, 55.6, 55.9, and 55.4 MB respectively. All five outputs had the expected hash.

## Prior Docker Desktop phase profile (superseded)

The tools-only instrumented baseline and experimental candidate were profiled on the full seed 1 input. These profiler timings are retained as history and are not used to choose the current parser. There is no separate scatter phase: endpoint latency values are appended directly while parsing.

| Phase / observation | mmap baseline | pread candidate |
|---|---:|---:|
| Parse, median of 5 | 1.109 s | 0.926 s |
| Merge, median | about 70–86 ms | about 66 ms |
| Selection, median | about 32 ms | about 34 ms |
| Endpoint sort | about 0.5–1.7 ms | included in the same post-parse pipeline |
| Output formatting | about 36–45 ms | about 46 ms |
| Output write | about 150–170 ms | about 157 ms |
| Instrumented main total, median | about 1.50 s | about 1.253 s |
| Major faults after parsing | about 3,907 | about 18 |
| Cgroup memory after parsing / after output | about 281 / 297 MB | about 42 / 58 MB |

The baseline page-fault cost was the largest observed bottleneck. Its mmap parse phase took about 1.1 s and incurred roughly 3,907 major faults. The `pread` version reduced the profiled parse median to about 0.93 s and major faults to about 18, while greatly reducing charged file-page memory. Thread completion was reasonably balanced: the pread workers generally finished within roughly 10–25 ms of one another in the best profiles. Mmap workers had a wider roughly 50 ms finish spread. Thread start times were within a few milliseconds. The 1 MiB pread buffer is reused per worker.

Thread-count comparisons, each median of five full profiler runs, all with matching hashes:

| Input path | 6 threads parse | 10 threads parse | 12 threads parse | Choice |
|---|---:|---:|---:|---|
| mmap | 1.299 s | 1.277 s | 1.109 s | 12 |
| pread | 1.306 s | 0.990 s | 0.926 s | 12 |

Pread peak memory was about 48.5 MB at 6 threads, 57.7 MB at 10, and 58.3 MB at 12. Twelve threads gave the fastest parse and the production candidate retains that setting.

## Prior Docker profiler experiments (historical)

Every item below was tested in the tools-side profiler copy, not retained in the shipped source. Each experiment used five full seed 1 runs and each output hash matched. The comparison baseline for the profiling experiments was mmap: about 1.50 s instrumented main total and 1.109 s parse median. Board run ID: TBD.

| Experiment | Measurement | Decision |
|---|---|---|
| `MAP_POPULATE` | mmap phase median about 3.5–3.9 s; parse about 0.22 s; total about 4.17 s; peak about 296 MB | Reverted; pre-faulting moved the cost earlier and made total slower |
| `MADV_SEQUENTIAL` | total about 1.45 s versus about 1.50 s; major faults remained about 3,896 versus 3,907 | Reverted; small difference was within VM noise and faults barely changed |
| `MADV_WILLNEED` | total median about 1.668 s | Reverted; slower |
| 4 MiB pread chunks | parse median about 0.994 s; total about 1.35 s; peak about 91 MB | Reverted; slower and more memory |
| 256 KiB pread chunks | parse median about 1.188 s; total about 1.516 s; peak about 58 MB | Reverted; slower |
| SWAR delimiter scan | pread parse median about 0.956 s versus 0.926 s for byte scan | Reverted; no improvement |
| AVX2 delimiter scan | pread parse median about 0.988 s; total about 1.347 s | Reverted; slower |

## Correctness

The current Linux-only mmap candidate produced the expected full seed 1 SHA-256 `495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3` on all ten WSL runs. Its Linux binary passed byte-for-byte comparisons with the Python-reference outputs for sample seeds 2, 3, and 7:

| Dataset | SHA-256 | Result |
|---|---|---|
| Seed 2 sample | `fd26778fa5e70fac2905a0a0f0142bd26ca4e6f510dfe182a881e525286ff12f` | Rust matched Python reference |
| Seed 3 sample | `db28ff42849be9f28fcc7cbac23b6c292e2c0810c054056a2bdbffe6f7321dac` | Rust matched Python reference |
| Seed 7 sample | `39bfccc5334c93cbe0c762f5e6c3300e91ca2e266567561da3176c038cee49f7` | Rust matched Python reference |

The expected hashes are the Python reference outputs generated for those samples. All checks used the Linux WSL binary, not the Windows executable. Rust formatting was checked with rustfmt defaults; `cargo fmt --check` passes.

## Source size and indicative K

The formatted Linux-only Rust source has 313 code lines by `scc`; the Dockerfile contributes 7 lines. The organizer-style count for Rust plus Dockerfile is `S=320`, down from the original 394. Comments and blank lines are not counted. `D=1` because the Rust project has no direct non-std dependencies.

Using the board's supplied time and memory unchanged, the reduction from `S=394` to `S=320` alone scales its K by `(394/320)^0.15`, or about 1.032. That gives a size-only indicative `K≈13.87`; no timing or memory improvement is claimed. The WSL wall medians are not precise enough to estimate a reliable board-time change, and WSL RSS charges mapped input pages differently from the board.

This is not an official board result. The benchmark host is WSL2 rather than the scoring host, and the measured wall-time difference is within local run noise. The source-size reduction is the clear change; the 0.05 s wall-time target was not reached.
