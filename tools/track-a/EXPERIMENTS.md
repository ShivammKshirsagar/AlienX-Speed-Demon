# Track A experiments

All timings below are local WSL measurements on ext4 with warm input and `taskset -c 0-11`. Each A/B row is based on the alternating 20-run, seed-1 hash-checked `ab.sh` comparison unless noted. RSS is the script's max-RSS-minus-input-mapping proxy, not board cgroup memory. “Min / median” values are milliseconds.

| Step | Change | Before min / median | After min / median | RSS proxy before / after (KB) | Result |
|---|---|---:|---:|---:|---|
| a | Skip destructors after output with `_exit(0)` | 112.397 / 124.058 | 98.428 / 120.405 | 43,026 / 42,854 | Kept: min improved 12.4%; median did not regress. |
| b | Shared mapped output with positioned-write fallback | 94.807 / 114.968 | 82.814 / 105.503 | 42,866 / 50,034 | Kept: min improved 12.6%; RSS proxy rose about 7 MB. |
| c | Atomic 96-chunk scheduling | 104.725 / 131.860 static | 113.162 / 126.414 dynamic | 65,574 / 67,318 | Reverted: min slower and memory higher. |
| d | Flat per-worker state, prefix/scatter, exact latency array, parallel scatter | 116.950 / 132.194 | 99.090 / 113.030 | 66,402 / 50,974 | Kept: min improved 15.3%, median 14.5%, proxy fell about 15 MB. |
| e | Open-address endpoint table and previous-endpoint cache | HashMap/open-table direct min 107.938 / 125.686 vs 104.616 / 115.218; parse median 49.583 vs 48.470 ms | — | — | Reverted: total min gain 3.1%, below gate; parse improvement was only 2.2%. Cache variant also missed its gate. |
| f | Persistent worker team across phases | 121.657 / 139.196 | 115.007 / 128.132 | 50,306 / 52,586 | Kept: min improved 5.5%, median 8.0%; proxy rose about 2.3 MB. |
| g | Arithmetic date/hour, SWAR latency, combined AVX2 delimiter scan | 115.224 / 131.337 | 108.595 / 129.201 | 52,586 / 52,586 | Kept: min improved 5.8%; median did not regress. An initial delimiter-offset bug was fixed before these measurements. |
| h | Partial selection for p99/p95/median | 97.514 / 132.889 full sort | 105.656 / 123.060 `select_nth_unstable` | — | Kept: selection phase median improved 18.5%, and direct A/B median improved without regression. |
| i | Free packed records after scatter; exact-size 4-byte latency storage | Included in d | Included in d | Included in d | Kept as part of d; no separate A/B was run. |
| Final selection | Choose the best estimated-K candidate for `track-a/` | Current flat-state variant: 105.656 ms min, about 52,586 KB RSS proxy, S 720 | Shared-mmap B2: 82.814 ms min, 50,034 KB RSS proxy, S 539 | — | Selected B2 based on the recorded local score inputs; the measurements came from separate sessions, so this ranking is provisional until board comparison. |

The earlier packed rewrite was slower overall. Its profiler showed parse around 66.8 ms and merge/scatter around 32.9 ms; the parse saving did not repay the more expensive merge/scatter path. This is consistent with the original 27 ms merge estimate, but the matched baseline Instant profile could not be recovered in this environment, so the attribution is not definitive.

## Parse ablation ladder

| Cumulative rung | 12-thread ns/line | 12-thread wall (ms) | 1-thread ns/line | 1-thread wall (ms) |
|---|---:|---:|---:|---:|
| Newline scan | 40.2 | 37.194 | 24.7 | 76.932 |
| + fields and hour | 61.7 | 23.432 | 38.9 | 121.036 |
| + endpoint lookup | 104.1 | 36.299 | 65.5 | 203.585 |
| + status and latency | 130.8 | 39.832 | 67.7 | 210.493 |
| + group bookkeeping | 179.5 | 58.978 | 78.0 | 243.317 |
| + latency push | 217.4 | 73.628 | 100.0 | 326.750 |

Bookkeeping was the largest incremental rung (+48.7 ns/line and about 19 ms wall); storing latency added another 37.9 ns/line. The 1-thread versus 12-thread values are separate runs, so wall-time differences also include parallelism and memory effects.

## Current profile snapshot

