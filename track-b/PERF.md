# Track B: performance log

All timings below are native WSL2 runs with `/data` on the WSL ext4 filesystem, the input cache warmed by `cat /data/docs.txt >/dev/null`, and execution pinned with `taskset -c 0-11`. Each retained variant has five full seed-1 runs; every run produced the expected SHA-256 `f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927`. RSS is the Linux `/usr/bin/time -v` maximum in KB and includes resident mmap pages. Locally measured rows use `local WSL measurement, no board run` in the Board run ID column. Only an actual board result is identified by its run ID.

| Step | Change | Board run ID | Median wall (s) | User (s) | Sys (s) | Max RSS (KB) | Major/minor faults | Phase medians: parse / pass A / pass B / query / output (ms) | Result |
|---|---|---|---:|---:|---:|---:|---:|---|---|
| 0 | Single-threaded scans and query evaluation | local WSL measurement, no board run | 52.53 | 52.30 | 0.20 | 332668 | 0 / 25967 | 37 / 5591 / 7115 / 39729 / not instrumented | Baseline |
| 1 | Parallel query blocks, up to 12 workers | local WSL measurement, no board run | 17.50 | 51.65 | 0.29 | 337276 | 10 / 27245 | 46 / 5964 / 7359 / 3837 / 1 | Kept; 66.7% faster, full hash passed |
| 2 | Newline-aligned parallel corpus scans and disjoint postings fills | local WSL measurement, no board run | 5.54 | 56.18 | 0.22 | 375780 | 11 / 37794 | 42 / 586 / 1071 / 3736 / 1 | Kept; 68.3% faster, full hash passed |
| 3 | In-place bitmap updates and 16-bit result sum tables | local WSL measurement, no board run | 5.72 | 57.63 | 0.26 | 375808 | 12 / 37782 | 37 / 617 / 1050 / 3970 / 1 | Reverted; 3.2% slower than step 2 despite full hash pass |

Final source, after adding `/out/results.txt` output, ran three consecutive full seed-1 checks through the Linux WSL binary. All three hashes matched; median was 5.86 s wall, 58.63 s user, 0.30 s system, 375424 KB max RSS, and 13 major / 37760 minor faults. Median phases were 43 ms query parse, 640 ms pass A, 1105 ms pass B, 4102 ms query evaluation, and 6 ms output. Rust plus Dockerfile is 648 lines. The local formula proxy is K˜3.41 using this WSL median, 375.424 MB RSS, and S=648; it is not a board score.

The worked example matched both stdout and `results.txt`. Held-out sample comparisons against the Python reference passed:

- Seed 2: `86b92e3ceda664396bad5cabc3809b447de0610282eee484605fd23cbd12fea6`
- Seed 3: `d62a5112e713dd915c35129f31190d9566b8e76ed8615cf96bcd893e4ee4d197`

Reproduction from WSL after building with `RUSTFLAGS='-C target-cpu=x86-64-v3' cargo build --release --offline`:

```sh
cat /data/docs.txt >/dev/null
bash '/mnt/c/GitHub Desktop/AlienX-Speed-Demon/tools/track-b/profile-five.sh' parallel-scan '/mnt/c/GitHub Desktop/AlienX-Speed-Demon/track-b/target/release/solution'
```

Final scoring command requested by the brief: `python check.py b --data data\b --expected f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927`.

## Query class profile before query optimization

One warm full seed-1 WSL run on ext4, pinned to CPUs 0–11. Thread CPU time is summed across query workers; maximum is the largest single query in its class. The output SHA-256 remained `f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927`.

| Query class | Count | Total CPU (ms) | Max query CPU (ms) | Exact phrase candidates checked (total / max per query) | Bytes re-tokenized |
|---|---:|---:|---:|---:|---:|
| BOOL, first list, no bitmap operand | 21,535 | 227.906 | 12.617 | 0 / 0 | 0 |
| BOOL, first list, at least one bitmap operand | 5,650 | 356.991 | 11.614 | 0 / 0 | 0 |
| BOOL, first bitmap, no bitmap operand | 2,832 | 169.596 | 10.845 | 0 / 0 | 0 |
| BOOL, first bitmap, at least one bitmap operand | 983 | 113.821 | 10.080 | 0 / 0 | 0 |
| PHRASE with no anchor | 92 | 1,668.176 | 49.371 | 47,012 / 1,071 | 88,430,509 |
| PHRASE with anchor (`df <= N/1000`) | 18,908 | 33,118.744 | 30.176 | 869,738 / 250 | 1,627,629,674 |

