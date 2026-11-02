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

These are the latest measurements and supersede the Docker Desktop numbers below. The input and staged Rust projects were copied to Ubuntu WSL2's ext4 filesystem. The 238,836,234-byte seed 1 input was read once to warm the page cache. Both variants were built with WSL Cargo and run with `taskset -c 0-11`; ten baseline/candidate pairs were alternated. `/usr/bin/time` recorded wall, user, system, major/minor faults, and max RSS. All twenty full seed 1 outputs matched the expected SHA-256. The WSL distribution reports an ext4 root filesystem. Board run ID: run-6caaf1c7e57b.

| Linux binary | Wall median (s) | User median (s) | System median (s) | Major faults median | Minor faults median | Max RSS median (KB) |
|---|---:|---:|---:|---:|---:|---:|
| Preserved baseline mmap | 0.305 | 1.355 | 0.245 | 0 | 21,550.5 | 286,378 |
| Linux-only candidate mmap | 0.290 | 1.345 | 0.245 | 0 | 21,430.5 | 287,424 |

The 15 ms wall-time difference is within run-to-run noise; user and system time and RSS are effectively unchanged. The previous `pread` candidate was also tested here and was slower on warmed ext4, so it was removed from the shipped source. The mmap profile's parse median was 119.911 ms versus 173.821 ms for pread. The pread profile also had higher system CPU and only saved RSS by avoiding resident mapped file pages; those pages are excluded from the board's stated memory score. The retained source is the simpler Linux mmap parser.

| Attempt | Board run ID | Measurement | Decision |
|---|---|---|---|
| Remove Windows implementation and retain one Linux mmap path | run-6caaf1c7e57b | Source count fell to 313 Rust code lines plus 7 Dockerfile lines; ten full WSL run medians are shown above | Kept; no platform fallback remains |
| Replace mmap parsing with 1 MiB buffered pread |run-6caaf1c7e57b | Ten instrumented runs: parse median 119.911 ms mmap vs 173.821 ms pread; profiled total 267.123 ms vs 304.702 ms; user 0.920 s vs 1.195 s; system 0.135 s vs 0.625 s; max RSS 287,550 KB vs 54,942 KB | Reverted; slower with warm ext4 cache, and the RSS reduction is mostly mapped input pages that do not count on the board |

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

## Goal run: step 0 current-code profile

WSL reported 16 logical CPUs and affinity `0-15`; this run used `min(16,12)=12` workers pinned to `0-11`. The 238,836,234-byte input was copied to `/data` on ext4 and read once before timing. The current mmap implementation was profiled ten times; each profile output matched the seed 1 hash. This is the single current-code phase table for the goal run. Board run ID: run-6caaf1c7e57b.

| Phase | Wall median (ms) | User median (ms) | System median (ms) | Minor / major faults | Cumulative max RSS (KB) |
|---|---:|---:|---:|---:|---:|
| Process startup | 1.028 | 0.805 | 0.000 | 80 / 0 | 2,432 |
| Input mmap | 0.009 | 0.039 | 0.000 | 1 / 0 | 2,432 |
| Boundary planning | 0.069 | 0.084 | 0.089 | 12.5 / 0 | 3,008 |
| Parse, including worker spawn | 105.821 | 870.154 | 105.430 | 15,534 / 0 | 272,496 |
| Merge | 44.595 | 34.671 | 10.109 | 3,301.5 / 0 | 279,468 |
| Scatter | 0.000 | — | — | — | — |
| Selection | 31.469 | 45.332 | 1.397 | 0 / 0 | 279,468 |
| Endpoint/output sort | 0.448 | 0.593 | 0.028 | 0.5 / 0 | 279,468 |
| Output formatting | 25.560 | 21.439 | 4.109 | 2,470 / 0 | 287,826 |
| File write | 16.118 | 0.073 | 15.789 | 0 / 0 | 287,826 |
| Process exit | 4.034 | 0.034 | 6.651 | 0.5 / 0 | 287,826 |

The outer child-lifecycle median was 0.246526 s; summed phase CPU medians were 0.971160 s user and 0.153390 s system, with 21,404.5 minor and zero major faults and 287,826 KiB max RSS. Phase medians do not sum exactly to outer wall because phase sampling and log capture add overhead.

During parsing, fewer than all 12 workers were active for a median 33.3% of parse wall. Median worker finish offsets from parse start (ms), in spawn order, were 92.965, 104.622, 83.277, 94.042, 86.592, 95.583, 92.617, 85.972, 88.427, 93.588, 89.429, and 91.924. Parse and merge are the largest costs; begin Step 1A with merge parallelization.

