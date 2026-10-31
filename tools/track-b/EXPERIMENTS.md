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