The largest CPU class is anchored PHRASE: 33.12 CPU seconds, 869,738 candidate documents, and 1.63 GB re-tokenized. Anchorless PHRASE uses 1.67 CPU seconds, so the scan-time answer path for its 92 queries is not the first target. The current phrase evaluator chooses only the rarest term's postings; it does not intersect the other phrase terms before re-tokenizing a candidate.

| Pre-fix O(N) BOOL work | Count |
|---|---:|
| BOOL queries with a bitmap operand or bitmap first term, thus a whole-document bitmap operation/scan | 9,465 of 31,000 |
| 64-bit bitmap words cleared/copied/scanned across BOOL queries | 288,305,344 words, about 73,800 full-corpus sweeps of 3,907 words |
| Documents enumerated while converting bitmaps to lists | 1,471,289 |
| Set bits visited to calculate output sums | 70,281,332 |

Profile-run phases were parse 40 ms, pass A 611 ms, pass B 1,006 ms, query phase 3,689 ms, and output 5 ms; wall was 5.43 s and max RSS 375,932 KB. Previous five-run phase medians were 43 / 640 / 1,105 / 4,102 / 6 ms respectively.

## Query profile after phrase-postings intersection

The first query optimization intersects document postings for every phrase term in ascending `df` before retokenizing. It stops at the first missing term and verifies candidates with a rolling eight-token window, stopping at the first exact match. On the instrumented full seed-1 run, the anchored phrase class fell from 33,118.744 to 8,688.193 total CPU ms. Of 869,738 anchor-posting documents, 697,765 were rejected by the other phrase postings; exact verification fell to 171,973 documents and 358,701,176 bytes. Anchorless exact checks were 42,253 documents and 80,505,396 bytes. The full output hash passed.

| Run | Median wall (s) | Median user (s) | Median sys (s) | Max RSS (KB) | Median query phase (ms) |
|---|---:|---:|---:|---:|---:|
| Previous retained source, five runs | 5.54 | 56.18 | 0.22 | 375780 | 3736 |
| Phrase-postings intersection, five runs | 3.26 | 30.85 | 0.32 | 375908 | 1148 |

The 41.2% wall reduction exceeds the 10% keep threshold, so this change is retained. These five runs include temporary query-work counters; all per-query instrumentation was removed before final measurements.

## Reverted scan attempt

A single scan collected per-worker `Vec<u32>` postings for every query term, then merged common terms into bitmaps. Five full seed-1 hashes passed. Median wall was 2.78 s versus 3.26 s for the retained phrase-intersection variant (14.7% faster), but max RSS rose from 375,908 to 489,980 KB. After subtracting the resident `/data/docs.txt` mapping, the WSL process uses roughly 236 MB of private/index memory, above the 150 MB target; this worsens the memory term enough to make the local K estimate lower. Reverted; return to the two-pass compact allocation strategy and try a cheaper token-table prefilter.

## Reverted term-table prefilter attempt

Added a 2^20-bit query-term Bloom prefilter before open-addressed term-table probes. Five full seed-1 hashes passed. Median wall was 2.94 s versus 3.20 s for the retained phrase-intersection source (8.1% faster), below the 10% retention threshold; RSS was unchanged. Reverted.


## Scan-stage profile before changing the shipped scan

Measured with a throwaway single-worker Rust benchmark using the shipped token rules, FNV rolling hash, query-term table, term descriptors, and per-document dedupe/counter/posting/bitmap work. Input was the first newline-aligned 16,778,280-byte slice of warmed `/data/docs.txt` on WSL ext4, pinned with `taskset -c 0`. The table below reports the median of three harness runs. This profile was recorded before changing `track-b/src/main.rs`.

| Cumulative stage | ns/token | Detail |
|---|---:|---|
| (a) byte-class tokenization + rolling hash | 39.26 | 2,400,928 tokens in 16,731 documents |
| (b) (a) + term-table probe | 147.32 | 75.67% of tokens hit a query term |
| (cA) (b) + pass-A per-document dedupe and counter increment | 153.49 | Added about 6 ns/token over (b) by median subtraction |
| (cB) (b) + pass-B dedupe and postings/bitmap write | 184.67 | Added about 37 ns/token over (b) by median subtraction |