### Step 1A attempt 1: parallel group merge (reverted)

Board run ID: run-6caaf1c7e57b. Ten uninstrumented candidate runs had median wall 0.260 s, user 1.400 s, system 0.220 s, zero median major faults, 21,940.5 minor faults, and 289,096 KB max RSS. Compared with the baseline snapshot's 0.305 s median, the nominal wall improvement was 14.8%; all ten seed 1 hashes and all three sample hashes matched. However, the ten-run phase profile showed merge at 47.076 ms versus 44.595 ms before, and instrumented total at 0.309 s versus 0.247 s. Other phases also moved substantially under WSL load. The apparent wall gain was not corroborated by phase measurements, so the parallel-bucket merge was reverted rather than retained.

### Step 1A attempt 2: parallel output formatting and positioned writes (kept)

Board run ID:run-6caaf1c7e57b. Contiguous hour ranges are assigned using estimated line-byte weights. Each worker formats into its own buffer; the output file is sized once and worker buffers are written at prefix-summed offsets with `write_all_at`. Ten-run uninstrumented median moved from 0.290 s to 0.245 s (15.5% faster); candidate user/system time was 1.340 / 0.245 s, major/minor faults 0 / 19,599, max RSS 282,156 KB. All ten full seed 1 hashes and seeds 2, 3, and 7 sample-reference comparisons passed.

| Phase | Before median (ms) | After median (ms) |
|---|---:|---:|
| Parse | 105.821 | 105.685 |
| Merge | 44.595 | 44.522 |
| Selection | 31.469 | 33.547 |
| Output formatting | 25.560 | 6.509 |
| File write | 16.118 | 16.520 |
| Instrumented process wall | 246.526 | 231.074 |

Formatting was the confirmed gain; parallel positioned writes did not reduce the write phase. Parse is still only about 46% of instrumented wall, so continue Step 1A with selection balance and merge costs.

### Step 1A attempt 3: weighted contiguous selection ranges (kept)

Board run ID: run-6caaf1c7e57b. Group ranges are partitioned by cumulative latency-record count, a proxy for sort work, instead of giving each worker the same number of groups. Ten-run uninstrumented median was 0.240 s versus 0.245 s before (2.0% faster), with user/system 1.285 / 0.265 s, major/minor faults 0 / 19,859.5, and max RSS 281,844 KB. The phase median for selection fell from 33.547 ms to 11.871 ms; instrumented wall fell from 231.074 ms to 215.207 ms. All ten full hashes and sample seeds 2, 3, and 7 passed.

| Phase | Before median (ms) | After median (ms) |
|---|---:|---:|
| Parse | 105.685 | 105.148 |
| Merge | 44.522 | 52.798 |
| Selection | 33.547 | 11.871 |
| Output formatting | 6.509 | 9.628 |
| File write | 16.520 | 15.502 |
| Instrumented process wall | 231.074 | 215.207 |

Selection work is no longer a top cost. Merge is now the largest phase and is the next Step 1A target.

### Step 1A attempt 4: hour-major parallel merge (kept)

Board run ID: run-6caaf1c7e57b. Parser-local groups are stored by hour, then merged in parallel across contiguous hour ranges weighted by record count. Ten uninstrumented WSL runs had median wall time 0.180 s, user 1.100 s, system 0.195 s, zero major faults, 17,341.5 minor faults, and 279,180 KB max RSS. The preceding attempt's median was 0.240 s, a 25.0% reduction. All ten full seed 1 outputs matched SHA-256 `495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3`; seeds 2, 3, and 7 sample comparisons passed.

| Phase | Before median (ms) | After median (ms) |
|---|---:|---:|
| Parse | 105.148 | 86.511 |
| Merge | 52.798 | 29.493 |
| Selection | 11.871 | 11.916 |
| Output formatting | 9.628 | 6.999 |
| File write | 15.502 | 18.198 |
| Instrumented process wall | 215.207 | 166.321 |

The merge phase is substantially lower, while parse remains the largest phase. Continue Step 1A until parse dominates the remaining measured work, then proceed to parser tuning.

### Step 1B attempt 1: AVX2 delimiter scanning (kept)

Board run ID: run-6caaf1c7e57b. The Linux parser now uses runtime-checked AVX2 to find line endings and field separators, with a scalar tail and scalar fallback. Ten uninstrumented runs measured 0.155 s median wall time, 0.725 s user, 0.220 s system, zero major faults, 17,348.5 minor faults, and 279,364 KB max RSS. This is 13.9% faster than attempt 4's 0.180 s median. All ten seed 1 outputs matched the expected hash, and samples 2, 3, and 7 matched their Python references.