One instrumented run of the current source measured parse 60.004 ms, merge/scatter 37.455 ms, selection 4.342 ms, formatting 4.469 ms, writing 13.189 ms, and main-to-exit 130.127 ms. Per-worker parse finish times (ms): 53.151, 56.790, 57.250, 51.153, 59.648, 49.311, 52.633, 53.182, 58.075, 54.175, 56.005, 56.205. Instrumented phase timings are diagnostic; the direct A/B wall results above decide keep/revert.

## Validation status

The recorded checks from the active WSL session include seed-1 full output hash checks throughout A/B, worked example, empty input, and sample seed 2 after the persistent-pool and parser changes. The requested final fixture/reference sweep is still pending: WSL currently returns `Wsl/Service/E_ACCESSDENIED` before starting a distribution. No correctness result is claimed for checks that could not run here.

## 2026-11-01 structural pipeline comparison

One final 30-round same-session Docker Desktop comparison interleaved baseline and n1, n2, and n3. Each measured invocation used the 12-CPU, 8 GB, read-only, no-network container flags, warm input cache, and checked the seed-1 SHA-256. CPU is cgroup `usage_usec` delta (all container user+system CPU); memory is cgroup `memory.peak`. These Docker Desktop bind-mount measurements are local and are used only for the requested paired gate.

| Experiment | Variant | CPU min / median (s) | Wall min / median (s) | `memory.peak` median (MB) | S | K relative to baseline | Checks | Decision |
|---|---|---:|---:|---:|---:|---:|---|---|
| N1 | One-pass packed records, merged flat endpoint state, parallel prefix/scatter | 1.435 / 1.759 | 1.045 / 1.312 | 292.32 | 970 | 0.970 | Worked, empty, single, chunk-boundary, equal-time, hour-boundary, 5,000 endpoints, sample 2/3/7, full seed 2 reference, seed 1 hash: pass | Reverted: CPU minimum improved 13.9%, but estimated K lower |
| N2 | N1 plus previous-endpoint cache | 1.467 / 1.704 | 1.116 / 1.252 | 293.03 | 970 | 0.959 | Same full check set: pass | Reverted: CPU minimum improved 11.94%, below the >=12% gate; estimated K lower |
| N3 | Two-pass count/fill with stable chunk ownership and no packed-record array | 1.724 / 2.048 | 1.099 / 1.301 | 277.01 | 967 | 0.895 | Same full check set: pass | Reverted: memory fell 3.0%, but CPU minimum was 3.5% slower and K lower |

The phase snapshots are single in-process Docker runs, collected from tools-only instrumented copies; they are diagnostic and not used as the candidate gate:

| Phase (ms) | Baseline snapshot2 | N3 two-pass |
|---|---:|---:|
| Parse pass 1 | 955.3 | 905.7 |
| Merge, prefix, scatter/fill | 13.8 | 101.5 |
| Selection | 10.6 | 12.0 |
| Format | 9.1 | included with output |
| Write | 76.0 | included with output |
| Output setup + format + write | not split | 107.8 |

Parse remained over 80% of profiled in-process time for both. The existing measured parse ladder above attributes the largest incremental costs to endpoint lookup (+42.4 ns/line) and per-line bookkeeping (+48.7 ns/line), followed by latency storage (+37.9 ns/line). The one-entry endpoint cache was tried in N2; it met only the CPU-minimum condition, not the K condition. The two-pass experiment removed packed records from peak memory but paid for a second scan and its cursor work. No candidate cleared both required gates; `track-a/src` remains the byte-identical baseline snapshot2. No new board run ID or local timing was added to `track-a/PERF.md`.

## 2026-11-01 board-loop candidate v7

| Candidate | Change | Checks | S estimate | Full-input RSS proxy | Board evidence / status |
|---|---|---|---:|---:|---|
| v7 | Bundle v1 `_exit(0)` path with v2 arithmetic-hour, SWAR-latency, and paired AVX2 delimiter parser | Offline Docker build; worked, empty, single-line, 12-worker chunk-boundary, equal-time, hour-boundary, 5,000-endpoint reference, samples 2/3/7, full seed 2 reference, and full seed 1 hash all passed | 524 | 42,022 KB (VmHWM 275,260 KB minus 238,836,234-byte input mapping); control proxy 42,498 KB | Candidate C handoff only; no board run yet; no local timing used |

The v2+v4 parse-arena bundle was retained at `tools/track-a/candidates/parse-arena-bundle/` but not selected because the two-pass parse rescans each line. WSL and remote board-feed/Git access were unavailable; `track-a/` remains the exact control. See `LOOP_STATE.md` for the manual C/K/C procedure. PERF.md history was left intact.
