# Track B experiment log

| Experiment | Est. K before → after | Decision | Evidence |
|---|---:|---|---|
| Parallel query blocks | TBD → TBD | Kept | Historical local wall 52.53 to 17.50 s; hashes passed |
| Parallel corpus scans | TBD → TBD | Kept | Historical local wall 17.50 to 5.54 s; hashes passed |
| In-place bitmap updates | TBD → TBD | Reverted | 3.2% slower |
| Phrase posting intersection | TBD → TBD | Kept | Query phase 3,736 to 1,148 ms |
| Per-worker posting collection | TBD → TBD | Reverted | Peak RSS 489,980 KB |
| Query-term Bloom filter | TBD → TBD | Reverted | 8.1% faster, below prior threshold |
| Rolling phrase guard | TBD → TBD | Kept | Local wall 3.20 to 2.84 s |
| Phrase-byte resolver | TBD → TBD | Kept | Local wall 2.84 to 2.20 s |
| 64-bit tag/ID slot layouts | TBD → TBD | Reverted | Slower than baseline |
| CRC32C hash | TBD → TBD | Reverted | Slower than baseline |
| CRC32C plus avalanche | TBD → TBD | Reverted | Slower than baseline |
| Chunk-mixing hash | TBD → TBD | Reverted | Slower than baseline |
| Branchless ASCII predicate | TBD → TBD | Reverted | Slower than baseline |
| 16-byte inline terms | TBD → TBD | Reverted | 8.2% faster, below threshold |
| Inline terms plus branchless fold | TBD → TBD | Reverted | 9.0% faster, below threshold |
| Inline terms plus subrange predicate | TBD → TBD | Reverted | Below threshold |
| Non-atomic bitmap stores | TBD → TBD | Reverted | Slower despite lower RSS |
| Two-lane FNV hash | TBD → TBD | Reverted | Slower than baseline |
| AVX2 scanner and inline key | TBD → TBD | Kept | 147.32 to 62.21 ns/token; full hash passed |
| Phrase boundary and query-path update | TBD → TBD | Kept | Boundary and sample checks passed |
| One-pass hot/cold attempt | TBD → TBD | Reverted | Then-current wall median did not improve |
| Hot/cold one-pass B | TBD → TBD | Kept | Min 765.474 ms; RSS below 105 MB; hashes passed |
| Software-pipelined scan plus query arenas and line starts | TBD → TBD | Kept | Final min 916.720 ms, RssAnon 95,292 KB; K estimate 14.91 from board-time proxy |
| Sampled bitmap cutoff | TBD → TBD | Reverted | Min 947.436 ms, over 2% slower than retained source |
| 32-bit packed term/document pairs with checked full-width fallback | 12.10 → 12.72 | Kept | Three-run local WSL scoreboard: peak RssAnon 175,236 → 139,336 KB, min in-process total 1,025.707 → 1,005.811 ms; seed-1 hash and quick fixtures passed |
| Cgroup peak diagnostic | 12.76 → 12.76 | Diagnostic/incomplete | Single Docker build/run reached scatter, but failed writing `/out/results.txt` (permission denied) before SD_CGROUP exit output; no actual memory.peak captured; phase RssAnon 139,232 KB, RssFile 249,472 KB, VmHWM 388,704 KB |
| In-place comparison sort for pairs | 12.76 → 11.90 | Reverted | score.sh before the cgroup-delta fallback: total 992.670 ms, RssAnon 135,988 KB, S=1,018; converted with the later board calibration for comparison |
| Four-segment u16 cold postings | 12.76 → 12.83 | Reverted | score.sh: 874.548 ms, calibrated peak estimate 180,344 KB, local RssAnon 115,832 KB, S=1,079; under the 1% keep threshold |
| u16 postings plus denser bitmap cutoff | 12.83 → 12.49 | Reverted | score.sh: 914.941 ms, calibrated peak estimate 184,244 KB, local RssAnon 119,732 KB, S=1,083 |
| Reusable BOOL bitmap scratch | 12.76 → 13.21 | Kept | score.sh: 804.706 ms, calibrated peak estimate 203,660 KB, local RssAnon 139,148 KB, S=998; seed-1 hash and milestone fixtures passed |
| Compact sorted pair runs | 13.21 → 12.78 | Reverted | score.sh: 853.751 ms, calibrated peak estimate 199,668 KB, local RssAnon 135,156 KB, S=1,048 |
| Precompute phrase anchor positions | 13.21 → 12.86 | Reverted | score.sh: 842.024 ms, calibrated peak estimate 203,864 KB, local RssAnon 139,352 KB, S=1,025 |
| AVX2 bitmap word operators | 13.21 → 12.79 | Reverted | score.sh: 846.286 ms, calibrated peak estimate 203,576 KB, local RssAnon 139,064 KB, S=1,049 |
| Release scan-only state before scatter | 13.21 → 12.75 | Reverted | score.sh: 879.847 ms, calibrated peak estimate 194,268 KB, local RssAnon 129,756 KB, S=999 |
| u16 postings plus BOOL scratch | 13.21 → 12.67 | Reverted | score.sh: 906.902 ms, calibrated peak estimate 182,000 KB, local RssAnon 117,488 KB, S=1,031 |
| Fuse final bitmap operator and output summary | 13.21 → 13.08 | Reverted | score.sh: 806.028 ms, calibrated peak estimate 203,512 KB, local RssAnon 139,000 KB, S=1,061 |
| Cgroup memory measurement follow-up | 13.21 → 13.21 | Diagnostic | Root-run `docker-peak.sh` completed: memory.peak 146,067,456 B; exit memory.stat anon/file/kernel/shmem = 106,176,512 / 1,462,272 / 1,355,776 / 0 B; local scan-phase RssAnon 139,292 KB and RssFile 249,472 KB |
| mmap windowing / MADV_DONTNEED | 13.47 → 13.47 | Not applied | Cgroup file charge at exit was only 1.4 MB and memory.peak was 3.3 MiB above local scan RssAnon; discarding mmap windows would not address the measured peak and could refault pages during phrase checks |
| 48 dynamically claimed scan chunks | 12.72 → 11.89 | Reverted | Three-run scoreboard: min total 1,037.243 ms, peak RssAnon 178,616 KB, S=1,032; worse estimated K and memory than packed-pair source; fixtures/sample seed 2 passed |
| Single-core scan-stage diagnosis | 12.72 → 12.72 | Diagnostic | Warm 16 MiB slice: tokenize 10.79, +probe 45.79, +dedupe 60.35, +pair append 63.07, +radix/scatter 72.46 ns/token; 79.2% query hit rate; hardware perf counters unavailable |
| Six scan workers | 13.41 → 10.49 | Reverted | Three-run scoreboard: min total 1,339.705 ms, peak RssAnon 178,884 KB; 6-way scan is slower and larger than 12 workers; hash and quick fixtures passed |
| Unified list/bitmap word loop | 13.41 → 12.85 | Reverted | Three-run scoreboard: min total 1,000.208 ms, peak RssAnon 139,348 KB, S=965; reduced lines but query time regressed and estimated K fell; references and hash passed |
| Release each pair buffer after its scatter | 13.41 → 13.34 | Reverted | Three-run scoreboard: min total 936.890 ms and peak RssAnon 131,116 KB; K fell 0.5%, short of the keep threshold; fixture and seed-1 hash passed |
| Query-class CPU profile | 13.41 → 13.41 | Diagnostic | One full run: BOOL list 127.9 ms, BOOL bitmap 679.2 ms, PHRASE 1,003.7 ms summed thread CPU; phrase work dominates, full hash passed |
| Slice ASCII case-insensitive phrase compare | 13.41 → 12.96 | Reverted | Three-run scoreboard: min total 971.063 ms, peak RssAnon 139,024 KB; full hash and fixture/sample checks passed, K fell |