| Phase | Before median (ms) | After median (ms) |
|---|---:|---:|
| Parse | 86.511 | 80.748 |
| Merge | 29.493 | 26.709 |
| Selection | 11.916 | 9.062 |
| Output formatting | 6.999 | 5.166 |
| File write | 18.198 | 11.891 |
| Instrumented process wall | 166.321 | 147.018 |

AVX2 delimiter scanning reduced parse time by 6.7%; the instrumented total fell 11.6%. The phase profiler and production parser now use the same delimiter scan. Continue with one parser change at a time.

### Step 1B attempt 2: three rank selections (reverted)

Board run ID:run-6caaf1c7e57b. Replaced full group sorting with three exact `select_nth_unstable` rank selections. Ten direct runs measured the same 0.155 s median wall time as attempt 1, with user/system 0.760 / 0.225 s; all ten full hashes matched. The profiler selection phase improved from 9.062 to 6.491 ms, but instrumented total moved from 147.018 to 157.679 ms as parse and merge also rose. With no measured end-to-end improvement, the change was reverted.

### Step 1B attempt 3: arithmetic day index (reverted)

Board run ID:run-6caaf1c7e57b. Replaced the date match with a shorter arithmetic index. The first ten-run series measured 0.145 s versus 0.155 s, but its phase profile was slower overall (0.163 s versus 0.147 s) and parse increased from 80.748 to 87.360 ms. A repeat ten-run series measured 0.155 s. The direct gain did not repeat, so the date match was restored.

### Step 1B attempt 4: initial capacity for local latency vectors (reverted)

Board run ID: run-6caaf1c7e57b. Reserved eight values for each parser-local group. Direct median was 0.145 s versus 0.155 s, but this is a single 10 ms timer bucket difference. Phase parse was 81.137 ms versus 80.748 ms, instrumented total was 160.487 ms versus 147.018 ms, and peak RSS rose from 279,248 to 280,506 KiB. The apparent direct improvement was not corroborated and the capacity reservation was reverted.

### Step 1B attempt 5: chunked endpoint hashing (kept)

Board run ID:run-6caaf1c7e57b. The deterministic hasher now mixes endpoint data in 64-bit chunks, with a padded tail, rather than processing every byte separately. Ten direct runs measured 0.120 s median wall time, 0.580 s user, 0.180 s system, zero major faults, 17,297.5 minor faults, and 279,290 KB max RSS. Relative to attempt 1, direct wall time fell 22.6%. All ten full seed 1 hashes matched, as did sample seeds 2, 3, and 7.

| Phase | Before median (ms) | After median (ms) |
|---|---:|---:|
| Parse | 80.748 | 62.974 |
| Merge | 26.709 | 29.896 |
| Selection | 9.062 | 9.435 |
| Output formatting | 5.166 | 7.845 |
| File write | 11.891 | 11.503 |
| Instrumented process wall | 147.018 | 139.920 |

The parser gain is substantial; later phases moved within WSL run variation. The 64-bit chunked hash remains deterministic and preserves byte-key equality checks in `HashMap`.

### Step 1A attempt 5: one write_vectored call (reverted)

Board run ID: run-6caaf1c7e57b. Replaced concurrent `write_all_at` calls with a `write_vectored` loop over the formatted buffers. Ten direct runs measured the same 0.120 s median; user/system were 0.545 / 0.125 s. The phase table showed write at 11.366 ms versus 11.503 ms before, while instrumented total rose from 139.920 to 145.318 ms under run variation. With no measurable wall or write-phase gain, concurrent positioned writes were restored.

### Step 1A attempt 6: pre-size merged latency vectors (reverted)

Board run ID:run-6caaf1c7e57b. Counted each destination group's records before merging and reserved exact vector capacity. Ten direct runs remained at 0.120 s median, while the merge phase rose from 29.896 to 32.208 ms and instrumented total rose to 158.780 ms. Peak RSS increased from 279,374 to 286,502 KiB. The extra pass and reservation did not reduce merge time, so the original append path was restored.

### Step 1A attempt 7: move the first partial vector (kept)

Board run ID:run-6caaf1c7e57b. When a destination group has no latencies yet, move the first local vector into it; append later partials as before. Ten direct runs measured 0.110 s median wall, 0.545 s user, 0.150 s system, zero major faults, 17,105 minor faults, and 277,312 KB max RSS. Compared with attempt 5, wall fell 8.3%. All ten full seed 1 hashes and sample seeds 2, 3, and 7 passed.