The lookup dominates this sample's incremental work: (b) adds about 108 ns/token over (a). The pass-A state update adds little in this small slice; pass-B writes add more. WSL did not have the `perf` executable, so cache misses, instructions, and cycles could not be collected.

| Array touched by the scan | Bytes in this benchmark slice/workload |
|---|---:|
| Open-addressed term table (u32 slots) | 1,048,576 |
| Query term byte payload (separate per-term Vecs; no contiguous arena) | 586,896 |
| Term descriptors (exact shipped field layout, 56 bytes x 75,744 terms) | 4,241,664 |
| `last_doc` (u32 x 75,744) | 302,976 |
| Pass-A counters (u32 x 75,744) | 302,976 |
| Postings offsets (usize x 75,744) | 605,952 |
| Pass-B postings payload | 2,665,700 |
| Bitmap words for frequent terms | 1,546,848 |
## Final phrase refinements and rejected prefilter

A rolling-window guard now compares the last phrase token before checking the full window. Five full seed-1 hashes passed; median wall fell from 3.20 s to 2.84 s (11.25%), so it was retained. A phrase-specific resolver then compared token bytes only against the phrase's interned term IDs. Five hashes passed; median fell from 2.84 s to 2.20 s (22.5%), so it was retained. The optimized full hash remained `f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927`.

The final instrumented phrase-byte run measured query-class CPU: BOOL classes in table order were 217.348, 306.141, 168.265, and 90.293 ms; anchorless PHRASE was 713.843 ms; anchored PHRASE was 2,999.858 ms. Phrase posting-candidate totals were 47,012 (4,759 rejected) for anchorless phrases and 869,738 (697,765 rejected) for anchored phrases; exact checks were 42,253 and 171,973. Bytes re-tokenized were 80,505,396 and 358,701,176. One run measured 2.27 s wall, 379,132 KB max RSS, and phase times 34 / 642 / 1,067 / 440 / 2 ms (parse / pass A / pass B / query / output). These class timings are temporary-instrumentation measurements, not the final uninstrumented phase table.

Re-running the 2^20-bit term prefilter on top of phrase-byte comparison gave five full hashes but a 2.27 s median versus 2.20 s (3.2% slower), with 379,260 KB max RSS. It was reverted.
## Final production build and verification

All per-query and phase timers were removed from `src/main.rs`. The Linux release binary was rebuilt in WSL with `RUSTFLAGS='-C target-cpu=x86-64-v3' cargo build --release --offline`; data stayed on `/data` (WSL ext4), the input was warmed with `cat /data/docs.txt >/dev/null`, and runs used `taskset -c 0-11`.

| Final production measurement | Median |
|---|---:|
| Wall time, five full seed-1 runs | 2.33 s |
| Wall time, final three full seed-1 runs | 2.37 s |
| User / system time, final three | 22.655 / 0.200 s |
| Max RSS | 378,942 KB |
| Major / minor page faults | 12 / 38,572 |
| Rust + Dockerfile lines | 662 |
| Estimated local K using T=2.37 s, M=378.942 MB, S=662 | about 5.3 (estimate; not board score) |

Every one of the five-run and final three-run stdout files had SHA-256 `f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927`. The one-run post-optimization phase sample, measured before removing phase timers, was parse 34 ms, pass A 642 ms, pass B 1,067 ms, query evaluation 440 ms, output 2 ms (wall 2.27 s, max RSS 379,132 KB). This is a single instrumented run; the production five-run phase medians are unavailable because temporary phase timers were removed as required. The pre-optimization comparison is the recorded phase table above: 43 / 640 / 1,105 / 4,102 / 6 ms (five-run medians), with a 5.86 s three-run wall median.

The worked example matched `expected.txt` in stdout and `results.txt` (SHA-256 `9c296c063de983fd734d9e6ab985866967731d6478d33b7e1b8723391b6e325b`). Sample output comparisons against the Python reference passed:

- Seed 2: `86b92e3ceda664396bad5cabc3809b447de0610282eee484605fd23cbd12fea6`
- Seed 3: `d62a5112e713dd915c35129f31190d9566b8e76ed8615cf96bcd893e4ee4d197`
- Seed 4: `0bf35d72fb61f4f510512e16f3f19ed7df7615ccd0331488f3dd737563301fcb`
- Seed 5: `09abaaf35a7e6f3fd621d16d595c1fcb7a1db1656d32854de77c5ae40b0f0b56`