## Memory design note
1. Baseline ruler: Docker `memory.peak` is 146,067,456 B; board M is 161 MB; avoidable contributors include pair storage, hot bitmaps, and query-only overlap.
2. (a) For 25M cold memberships, 128-doc bit-packed delta blocks at 1.3-1.5 B/member imply 32.5-37.5 MB payload plus a directory; hot bitmaps add roughly 32 MB unless their break-even is enforced.
3. With directory and 5-10 MB of live tables, (a) estimates 70-80 MB peak and removes the largest cold-posting allocation.
4. At 20% slower time, M=75 MB and S=1,200 imply relative K about 1.015; at no time loss, relative K is about 1.111.
5. (b) Four u16 doc-id partitions need 2 B/member, or about 50 MB for 25M memberships, plus per-term partition offsets.
6. Eliminating global bitmaps gives an estimated 60-75 MB peak for (b); four sorted runs add merge work, likely less query CPU than block decoding but more than u32 lists.
7. At a 5% time increase, M=70 MB and S=1,100 imply relative K about 1.114; smaller memory savings can fail the 5 MB gate.
8. (c) Freeing scan-only lookup tables and query input after scatter should be nearly time-neutral, but lowers peak only if the measured peak is in query evaluation.
9. A 5 MB reduction at unchanged time and S changes K by only about 0.7% near 146 MB; it cannot alone meet the 60 MB target.
10. First measure phase-boundary RssAnon and Docker peak, then test (c) because it is smallest and exact; if peak is scan-bound, pursue (b) before block decoding.