| Phase | Before median (ms) | After median (ms) |
|---|---:|---:|
| Parse | 62.974 | 68.354 |
| Merge | 29.896 | 19.595 |
| Selection | 9.435 | 6.357 |
| Output formatting | 7.845 | 6.241 |
| File write | 11.503 | 13.616 |
| Instrumented process wall | 139.920 | 137.058 |

Merge and selection together fell by 13.4 ms; direct wall improved. This is the current A-stage candidate.

### Step 1A attempt 8: shared-mmap output copy (reverted)

Board run ID: run-6caaf1c7e57b. Mapped the pre-sized output file with `MAP_SHARED` and copied worker buffers into disjoint ranges. The first build attempt showed that the descriptor must also be readable; after correcting the open flags, ten runs measured 0.120 s median versus 0.110 s for attempt 7, with 92 median major faults, 19,480 minor faults, and 286,942 KB max RSS. The additional output faults, slower wall time, and higher RSS made this a regression; positioned writes were restored.

### Step 1B attempt 6: high-bit table indexing (reverted)

Board run ID: run-6caaf1c7e57b. Changed the open-addressing table's initial slot from the low hash bits to the high 32 bits. Ten direct runs measured 0.150 s versus 0.120 s for the low-bit table, so low-bit indexing was restored.

### Step 1B attempt 7: SWAR latency conversion (reverted)

Board run ID: run-6caaf1c7e57b. Parsed up to eight latency digits with packed 64-bit arithmetic and retained the scalar fallback for longer fields. Ten direct runs measured 0.125 s versus 0.120 s; parse phase was 75.065 ms versus 74.950 ms. The expected seed 1 hash matched in all ten runs, but the change did not reduce parse time and was reverted.

### Step 1B attempt 8: open-addressing endpoint table (reverted)

Board run ID:run-6caaf1c7e57b. Replaced per-local endpoint `HashMap` and endpoint byte copies with an 8,192-slot open-addressing table storing hash, input offset, length, and endpoint ID; slots grow only when full. Added 16-byte SSE2 equality checks. The first ten direct runs measured 0.120 s versus 0.110 s before; a repeat measured 0.130 s. In the first phase profile, parse rose from 68.354 to 75.419 ms; merge fell from 19.595 to 17.755 ms and peak RSS fell from 277,444 to 274,952 KiB, while instrumented total rose from 137.058 to 143.580 ms. All full seed 1 hash checks passed, but the time regression outweighed the memory reduction, so the `HashMap` endpoint index was restored.

### Step 1B attempt 9: borrowed endpoint slices (kept)

Board run ID: run-6caaf1c7e57b. Local and global endpoint maps now store slices borrowed from the input mapping instead of allocating and copying endpoint bytes. The first direct ten-run median was 0.115 s; the repeat was 0.110 s, matching attempt 7. The phase profile measured parse at 67.430 ms, merge at 15.235 ms, and instrumented wall at 130.769 ms versus 68.354 / 19.595 / 137.058 ms before. Peak RSS fell from 277,444 to 276,110 KiB. All ten full seed 1 hashes and samples 2, 3, and 7 passed.

### Step 1B attempt 10: prehashed endpoint map keys (reverted)

Board run ID:run-6caaf1c7e57b. Wrapped each borrowed endpoint with a cached hash so `HashMap` would hash a `u64` key and compare bytes on matches. Ten direct runs measured 0.130 s versus 0.110 s for borrowed slices. The extra wrapper hashing was slower, so plain borrowed byte-slice keys were restored; all ten seed 1 hashes passed.

### Step 1B attempt 11: compile-time AVX2 dispatch (kept)

Board run ID: run-6caaf1c7e57b. Set `RUSTFLAGS="-C target-cpu=x86-64-v3"` in the Docker build and WSL benchmark builds, then selected the AVX2 delimiter scanner at compile time instead of checking the CPU feature on every delimiter. Ten direct runs remained at 0.110 s median; parse fell from 67.430 to 63.738 ms and the median interval with fewer than 12 parser workers fell from 43.6% to 41.6%. All ten full seed 1 hashes and sample seeds 2, 3, and 7 passed. The Dockerfile's pinned base image lines are unchanged.

### Step 1B attempt 12: fused delimiter hashing and endpoint table (reverted)

Board run ID: run-6caaf1c7e57b. Combined AVX2 endpoint-space scanning with chunk hashing and open-addressing lookup. Ten direct runs measured 0.145 s versus 0.110 s for the borrowed `HashMap` keys; user/system time was 0.675 / 0.310 s. All ten seed 1 hashes matched. The extra hash and probe work was slower, so the borrowed-map parser was restored.

### Step 1B attempt 13: rotate-xor chunk hasher (kept)

