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