| Scatter pair-buffer ownership | Full-scale ext4-backed Docker, min-of-5 total 822.198 ms vs baseline 970.155 ms; memory.peak max 146,595,840 B vs baseline 146,563,072 B. Timing passed, memory failed the required -5 MB gate; reverted/not shipped. Peak remained at scan/scatter. |
| Bitmap list/bitset break-even from sampled df | Full-scale ext4-backed Docker, min-of-5 total 835.818 ms; memory.peak max 146,792,448 B. Timing passed, memory worsened and missed the -5 MB gate; reverted/not shipped. Output hash matched seed 1. Peak remained at `pairs_pre_drop` (RssAnon 138,780-139,080 KB); query peak was lower (93,232-93,848 KB), so scan-only state was not freed in query. |

## Follow-up interleaved round
1. The five-run baseline/pair/bitmap session used the same warm ext4 Docker volume, CPU set and flags; baseline T min/median = 951.967/990.752 ms and M median/max = 146,227,200/146,837,504 B.
2. Pair ownership T min/median = 911.846/963.464 ms and M median/max = 146,022,400/146,530,304 B; S 998→1,000 gives estimated K ×1.0217, below the 3% keep threshold.
3. Bitmap break-even T min/median = 878.401/942.884 ms and M median/max = 146,321,408/147,185,664 B; S 998→1,000 gives estimated K ×1.0406, so it clears the updated 3% score gate despite no memory reduction.
4. The pair-ownership and bitmap-threshold variants each built offline and passed the worked example, phrase-boundary fixture and seed-1 full hash; B sample seeds 2–5 were absent from the workspace and Docker volumes, so those reference checks remained unrun.
5. Radix sorting already runs one worker at a time (`for scan in &mut scans { radix_sort_pairs(..) }`); baseline measured memory.peak median/max 146,227,200/146,837,504 B, so 12 simultaneous radix temporaries are not present to remove.
6. Four-partition u16 lists use the 250,000-doc corpus's four 65,536-ID ranges, but candidate T min/median = 979.931/995.193 ms and M median/max = 149,491,712/149,626,880 B against its interleaved baseline 920.725/1,276.493 ms and 146,255,872/146,595,840 B; S 998→1,038 yields K ×0.9755, so reverted.