Board run ID: run-6caaf1c7e57b. Replaced the per-chunk FNV multiplication with a 5-bit rotate and xor. Ten direct runs kept the 0.110 s median while user time fell from 0.575 to 0.525 s; system time remained 0.185 s. In profiles, process wall fell from 133.626 to 126.215 ms, process user/system from 0.611785 / 0.175817 to 0.591633 / 0.167816 s, and minor faults from 16,377 to 15,459.5. Parse wall was 65.462 ms versus 63.738 ms, within phase variation. All ten full seed 1 hashes and samples 2, 3, and 7 passed.

The following Docker Desktop section is retained as historical context only. Its Windows bind mount distorted mmap timing and RSS, so it was not used to choose the current parser.

All measurements in the historical section below were collected in Docker Desktop's Linux VM on the Windows host, not on the scoring machine. Unless noted, each full-data run checked the output against seed 1 SHA-256 `495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3`. Local containers used 12 CPUs, an 8 GiB memory and swap limit, read-only root, a 1 GiB `/tmp` tmpfs, no network, and a read-only `/data` mount. Times below are child-process elapsed time; memory is the container cgroup peak.

## Prior Docker Desktop run (superseded)

The board-provided reference is commit `d8ab7d8`: `T=0.163 s`, `M=65 MB`, `S=394`, `D=1`, `K=13.44`, score `56.422`. These are organizer results supplied in the task brief, not measured here.

| Step | Change | Board run ID | Local T (s) | Local M (MB) | Result |
|---|---|---|---:|---:|---|
| 0 | Preserved mmap baseline snapshot | run-42f9a5351366 | 1.54 median of 5 paired runs | 284.6 median of 5 | All five full seed 1 hashes matched |
| 1 | Experimental Unix input parsing used disjoint newline-aligned `pread` ranges and a reusable 1 MiB buffer per worker | run-42f9a5351366 | 1.49 median of 5 paired runs; 1.31 median of 5 consecutive candidate runs | 56.2 median paired; 55.7 median consecutive | Reverted after warmed WSL ext4 profiles showed slower parsing |

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

Every item below was tested in the tools-side profiler copy, not retained in the shipped source. Each experiment used five full seed 1 runs and each output hash matched. The comparison baseline for the profiling experiments was mmap: about 1.50 s instrumented main total and 1.109 s parse median. Board run ID: run-6caaf1c7e57b.

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

## Goal run: stop-rule result

The requested 5-consecutive-attempt stop rule was reached in Step 1B. The last five direct medians were 0.115/0.110 s (borrowed endpoint slices, repeated), 0.130 s (prehashed keys), 0.110 s (compile-time AVX2 dispatch), 0.145 s (fused hash/table), and 0.110 s (rotate-xor hasher). None improved the direct wall median by 5% over the preceding kept candidate. The 3.5x stop condition was not reached: the final 0.110 s WSL median is 2.77x faster than the same-method 0.305 s baseline snapshot. Steps C and D were not started after this stop condition.

The final WSL run used 12 workers pinned to CPUs `0-11` on a 16-logical-CPU WSL2 VM. `/data/requests.log` was on ext4 and warm before timing. Ten direct full seed 1 runs had median wall/user/system times of 0.110 / 0.525 / 0.185 s, zero major faults, 15,472.5 minor faults, and 275,806 KB max RSS. The final phase profile median was 0.126215 s; parsing was 65.462 ms, merge 13.498 ms, selection 9.056 ms, formatting 5.929 ms, and writing 14.345 ms. The user-per-thread parse target of 40 ns per record was not reached.

The one requested Docker board-style check passed: image size 30 MB, output SHA-256 `495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3`. Its 1.87 s includes Docker startup and is not a scoring-time measurement. `cargo fmt --check` passed for both Rust manifests. The pinned Docker `FROM` lines remain unchanged; the build sets the permitted `RUSTFLAGS="-C target-cpu=x86-64-v3"`.

`scc` is not installed in the environment. A nonblank, non-comment Rust-line approximation counted 489 lines in `src/main.rs`; the Dockerfile has 8 code lines, for `S≈497`. This is not an exact `scc` result. For an indicative K only, scale the board reference `T=0.169 s`, `M=68 MB` by the same-method WSL timing ratio `0.110/0.305` and by the ratio of WSL peak RSS after subtracting the 238,836,234-byte input mapping. This gives estimated `T≈0.06095 s`, `M≈53.19 MB`, `S≈497`, and `K≈22.11` (about 67.23 points) from the supplied formula. These are estimates, not board measurements; the `K≥32` target was not reached.
## Track A optimization round: 2026-11-01