The single requested Docker check passed: pinned base images were allowlisted, the image was 30 MB, and the container output hash matched the expected seed-1 hash. Container time was 5.93 s including startup.

Final board run ID: `run-c29a5e5f8214`.
## Token cost optimization round

No scan optimization met the required 10% median wall-time gain, so the shipped source is restored to the prior working version. Each timing below is a five-run median on full seed 1, local WSL, with the cache warm and CPUs 0–11 pinned. Every listed variant preserved the expected full hash; each was reverted because it was slower or below threshold.

| Attempt | Board run ID | Median wall (s) | Max RSS (KB) | Decision |
|---|---|---:|---:|---|
| Baseline before this round | local WSL measurement, no board run | 2.33 | 379,008 | Reference measurement |
| 64-bit tag/ID slots with 32-byte inline term storage | local WSL measurement, no board run | 3.43 | 382,148 | Reverted; slower |
| 64-bit tag/ID slots without inline storage | local WSL measurement, no board run | 2.67 | 380,032 | Reverted; slower |
| CRC32C token hash | local WSL measurement, no board run | 2.59 | 379,132 | Reverted; slower |
| CRC32C plus 64-bit avalanche | local WSL measurement, no board run | 2.54 | 378,624 | Reverted; slower |
| Chunk-mixing hash | local WSL measurement, no board run | 2.62 | 378,752 | Reverted; slower |
| Branchless ASCII word predicate | local WSL measurement, no board run | 2.50 | 378,900 | Reverted; slower |
| 16-byte inline terms with correct case folding | local WSL measurement, no board run | 2.14 | 380,324 | Reverted; 8.2% faster, below threshold |
| Inline terms plus branchless folding | local WSL measurement, no board run | 2.12 | 380,064 | Reverted; 9.0% faster, below threshold |
| Inline terms plus subrange byte predicate | local WSL measurement, no board run | 2.13 | 380,192 | Reverted; below threshold |
| Non-atomic bitmap word writes with doc-aligned chunks | local WSL measurement, no board run | 2.61 | 355,896 | Reverted; slower despite lower RSS |
| Two-lane FNV-style hash | local WSL measurement, no board run | 2.63 | 378,752 | Reverted; slower |

The restored source's new five-run median was 2.60 s wall, 24.39 s user, 0.25 s system, 378,880 KB max RSS, and 12 major / 38,546 minor faults. The required final three full runs all matched the expected hash; their median was 2.44 s wall, 23.29 s user, 0.21 s system, and 378,942 KB max RSS (12 major / 38,571 minor faults, median). The local formula gives estimated K about 5.3 using T=2.44 s, M=378.942 MB, and S=658; this is an estimate, not a board score. The goal of 1.0 s was not reached.

| Phase | Before this round (ms) | After this round (ms) |
|---|---:|---:|
| Query parse | 34 | 34 |
| Pass A | 642 | 642 |
| Pass B | 1,067 | 1,067 |
| Query evaluation | 440 | 440 |
| Output | 2 | 2 |

These phase figures are the one-run instrumented measurement for the restored source (2.27 s wall, 379,132 KB RSS). Since no experimental source change was retained, the same measured source and phase profile apply before and after this round; the five-run and final-three production timings above are uninstrumented.

The final worked example matched stdout and `results.txt` (SHA-256 `9c296c063de983fd734d9e6ab985866967731d6478d33b7e1b8723391b6e325b`). Final sample comparisons against the Python reference passed:

- Seed 2: `86b92e3ceda664396bad5cabc3809b447de0610282eee484605fd23cbd12fea6`
- Seed 3: `d62a5112e713dd915c35129f31190d9566b8e76ed8615cf96bcd893e4ee4d197`
- Seed 4: `0bf35d72fb61f4f510512e16f3f19ed7df7615ccd0331488f3dd737563301fcb`
- Seed 5: `09abaaf35a7e6f3fd621d16d595c1fcb7a1db1656d32854de77c5ae40b0f0b56`
The single Docker validation for this round passed on the restored source: base images were allowlisted, the image was 30 MB, the container output hash matched, and runtime including startup was 6.38 s.