## Block-compressed postings design note (before implementation)
1. Encode each cold posting list in 128-document blocks; a block stores its base doc, count, width, and packed local deltas (0–127), with a per-term block directory for skip-to-block lookup.
2. Count then fill exact block payloads in two corpus passes; each worker owns its temporary block counters and writes a disjoint term/block range, so no global pair buffer or scatter overlap is retained.
3. Dense terms stay exact bitmaps; the sampled threshold selects representation only, and full scans determine every output bit or posting.
4. At 25M cold memberships, fixed 7-bit deltas are about 21.9 MB; allow 8–16 MB for block headers/directories and alignment.
5. Retain the existing query table, docs mapping, hot bitmaps, and worker state at roughly 35–50 MB; expected steady peak is 65–90 MB if count/fill buffers stay under 5 MB.
6. Two full token scans and block packing add an estimated 35–70% to scan CPU; block decode in BOOL/PHRASE may add 10–30% query CPU, giving total T roughly 1.3–1.7× baseline until measured.
7. Using M=80 MB and T=1.5× predicts K ×0.92 before line-count change; it is a memory probe, not a presumed score win, and will be kept only if measured K wins.
8. The main risk is directory overhead for sparse terms; if it exceeds 16 MB, store only non-empty blocks with delta-coded block IDs and preserve binary-search skips.
9. Keep list/bitmap BOOL evaluation left-to-right; decode one block per side into fixed scratch arrays and never expand whole postings lists for a merge.
10. Reject if peak remains above 100 MB without a ≥3% estimated-K gain; report actual peak location and time even if reverted.

## Interleaved follow-up decisions
- The latest scoring instruction changes the keep gate from the earlier absolute 5 MB memory-drop gate to estimated K ≥3% using same-session min T and median `memory.peak`; the earlier pair/bitmap “reverted” rows record the then-active gate, while the decision below uses the updated gate.
- Pair ownership: same-session T min/median 911.846/963.464 ms, M median/max 146,022,400/146,530,304 B, S=1,000; relative K ×1.0217, so rejected.
- Bitmap break-even: same-session T min/median 878.401/942.884 ms, M median/max 146,321,408/147,185,664 B, S=1,000; relative K ×1.0406, so selected. Full seed-1 and both fixtures pass. Local cgroup max is 147,185,664 B; the no-timing final-source check measured memory.peak 146,309,120 B (anon 225,280; file 1,511,424; kernel 585,728; shmem 0 B after exit).
- The selected bitmap candidate's sampled df chooses only the representation. Its observed RssAnon peak is in the scan/pair-scatter phase; query-complete RssAnon is about 93 MB, below scan. No scan-state free was needed for this selected version.
- Radix temp-buffer change: no change was needed. The baseline already sorts one worker buffer at a time, and the interleaved baseline M median/max was 146,227,200/146,837,504 B; there are not 12 simultaneous radix temporaries.
- Four-run u16 lists: full hash passed, but interleaved baseline T min/median 920.725/1,276.493 ms and M median/max 146,255,872/146,595,840 B versus candidate 979.931/995.193 ms and 149,491,712/149,626,880 B; S=1,038, relative K ×0.9755, reverted.
- 128-doc packed blocks: 7,823,302 block records and 91,177,666 encoded bytes; full hash/fixtures passed, but a diagnostic run reached 176,721,920 B `memory.peak` and 1,245.011 ms, reverted.
- 1,024-doc packed blocks: 4,449,060 records and 66,897,565 bytes; after removing per-block allocations and freeing scan-only state before query, one diagnostic run was 1,217.181 ms and 141,996,032 B `memory.peak`, reverted.
- 8,192-doc packed blocks: 1,647,520 records and 37,632,458 bytes; interleaved baseline T min/median 986.838/1,053.900 ms, M median 146,272,256 B; candidate T min/median 1,132.241/1,202.277 ms, M median/max 108,462,080/108,986,368 B, S=1,141, estimated K ×0.9427, reverted. Full hash/fixtures passed; scan/pair conversion set the peak, and query RssAnon fell from about 99.9 MB to 94.9 MB after releasing ScanTable, hot tables, hot ids, chunk metadata, and query text.
- Track B sample/reference inputs for seeds 2–5 are absent (`data/s2` and `data/s3` contain Track A `requests.log`; no B `docs.txt`/`queries.txt` or reference outputs exist in the workspace or Docker volumes). Those checks remain outstanding; no run IDs or PERF rows were fabricated because nothing was pushed.