This section is appended to the existing performance history. Earlier entries above remain in this file. New local measurements are from the WSL A/B records under `tools/track-a/`; no board run IDs were supplied for this round.

| Step | Change | Evidence | Board run ID | Result |
|---|---|---|---|---|
| a | Exit with `_exit(0)` after output | 20 alternating runs: min/median 112.397/124.058 -> 98.428/120.405 ms; RSS proxy 43,026 -> 42,854 KB | local WSL measurement | Kept |
| b | Shared-mapped output with positioned-write fallback | 20 alternating runs: min/median 94.807/114.968 -> 82.814/105.503 ms; RSS proxy 42,866 -> 50,034 KB | local WSL measurement | Kept; selected shipped candidate by estimated K |
| c | Atomic 96-chunk scheduling | Static 104.725/131.860 vs dynamic 113.162/126.414 ms; RSS proxy 65,574 vs 67,318 KB | local WSL measurement | Reverted; min slower and memory higher |
| d | Flat per-worker state, exact latency storage, parallel prefix/scatter | Min/median 116.950/132.194 -> 99.090/113.030 ms; RSS proxy 66,402 -> 50,974 KB | local WSL measurement | Kept in candidate c1 |
| e | Open-address endpoint table and previous-endpoint cache | Total min 107.938 -> 104.616 ms; parse median 49.583 -> 48.470 ms | local WSL measurement | Reverted; below the required gain |
| f | Persistent worker team | Min/median 121.657/139.196 -> 115.007/128.132 ms; RSS proxy 50,306 -> 52,586 KB | local WSL measurement | Kept in candidate c1 |
| g | Arithmetic/SWAR parsing and combined AVX2 delimiter scan | Min/median 115.224/131.337 -> 108.595/129.201 ms; RSS proxy 52,586 KB both | local WSL measurement | Kept in candidate c1 |
| h | Partial percentile selection | Selection phase median improved 18.5%; full-sort vs partial-selection min/median 97.514/132.889 vs 105.656/123.060 ms | local WSL measurement | Kept in candidate c1 under the phase gate |
| i | Release packed records after scatter; exact 4-byte latency array | Included in step d; no separate A/B | local WSL measurement | Kept as part of d |

The shared-mmap B2 source is currently in `track-a/src/main.rs`; Rust plus Dockerfile count is 590 physical lines, with a local nonblank/noncomment estimate S=539. Its measured WSL min/median was 82.814/105.503 ms and RSS proxy was 50,034 KB. Applying the supplied `T_board=0.55*WSL min` and score formula with RSS proxy 50,034/1024 MiB and S=539 gives estimated board T=0.0455 s, K about 25.7, or about 70.5 points. This is not a board result and the final fixture/reference sweep remains pending because WSL launch returned `Wsl/Service/E_ACCESSDENIED`.

The instrumented phase snapshot belongs to candidate c1, not the currently selected B2 source: parse 60.004 ms, merge/scatter 37.455 ms, selection 4.342 ms, formatting 4.469 ms, writing 13.189 ms, main-to-exit 130.127 ms. The exact B2 phase table was not captured. The parser ablation ladder was 40.2, 61.7, 104.1, 130.8, 179.5, and 217.4 ns/line at 12 threads; at one thread it was 24.7, 38.9, 65.5, 67.7, 78.0, and 100.0 ns/line. Group bookkeeping was the largest incremental cost. See `tools/track-a/EXPERIMENTS.md` for the full measurements and candidate details.

## Board variants (2026-11)

These are single-change variants built from the baseline. Fill in each author-run ID and board metrics after its push. No local timing claims are used to select them.

| Step | Change | Board run ID | Board T / M / S / K | Status |
|---|---|---|---|---|
| v1 | `_exit(0)` after output | RUN-ID-TBD | Points 67.034; T/M/S/K not supplied | Best known so far; below target 70 |
| v2 | Arithmetic/SWAR/AVX2 line parser | RUN-ID-TBD | Points 66.987; T/M/S/K not supplied | Best known; just below 70 |
| v3 | Open-address endpoint lookup | RUN-ID-TBD | Points 66.506; T/M/S/K not supplied | Rejected; below best 66.987 |
| v7 | v1 exit path plus v2 parser | RUN-ID-TBD | Points 67.289; T/M/S/K not supplied | Best known so far; below target 70 |
| v8 | v7 plus v5 percentile selection | RUN-ID-TBD | Points 67.105; T/M/S/K not supplied | Rejected; below v7 67.289 |
| v4 | Per-worker bump arena for grouped latencies | RUN-ID-TBD | Points 62.753; T/M/S/K not supplied | Rejected; below baseline 66.975 |
| v5 | Partial percentile selection | RUN-ID-TBD | Points 66.661; T/M/S/K not supplied | Rejected; below best 66.987 |

