# Track B: performance log

## Run IDs I filled in

Board IDs are filled in by the author after each push; TBD means not yet pushed.

## Optimization ledger

| Step | Change | Board run ID | Commit | Board T / M / S / K | Local in-process ms | Result |
|---|---|---|---|---|---:|---|
| Baseline | Initial parallel query and scan implementation | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reference |
| Parallel query blocks | Up to 12 query workers | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Kept; historical local median wall 17.50 s |
| Parallel corpus scans | Newline-aligned scans and disjoint posting fills | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Kept; historical local median wall 5.54 s |
| In-place bitmap updates | Bitmap updates and 16-bit result sum tables | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reverted; 3.2% slower |
| Phrase posting intersection | Reject phrase candidates using all phrase-term postings before verification | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Kept; local query phase 3,736 to 1,148 ms |
| Per-worker posting collection | Collect per-term postings and merge common terms into bitmaps | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reverted; peak RSS rose to 489,980 KB |
| Query-term Bloom filter | 2^20-bit prefilter before table probes | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reverted; 8.1% faster, below threshold |
| Rolling phrase guard | Check final phrase token before full-window comparison | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Kept; local median wall 3.20 to 2.84 s |
| Phrase-byte resolver | Compare tokens against interned phrase terms | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Kept; local median wall 2.84 to 2.20 s |
| CRC/hash and slot attempts | Inline slots, CRC variants, chunk mixing, branchless folding, bitmap writes | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reverted; see individual experiment rows below |
| AVX2 scan/probe rewrite | AVX2 token masks and inline 128-bit term keys | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Kept; token probe 147.32 to 62.21 ns/token |
| Phrase boundary/query path | LF bounds, borrowed bitmap rows, dynamic query blocks, AVX2 anchor verification | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Kept; instrumented query phase 476 to 200 ms |
| One-pass hot/cold attempt | Sampled hot bitmaps and one-pass cold-pair scatter | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reverted at the then-current stop gate |
| Hot/cold one-pass B | Hot bitmaps, cold pairs, sorted flat scatter | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | 765.474 | Kept; sample/hash checks passed |
| Software-pipelined scan | Staged AVX2 positions, tag prefetch, merged state, stable radix scatter | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | 916.720 | Kept; total improved over previous measured source |
| Query arenas and parallel setup | Flat query ID/op arenas, fast interning hash, parallel hot sampling, zeroed bitmaps, concurrent MADV_WILLNEED | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Kept; correctness checks passed |
| Phrase/query memory changes | Line-start phrase bounds, borrowed posting lists/bitmap rows, integer output | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Kept; correctness checks passed |
| Bitmap-frequency cutoff | Keep only sampled terms above N/32 in hot bitmaps | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | 947.436 | Reverted; total was over 2% slower |
| Current local scoreboard | Software-pipelined source, final min-of-3 | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | 916.720 | Local result; author can fill the run ID after pushing |
| Peak-memory scoreboard instrumentation | Sample phase-boundary and live-process RssAnon, VmHWM, RssFile, scan faults, worker times; use peak RssAnon for estimated M | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | 1025.707 | Added; three-run baseline and packed-pair scoreboards measured locally |
| Packed pair buffers | Store term ID plus worker-local doc offset in 32 bits; switch to full pairs on range overflow | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | 1005.811 | Kept; estimated K 12.10 → 12.72, peak RssAnon 175,236 → 139,336 KB; hash and quick fixtures passed |
| 48 dynamic scan chunks | Atomic work queue over newline-aligned chunks | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | 1037.243 | Reverted; estimated K 12.72 → 11.89 and peak RssAnon rose to 178,616 KB; hash and quick fixtures passed |
| Six scan workers | Limit scan/query workers to 6 | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | 1339.705 | Reverted; estimated K 13.41 → 10.49, peak RssAnon 178,884 KB; hash and quick fixtures passed |
| Unified list/bitmap word loop | Replace mixed set branches with one word loop plus cursors | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | 1000.208 | Reverted; estimated K 13.41 → 12.85 despite S 1,011 → 965; queries slowed to 184.790 ms; checks passed |
| Release pair buffers during scatter | Move each worker buffer into its scatter task so it drops on completion | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | 936.890 | Reverted; peak RssAnon fell to 131,116 KB but estimated K 13.41 → 13.34, below the keep gate; checks passed |
| Phrase token equality | Use slice `eq_ignore_ascii_case` for exact ASCII-folded token comparison | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | 971.063 | Reverted; estimated K 13.41 → 12.96; correctness passed |
| Current packed-pair source scoreboard | 12-worker packed-pair source, three-run minimum | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | 906.566 | Local estimate: T_board 0.4986 s, peak RssAnon 139,104 KB, S=1,011, K=13.407, 56.37 points; seed-1 hash OK |
| Existing board result | Existing recorded board run | run-c29a5e5f8214 | COMMIT-TBD | 0.550 s / 202 MB / 941 / 11.49 | TBD | Board result already recorded |