## Selection correction for board evidence

The preceding WSL-based selection note is superseded. The author supplied the real board comparison: baseline commit b4d1b99 scored 66.975 points (K=21.85, T=0.066 s, M=47 MB, S=497), while the flat-state/shared-output rewrite scored 62.698 (T=0.081 s, M=71 MB, S=540). The active `track-a/src/main.rs` has been restored to the baseline source and matches `tools/track-a/baseline-snapshot2/src/main.rs` byte-for-byte. The v1-v5 rows above are independent board candidates; no local timing or score is used to choose among them.

## Recent summary preserved from the working copy

The complete prior log is preserved in [PERF-history.md](../tools/track-a/PERF-history.md). Historical performance rows below summarize each retained or reverted idea; the linked file has the earlier details and checks.

| Step | Change | Evidence | Board run ID | Result |
|---|---|---|---|---|
| Reference | Organizer Python baseline | T 51.906 s; M 198.9 MB; supplied in the original run record | run-30c2dd2e0904 | Reference |
| A1 | mmap, newline-aligned workers, per-worker endpoint interning | Original full seed 1 hash matched | RUN-ID-TBD | Kept |
| A2 | Parallel group merge | Did not corroborate wall-time gain in phase profile | local WSL measurement | Reverted |
| A3 | Parallel hour-range formatting and positioned writes | Formatting phase fell from 25.560 to 6.509 ms; local 10-run median from 0.290 to 0.245 s | local WSL measurement | Kept |
| A4 | Weight selection ranges by record count | Selection phase fell from 33.547 to 11.871 ms | local WSL measurement | Kept |
| A5 | Hour-major parallel merge | Local median fell from 0.240 to 0.180 s | local WSL measurement | Kept |
| A6 | AVX2 delimiter scan | Local median fell from 0.180 to 0.155 s; hashes matched | local WSL measurement | Kept |
| A7 | Three rank selections | Same direct median; instrumented total increased | local WSL measurement | Reverted |
| A8 | Arithmetic day index | Initial gain did not repeat | local WSL measurement | Reverted |
| A9 | Reserve local latency vectors | No corroborated gain; RSS rose | local WSL measurement | Reverted |
| A10 | Chunked endpoint hashing | Local median 0.155 to 0.120 s | local WSL measurement | Kept |
| A11 | `write_vectored` | Same direct median; no write-phase gain | local WSL measurement | Reverted |
| A12 | Pre-size merged vectors | Merge time and RSS increased | local WSL measurement | Reverted |
| A13 | Move first partial latency vector | Local median 0.120 to 0.110 s | local WSL measurement | Kept |
| A14 | Shared-mmap output copy | Slower with more major faults and higher RSS | local WSL measurement | Reverted |
| A15 | High-bit hash-table indexing | Local median 0.150 vs 0.120 s | local WSL measurement | Reverted |
| A16 | SWAR latency conversion | Parse time unchanged within measurement noise | local WSL measurement | Reverted |
| A17 | Open-addressed endpoint table | Slower despite lower RSS | local WSL measurement | Reverted |
| A18 | Borrow endpoint slices from mmap | Merge profile and RSS improved; hashes matched | local WSL measurement | Kept |
| A19 | Prehashed endpoint keys | Local median 0.130 vs 0.110 s | local WSL measurement | Reverted |
| A20 | Compile-time AVX2 dispatch | Parse phase improved; full hashes and samples matched | local WSL measurement | Kept |
| A21 | Fused delimiter hash and endpoint table | Local median 0.145 vs 0.110 s | local WSL measurement | Reverted |
| A22 | Rotate-xor chunk hasher | Same direct median; lower user time in profile | local WSL measurement | Kept |
| A23 | Dense per-worker counters and packed records with prefix/scatter | 15 alternating full seed 1 runs: baseline min/median 121.142/137.124 ms, candidate 136.079/149.736 ms; RSS proxy max 43,245/50,153 KB; code-line S 497/540. All hashes matched. | local WSL measurement | Reverted; failed the required 10% minimum-wall gain |
| A24 | Buffered `pread` input | Warm ext4 parse and total time were slower than mmap | local WSL measurement | Reverted |

The latest candidate passed the worked example, empty input, sample seeds 2/3/7, full seed 2, boundary fixtures, single-line input, and the 5,000-endpoint ECAP-growth fixture. The final source is the saved mmap baseline because the packed-record rewrite missed the timing gate and raised the RSS proxy. No Docker check was run.

## Previously selected candidate (H164)

At that point, H164 was selected in `track-a/src/main.rs` (SHA256 `DBB4E2CCBD8AB7CAE22DCAF68B934C6FE3662BA571C2848A2F3E3442F46F0C65`). Its board result is recorded below.

| Candidate | Board run ID | Board T / M / S / K | Commit |
|---|---|---|---|
| H164 | run-0e251929474d | 65.398 pts; T/M/S/K not supplied | COMMIT-TBD |

## Board result update

H164 scored 65.398 points on run `run-0e251929474d`, below the author-reported b4d1b99 baseline score of 66.975 points (K=21.85, T=0.066 s, M=47 MB, S=497). H164 is not the current source. I restored `track-a/src/main.rs` from `tools/track-a/baseline-snapshot2/src/main.rs`; the files match at SHA256 `71CF21E17AF0FF9B2E672943C3444C346B`. The restored baseline's prior board result remains the expected reference; no new board run is claimed for this restoration.

| Candidate | Board run ID | Board T / M / S / K | Commit |
|---|---|---|---|
| b4d1b99 baseline (restored locally) | RUN-ID-TBD | Prior result: T=0.066 s / M=47 MB / S=497 / K=21.85 / 66.975 pts | b4d1b99 |

## 71-point target iteration

| Candidate | Change | Board run ID | Commit | Board T / M / S / K | Result |
|---|---|---|---|---|---|
| v4 | Per-worker latency arena | RUN-ID-TBD | COMMIT-TBD | Points 62.753; T/M/S/K not supplied | Rejected; below baseline 66.975 |
| v2 | Arithmetic/SWAR/AVX2 line parser | RUN-ID-TBD | COMMIT-TBD | Points 66.987; T/M/S/K not supplied | Below target 71 |
| v5 | Partial percentile selection | RUN-ID-TBD | COMMIT-TBD | Points 66.661; T/M/S/K not supplied | Rejected; below best 66.987 |
| v3 | Open-address endpoint lookup | RUN-ID-TBD | COMMIT-TBD | Points 66.506; T/M/S/K not supplied | Rejected; below best 66.987 |
| v1 | `_exit(0)` after output | RUN-ID-TBD | COMMIT-TBD | Points 67.034; T/M/S/K not supplied | Below target 71 |
| v7 | v1 exit path plus v2 parser | run-eefcfa8a21c2 | 73b102f33ad580661005ba129efd396ddf72df0c | T=0.062314 s / M=47,575,040 B / S=526 / K=22.171133 | Best known; 67.289394 pts, below target 71 |
| v8 | v7 plus v5 percentile selection | run-40f20faa186c | ce593ff82c0ef4852183836fa5bf1830d200a521 | T=0.063016 s / M=47,259,648 B / S=541 / K=21.983725 | Rejected; 67.105065 pts, below v7 |
| c3 | Shared-mapped output with positioned-write fallback | RUN-ID-TBD | COMMIT-TBD | Points 62.596; T/M/S/K not supplied | Rejected; below v7 67.289 |
| H142 | Compact group index, 7 parse/5 post workers, huge-page hint | RUN-ID-TBD | 6dec450 | TBD | Locally committed; push/board run pending |

## Board update: H142 and H83

H142 was pushed as commit `1e0632050363f93262caef45d42b84f1b04c56a8` and scored 64.679018 points. The board reported T=0.0848 s, M=38,584,320 B, S=555, K=19.659857 (run `run-dc3e534c6b5b`). This is below the v7 best of 67.289394, so H142 is rejected. The active source is now H83, restored from `tools/track-a/work/h83-nine-parse-five-post/src/main.rs`; it uses 9 parse workers, 5 later-phase workers, and CRC32C hash plus avalanche. Its S is 558. It has passed the saved local full-hash, sample and fixture checks; no board score is claimed yet.

| H83 | 9 parse/5 post workers; CRC32C plus avalanche | run-95ac2b92059b | 74c7df4534f2fa8b86367c9f884f5d6b70605c67 | T=0.082354 s / M=43,110,400 B / S=558 / K=19.496224 / 64.497525 pts | Rejected; all board gates passed, but below v7 |

H83 completed successfully on the board (run `run-95ac2b92059b`): T=0.082354 s, M=43,110,400 B, S=558, K=19.496224, 64.497525 points. Restored v7 as the best-known source, SHA256 `2FD6CA6CFA5551C735C3A2D2D812D767D1334FA60570302F480D2E157960AE17`, S=526. The current best remains 67.289394 points; the 71-point target is unmet.