The historical experiment tables below remain intact. `RUN-ID-TBD` and `COMMIT-TBD` mark values the author has not supplied.

## Current round scan diagnosis

Measurements are local WSL on the warm 16 MiB seed-1 slice pinned to CPU 0. `perf stat` is not installed; minor/major faults are sampled with `getrusage`.

| Cumulative stage | ns/token | Minor/major faults |
|---|---:|---:|
| AVX2 tokenization | 10.79 | 2 / 0 |
| + term-table probe (79.2% hit rate) | 45.79 | 0 / 0 |
| + per-doc dedupe and counter | 60.35 | 0 / 0 |
| + pair append (1,472,765 pairs) | 63.07 | 3,390 / 0 |
| + radix and scatter | 72.46 | 7,091 / 0 |

Touched storage: term table + tags 4,718,592 B; term bytes 586,904 B; last-doc 302,988 B; counters 302,988 B; 16 MiB-slice pairs 11,782,120 B; posting offsets 302,988 B for one worker / 3,635,856 B for 12.

One full-run thread-CPU query profile: BOOL lists 127.9 ms; BOOL bitmap queries 679.2 ms; PHRASE queries 1,003.7 ms. The sum exceeds wall time because it adds parallel thread CPU. The profile build passed the full hash; query timing instrumentation was removed from the retained source.

All timings below are native WSL2 runs with `/data` on the WSL ext4 filesystem, the input cache warmed by `cat /data/docs.txt >/dev/null`, and execution pinned with `taskset -c 0-11`. Each retained variant has five full seed-1 runs; every run produced the expected SHA-256 `f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927`. RSS is the Linux `/usr/bin/time -v` maximum in KB and includes resident mmap pages. Local-only rows use `RUN-ID-TBD` until the author supplies a board run ID.

| Step | Change | Board run ID | Median wall (s) | User (s) | Sys (s) | Max RSS (KB) | Major/minor faults | Phase medians: parse / pass A / pass B / query / output (ms) | Result |
|---|---|---|---:|---:|---:|---:|---:|---|---|
| 0 | Single-threaded scans and query evaluation | RUN-ID-TBD | 52.53 | 52.30 | 0.20 | 332668 | 0 / 25967 | 37 / 5591 / 7115 / 39729 / not instrumented | Baseline |
| 1 | Parallel query blocks, up to 12 workers | RUN-ID-TBD | 17.50 | 51.65 | 0.29 | 337276 | 10 / 27245 | 46 / 5964 / 7359 / 3837 / 1 | Kept; 66.7% faster, full hash passed |
| 2 | Newline-aligned parallel corpus scans and disjoint postings fills | RUN-ID-TBD | 5.54 | 56.18 | 0.22 | 375780 | 11 / 37794 | 42 / 586 / 1071 / 3736 / 1 | Kept; 68.3% faster, full hash passed |
| 3 | In-place bitmap updates and 16-bit result sum tables | RUN-ID-TBD | 5.72 | 57.63 | 0.26 | 375808 | 12 / 37782 | 37 / 617 / 1050 / 3970 / 1 | Reverted; 3.2% slower than step 2 despite full hash pass |

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

| Step | Change | Board run ID | Commit | Board T / M / S / K | Local in-process ms | Result |
|---|---|---|---|---|---:|---|
| Baseline | Prior source; local median wall 2.33 s, max RSS 379,008 KB | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reference measurement |
| Inline slot layout A | 64-bit tag/ID slots with 32-byte inline term storage; wall 3.43 s, RSS 382,148 KB | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reverted; slower |
| Inline slot layout B | 64-bit tag/ID slots without inline storage; wall 2.67 s, RSS 380,032 KB | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reverted; slower |
| CRC32C hash | CRC32C token hash; wall 2.59 s, RSS 379,132 KB | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reverted; slower |
| CRC32C avalanche | CRC32C plus 64-bit avalanche; wall 2.54 s, RSS 378,624 KB | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reverted; slower |
| Chunk-mixing hash | Chunk-mixing token hash; wall 2.62 s, RSS 378,752 KB | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reverted; slower |
| Branchless predicate | Branchless ASCII word predicate; wall 2.50 s, RSS 378,900 KB | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reverted; slower |
| Inline terms | 16-byte inline terms with case folding; wall 2.14 s, RSS 380,324 KB | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reverted; 8.2% faster, below threshold |
| Inline terms plus folding | Inline terms plus branchless folding; wall 2.12 s, RSS 380,064 KB | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reverted; 9.0% faster, below threshold |
| Inline terms plus subrange | Inline terms plus subrange byte predicate; wall 2.13 s, RSS 380,192 KB | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reverted; below threshold |
| Non-atomic bitmap stores | Non-atomic bitmap word writes with doc-aligned chunks; wall 2.61 s, RSS 355,896 KB | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reverted; slower despite lower RSS |
| Two-lane FNV hash | Two-lane FNV-style token hash; wall 2.63 s, RSS 378,752 KB | RUN-ID-TBD | COMMIT-TBD | TBD / TBD / TBD / TBD | TBD | Reverted; slower |

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

## Software-pipelined scan (local WSL measurement)
| Phases (ms: parse/table/setup/scan/query/output) | Before (previous min-of-5) | After (min-of-3) |
|---|---:|---:|
| Time | 78.8/15.4/19.6/656.7/220.2/4.6 = 995.466 | 40.3/14.6/2.1/676.7/173.3/9.6 = 916.720 |
| RssAnon | 96,648 KB | 95,292 KB |

Kept WP0–WP3: staged AVX2 token positions, tag-prefetched probing, merged per-term state, stable two-pass radix scatter, query arenas, parallel hot sampling, zeroed bitmap storage, line-start phrase bounds, borrowed list/bitmap rows, and itoa output. Reverted WP4’s bitmap-frequency cutoff because its measured total was over 2% slower. Rust + Dockerfile harness-style count (formatted copy without `.rustfmt.toml`): 941 before, 962 after; the <=700 target was not met. Final seed-1 hash passed; sample seeds 2–5 passed against saved Python-reference outputs.
## AVX2 scan/probe rewrite (local WSL measurement)
AVX2 token masks and an inline 128-bit term key now serve both corpus passes and phrase verification; scalar fallback remains. Tokenize+probe: 147.32 to 62.21 ns/token; full seed-1 wall: 2.33 to 2.22 s; Rust + Dockerfile: 768 lines. Kept; required hashes passed.

## Phrase boundary and query-path update (local WSL measurement)
Fixed PHRASE document bounds; borrowed bitmap rows, dynamic 64-query blocks, per-worker scan state, and AVX2 anchor verification reduced the single instrumented wall run 1.86 to 1.38 s (query 476 to 200 ms); final three-run median 2.195 s, 378,816 KB RSS, expected seed-1 hash passed. Rust + Dockerfile: 869 lines.

## One-pass hot/cold attempt (local WSL measurement)
Sampled hot bitmaps and one-pass cold-pair scatter passed the required sample/tiny checks and all three seed-1 hashes, but median stayed 2.195 s (baseline 2.195 s); reverted to `tools/track-b/baseline-src` at the 25% stop gate.

## Hot/cold one-pass B (local WSL measurement)
Reinstated sampled 1,024-term hot bitmaps and one-pass cold pairs, sorted before flat scatter; pass-B ablations (probe only / bitmap / postings) were 409.759 / 486.972 / 622.342 ms, so postings dominated. Min-of-5 phase times, ms (parse/table/scan A/setup/scan B/query/output): baseline 58.1/5.5/308.4/45.0/702.0/183.3/4.2, 1,306.727 total, 126,132 KB RssAnon; final B 78.8/15.4/0/19.6/656.7/220.2/4.6, 995.466 total, 96,648 KB. A was 1,319.802 ms; B sorted-pair trial was 765.971 ms. External-minus-internal gap: 77.860 ms. Seed-1 hash passed five times; Rust + Dockerfile: 796 lines.
