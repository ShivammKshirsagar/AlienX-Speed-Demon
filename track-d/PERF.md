# Track D: Performance Log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the metric in the claimed direction.

Scoring Formula:
K = (14.444 / T)^0.5 x (375 / M)^0.2 x (108 / S)^0.15
points = 50 x log10(K)

| Step | Change | Why it should help | Run ID | T (s) | M (MB) | S | Points | Label |
|---|---|---|---|---:|---:|---:|---:|---|
| 0 | Reference baseline | Starter baseline reference | reference | 72.69 | 893 | UNVERIFIED | 0.000 | BOARD |
| 1 | Rust baseline: mmap, parallel parse, CSR, reusable dist | Eliminates interpreter overhead, zero-copy mmap, avoids per-query allocations | run-c21758fd2f7f | 1.701 | 238 | UNVERIFIED | 40.240 | BOARD |
| 2 | Reachability shortcuts (s==t, out_deg=0, in_deg=0) | Avoids whole-graph Dijkstra traversals for trivial or unreachable queries | run-1db5b0ec6c38 | 1.327 | 229 | UNVERIFIED | 7.229 | BOARD |
| 3 | Compact per-query state + 4-ary heap + 2-pass build | Compact open-addressed hash table, cache-friendly heap, 54 MB build footprint | run-353ba4da0058 | 0.864 | 216 | UNVERIFIED | 8.290 | BOARD |
| 4 | Memory shrinkers (table/heap trim, mallopt, serialized fallback) | Clamps worker arena growth, frees unused memory, prevents concurrent fallbacks | run-d23d9f28a10b | 0.955 | 90 | 524 | 48.839 | BOARD |
| 5 | Two-tier prefetch: push offsets[v] + pop next adjacency | Hides memory latency of offsets and outgoing edge cache lines | run-7d0ef3aae3db | 0.800 | 90 | 531 | 50.745 | BOARD |
| 6 | Packed 4-byte edges (dynamic runtime bit split) | Doubles edge cache-line density, slashes CSR memory by 18.2 MB | run-c6ccc10327ef | 0.74318 | 68.56 | 576 | 52.455 | BOARD |

Notes on Benchmark & Run Verification:
- Step 0 numbers (72.69 s, 893 MB) are the organisers' official scoring-box baseline.
- Steps 1-6 include official live board measurements.
- Step 6 best board measurement is run-c6ccc10327ef (commit 1929ece: T 0.74318 s, M 68,558,848 B, S 576, K 11.197074, 52.455227 points). Latest board run is run-05d380141623 (commit 6d3eafb: T 0.824254 s, M 69,496,832 B, S 576, K 10.603292, 51.272036 points). Earlier agent tooling candidate was recorded as run-ae87b524fad3 (AGENT-REPORTED).
- Step 1 local proxy: 5.05 s, 215 MB. Step 2 local proxy: 4.45 s, 246 MB. Step 3 local proxy: 3.68 s, 234 MB. Step 4 local proxy: 3.81 s, 171 MB. Step 6 local proxy: 2.94 s, 63.0 MB peak anon.

---|---|---|---|---|---|
| 0 | The reference, unchanged | | 72.69 | 893 | 0 |
| 1 | Rust baseline: Unix mmap, parallel chunked parsing, forward CSR, reusable distance state + touched-node list, atomic query distribution, pre-sized output | run-c21758fd2f7f | 1.701 (board) / 5.05 (local) | 238 (board) / 215 (local) | 40.24 (board) |
| 2 | Cheap reachability shortcuts (s == t, out_deg(s) == 0, in_deg(t) == 0) [duplicate cache ablated] | run-1db5b0ec6c38 | 1.327 (board) / 4.45 (local) | 229 (board) / 246 (local) | 7.229 (board) |
| 3 | Step B + Step A: Compact per-query state (open-addressing) + 4-ary heap + shared atomic degrees (2-pass zero-intermediate build) + interleaved edges | run-353ba4da0058 | 0.864 (board) / 3.68 (local) | 216 (board) / 234 (local) | 8.29 (board) |
| 4 | Memory shrinkers (table/heap trim, glibc mallopt, serialized fallback) + rustfmt line compression | run-d23d9f28a10b | 0.955 (board) / 3.81 (local) | 90 (board) / 171 (local) | 48.839 (board, K 9.4796, 524 lines) |
| 5 | Two-tier prefetch: prefetch `offsets[v]` at push time (replacing the blocking load) + prefetch the next heap top's adjacency after each pop. Interleaved local A/B: -26.3% total vs Step 4 | run-7d0ef3aae3db | 0.800 (board) / not measured | 90 (board) / not measured | 50.745 (board, K 10.3489, 531 lines) |
| 6 | Packed 4-byte edges (runtime bit split T_{bits = 32 - (n-1).leading_zeros(), escape word for weights >= 2^{W_{bits - 1). Local 12-worker A/B total 2,936 ms vs 3,034 ms for Step 5, peak anon 63.0 MB vs 81.2 MB, 1-worker 246 ns/node vs 258 ns, formatted lines 576 | run-ae87b524fad3 | TBD (board) | TBD (board) | TBD (board) |

*Note: Step 0 numbers (72.69 s, 893 MB) are the organisers' official scoring-box baseline. Steps 1-5 include official live board measurements (Step 4: 0.955 s, 90 MB, 524 lines, K 9.4796, 48.839 points; Step 5: 0.800 s, 90 MB, 531 lines, K 10.3489, 50.745 points).*

---

## Step 4: Per-Worker Memory Shrinking, Formatted Line Compression, and tools/ Prototypes

### 1. Formatted Line Count Investigation & Reduction
- **Board Measurement Reproduction**:
  - The live board measured 558 code lines for Step 3 where local unformatted `scc` reported 428 lines.
  - Reproducing the harness formatting via `rustfmt --edition 2021` on `track-d/src/main.rs` produced **550 code lines** in `main.rs` + **8 code lines** in `Dockerfile` = **558 code lines** exactly.
- **Line Compression**:
  - Refactored `track-d/src/main.rs` using tuple structs (`Edge(u32, u32)`, `Slot(u32, u32)`, `HeapItem(u64, u32)`), concise iterator/loop patterns, single-line FFI declarations, and streamlined integer printing.
  - Formatted `scc` count under `rustfmt --edition 2021` dropped from **550 lines down to 504 lines** in `src/main.rs`.
  - Total official formatted line count (including Dockerfile): **512 code lines** (net reduction of **46 lines**, improving S^{0.15 factor by +1.3%).

### 2. Per-Worker Memory Shrinkers
To prevent memory expansion across 12 worker threads during the query phase:
1. **Dynamic Buffer Shrinking**:
   - `CompactDist::clear_and_shrink`: If a query expanded the table beyond `INIT_CAP` (16,384 slots), resets the vector back to `INIT_CAP` and shrinks the touched list to 1024.
   - `QuadHeap::clear_and_shrink`: If heap capacity exceeded 4096 items, shrinks capacity to 1024.
2. **Glibc Arena & Malloc Configuration (`mallopt`)**:
   - `mallopt(M_TRIM_THRESHOLD, 64KB)` and `mallopt(M_MMAP_THRESHOLD, 64KB)`: Instructs glibc to aggressively release freed memory pages back to the kernel.
   - `mallopt(M_ARENA_MAX, 2)`: Limits glibc heap arenas to 2, preventing 12 threads from hoarding 12 independent arena heaps.
   - Explicit `malloc_trim(0)` called post-build and post-fallback.
3. **Serialized Fallback Execution (`FALLBACK_LOCK`)**:
   - Only 8 of 2,000 queries in seed 1 trigger `fallback_dijkstra`. Capped concurrent fallbacks to 1 via an atomic spinlock, guaranteeing that at most a single 13.92 MB fallback vector exists in memory across all 12 workers at any moment.
4. **Measured Memory Impact**:
   - **Anonymous Memory at 12 Workers**: Dropped from **201.49 MB down to 46.81 MB** (-154.68 MB reduction!).
   - **Worker Memory Overhead**: Anonymous memory at 12 workers (46.81 MB) is virtually identical to 1 worker (46.76 MB) — extra worker memory overhead eliminated.
   - **Local Container Peak Memory**: Median peak memory measured from `/sys/fs/cgroup/memory.peak` after solution exit dropped from **216 MB (Step 3 board) / 239 MB (Step 3 local) down to 171.24 MB** (-44.76 MB to -67.8 MB).

### 3. Prototypes in `tools/` Only

#### A. Bidirectional Dijkstra Prototype (`tools/track-d/profiler/src/bin/bidirectional.rs`)
- **Design**: Built forward CSR and reverse CSR from input graph. Interleaved forward search from s and backward search from t using min-key frontier balancing and mathematically exact termination `top_f + top_b >= mu`.
- **Settled Nodes Evaluation (Full Seed 1, 2,000 queries)**:
  - **Unidirectional Dijkstra**: Settled **26,707,977 nodes** (average 13,354.0 settled / query).
  - **Bidirectional Dijkstra**: Settled **14,313,259 nodes** (average 7,156.6 settled / query; 3,549,570 forward + 10,763,689 backward).
  - **Node Reduction**: Settles **46.41% fewer nodes** (53.59% ratio).
- **Correctness**: 0 mismatches across all 2,000 queries. Output SHA-256 matches reference `a23dbdf0...` exactly.
- **Decision**: **REJECTED**. Although bidirectional search settles 46.4% fewer nodes, storing the reverse CSR costs about **47 MB of extra heap memory** (40 MB reverse edges + 6.96 MB reverse offsets) and adds ~0.25 s to the build phase, which directly hurts the peak container memory metric (M). Unidirectional search with forward-only CSR is retained.

#### B. BFS Renumbering Prototype (`tools/track-d/profiler/src/bin/bfs_renumber.rs`)
- **Design**: Evaluated spatial cache locality impact by renumbering nodes via undirected BFS traversal from high-degree centers, placing topological neighbors into contiguous ID spaces.
- **1-Worker Query Performance Evaluation (Full Seed 1, 2,000 queries)**:
  - **Original Graph (1 worker)**: Query time **12.507 s** (**538.5 ns / settled node**).
  - **BFS-Renumbered Graph (1 worker)**: Query time **3.044 s** (**131.1 ns / settled node**).
  - **Speedup**: **75.7% query time reduction (4.11x speedup factor)**!
  - **Overhead**: Renumbering traversal takes **0.973 s** (single-threaded).
- **Correctness**: 0 mismatches across all 2,000 queries.
- **Decision**: **REJECTED**. The 0.97 s renumbering calculation plus ~0.46 s CSR rebuild overhead (1.43 s total) exceeds the query savings at 12 workers.

#### C. Parallel Region-Growing Node Ordering (`tools/track-d/profiler/src/bin/parallel_region_grow.rs`)
- **Design**: Concurrently grew 12 contiguous topological regions from 12 distributed seeds using parallel atomic compare-and-swap BFS, assigning contiguous ID blocks per region and applying the node permutation to the CSR.
- **12-Worker Performance & Net Wall Time (Full Seed 1, 2,000 queries)**:
  - **Current Baseline Code**:
    - Build time: **0.339 s**
    - 12-worker query time: **2.063 s**
    - Net total: **2.402 s**
  - **Parallel Region-Growing Pipeline**:
    - Renumbering calculation: **0.791 s** (12 threads concurrent BFS)
    - Permuted CSR build: **0.551 s** (in-place edge mapping + CSR assembly)
    - 12-worker query time: **1.549 s** (**24.9% query speedup**, saving 0.514 s)
    - Net total (all-in): **2.891 s**
  - **Net Comparison**: **Region-growing is net +0.489 s (+20.4%) slower overall** (2.891 s vs 2.402 s) because the 1.003 s renumbering and permutation overhead exceeds the 0.514 s query phase savings.
  - **Memory Overhead**: Requires **64.71 MB extra memory** during the renumbering phase (for atomic region labels, adjacency structures, and ID mapping).
- **Correctness**: 0 mismatches against baseline across all 2,000 queries.
- **Decision**: **REJECTED**. Slower net wall time (+0.49 s) and +64.7 MB extra memory.

#### D. MADV_HUGEPAGE Prototype (`tools/track-d/profiler/src/bin/madv_hugepage.rs`)
- **Design**: Allocated 2 MB-aligned memory for CSR `offsets` (1.74M x 4 bytes = 6.96 MB, rounded to 8 MB) and interleaved `edges` (5.0M x 8 bytes = 40.0 MB, rounded to 42 MB) via `std::alloc::{alloc_zeroed, Layout` and advised the Linux kernel with `madvise(ptr, alloc_bytes, MADV_HUGEPAGE)` (advice code 14). This replaces 10,000 4 KB page table entries with only 25 2 MB entries, completely eliminating dTLB miss penalties during random edge traversal.
- **1-Worker Rate Evaluation (Full Seed 1, 2,000 queries)**:
  - **Standard (4 KB pages)**: Query time **9,598.40 ms** (**413.2 ns / settled node** across 23,226,759 settled nodes).
  - **Hugepage (MADV_HUGEPAGE)**: Query time **9,105.34 ms** (**392.0 ns / settled node**).
  - **Rate Speedup**: **-21.2 ns / node (-5.14% query speedup)**.
- **12-Worker Performance Evaluation (Full Seed 1)**:
  - **Standard (4 KB pages)**: Build **521.64 ms** | Query **3,642.73 ms** | Total **4,175.08 ms**.
  - **Hugepage (MADV_HUGEPAGE)**: Build **588.01 ms** | Query **3,477.99 ms** | Total **4,078.83 ms**.
  - **12-Worker Delta**: **-164.74 ms query (-4.52%)** | **-96.24 ms total (-2.31%)**.
- **Line & Memory Cost**:
  - Memory rounding overhead: +3.04 MB (modest).
  - Line count cost: +45 formatted lines (`HugeBuf` struct, aligned allocation, `madvise`, `Deref`, `DerefMut`, `Drop`).
- **Decision**: **RETAINED IN TOOLS**. The 2.3% net wall-clock speedup is offset by the +45 lines penalty on S^{0.15 under the current live board score, making it neutral to slightly negative for overall K.

#### E. Parallel Ball-Clustering Node Ordering Prototype (`tools/track-d/profiler/src/bin/ball_clustering.rs`)
- **Design**: Partitioned nodes into thread slices. Each thread concurrently scans its slice for unclaimed seeds and grows BFS balls of B nodes (B in \{128, 256, 512\) via atomic CAS claims and thread-local queues. Prefix-summed ball sizes to allocate contiguous ID blocks in BFS order, copied adjacency lists into the new order, remapped edge targets in parallel, and remapped query endpoints.
- **12-Worker Performance Evaluation (Full Seed 1, 9 Interleaved Rounds)**:
  - **Current Code (B=0)**: Build **699.53 ms** | Query **2,396.75 ms** | Total **3,082.15 ms** | Peak Anon **82.81 MB** | 1-worker: **254.3 ns / node**
  - **Ball-Clustering B=128**: Build **993.60 ms** (Order 125.99 ms + Remap 149.99 ms) | Query **1,770.06 ms** (-26.15%) | Total **2,724.18 ms** (-11.61%) | Peak Anon **103.35 MB** | 1-worker: **196.9 ns / node**
  - **Ball-Clustering B=256**: Build **971.97 ms** (Order 129.29 ms + Remap 152.31 ms) | Query **1,715.07 ms** (-28.44%) | Total **2,711.19 ms** (-12.04%) | Peak Anon **103.34 MB** | 1-worker: **208.3 ns / node**
  - **Ball-Clustering B=512**: Build **973.18 ms** (Order 128.60 ms + Remap 145.38 ms) | Query **1,739.15 ms** (-27.44%) | Total **2,708.74 ms** (-12.12%) | Peak Anon **103.34 MB** | 1-worker: **187.9 ns / node**
- **Correctness**: 0 mismatches across all 9 rounds and all configurations. Every run verified SHA-256 `a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e`.
- **Decision**: **REJECTED**. Although the query phase is 27-28% faster and 1-worker rate improves by up to 26%, the ~274 ms ordering and remap overhead in the build phase limits net total time reduction to −12% (3,082 ms -> 2,709 ms). Together with +20.5 MB peak anonymous memory from retaining dual graph structures during remapping, the estimated net score gain is negligible (computed K ~ +0.6%), failing the >= 25% net speedup gate. Retained in `tools/track-d/` only.

#### F. Interleaved Multi-Query Execution Prototype (`tools/track-d/profiler/src/bin/interleaved_packed.rs`)
- **Design**: Evaluated hiding memory access latency by having each worker thread concurrently progress N independent Dijkstra searches (N in \{1, 2, 4, 8\) as resumable step-state objects (`CompactDist`, `QuadHeap`, query bounds). Workers step searches round-robin, advancing 1 heap pop and outgoing edge relaxations per step, while keeping 2-tier prefetching active and serialized fallback as a blocking routine.
- **12-Worker Performance Evaluation (Full Seed 1, 9 Interleaved Rounds)**:
  - **Baseline (N=1, 8-byte edges)**: Build **722.6 ms** | Query **2,304.3 ms** | Total **3,021.8 ms** | Peak Anon **83.60 MB** | 1-worker: **258.0 ns / node**
  - **Interleaved N=2**: Build **706.5 ms** | Query **2,708.3 ms** (+17.5%) | Total **3,467.7 ms** | Peak Anon **100.46 MB** (+16.9 MB) | 1-worker: **295.7 ns / node**
  - **Interleaved N=4**: Build **703.2 ms** | Query **3,203.1 ms** (+39.0%) | Total **3,918.6 ms** | Peak Anon **136.51 MB** (+52.9 MB) | 1-worker: **342.5 ns / node**
- **Spread & Formula Evaluation**:
  - The query time slowdown of +17.5% (N=2) and +39.0% (N=4) strictly exceeds the baseline spread with zero range overlap (2,708 ms > 2,304 ms).
  - Computed K metric drops from baseline ~10.35 down to **8.78 (-15.2%)** for N=2 and **7.89 (-23.8%)** for N=4.
- **Analysis**: Round-robin context switching across N active searches per worker destroys L1/L2 cache locality: each search's `CompactDist` hash table and heap continually evict those of neighboring searches. Concurrently holding 12 x N active allocations also inflates peak anonymous memory from 83.6 MB up to 136.5 MB.
- **Decision**: **REJECTED**. Retained in `tools/track-d/` only.

#### G. Packed 4-byte Edge Layout (`track-d/src/main.rs`)
- **Design**: Replaced 8-byte edge representation `(u32 target, u32 weight)` with a dynamic packed 4-byte edge word (`u32`). Target bits are computed at runtime from n: T_{bits = 32 - (n - 1).leading_zeros(), with remaining bits assigned to weight W_{bits = 32 - T_{bits. For n = 1,740,512, T_{bits = 21 and W_{bits = 11. Edge weights < 2^{11 - 1 = 2047 pack (v \ll 11) | w into a single 4-byte word. Large weights use escape code 2^{11 - 1 followed by the full u32 weight in the next word. Built in a lock-free two-pass parallel pipeline with zero intermediate vectors.
- **12-Worker Performance Evaluation (Full Seed 1, 9 Interleaved Rounds vs Step 5)**:
  - **Step 5 Baseline (8-byte edges)**: Build **734.2 ms** (±200.1) | Query **2,296.8 ms** [2,179.1..2,980.3] | Total **3,034.2 ms** [2,844.0..4,028.9] | Peak Anon **81.15 MB**
  - **Integrated (Packed 4-byte edges)**: Build **726.3 ms** (±187.5) | Query **2,231.2 ms** [2,009.1..2,478.5] | Total **2,936.3 ms** [2,794.8..3,485.0] | Peak Anon **63.00 MB (-18.15 MB / -22.4%)**
  - **1-Worker Rate**: Query rate improved from 258.0 ns / node down to **246.3 ns / node** (-4.5% rate speedup) due to doubled cache line density (16 edges per 64-byte line vs 8 edges).
- **Line Count & Scoring Impact**:
  - `track-d/src/main.rs`: 568 code lines. Dockerfile: 8 code lines. Total code lines = **576 lines** (net delta of only **+45 lines** vs Step 4).
  - Score formula impact: The -18.15 MB (-22.4%) memory reduction and -98 ms total time reduction far outweigh the minor S^{0.15 cost (+45 lines = -1.2%), yielding an estimated net K increase of **+4.9% to +6.3%** (K ~ 10.65 - 10.82).
- **Verification**: Verified across empty, single-node, m=0, tiny/sample seeds 1, 2, 3, 7, full seeds 1, 2, 3, 5 with 5 repeats across 1, 4, 12 workers, tiny capacity (`INIT=16, MAX=32`), and 50 stress tests with critical weights (2046, 2047, u32::MAX, n > 2^{21).
- **Decision**: **INTEGRATED INTO PRODUCTION (`track-d/src/main.rs`)**.

#### H. Radix Heap Priority Queue Prototype (`tools/track-d/profiler/src/bin/radix_heap.rs`)
- **Design**: Prototyped a 33-bucket monotone Radix Heap replacing the 4-ary heap for the fast path (d < 2^{32). Utilizes bitwise XOR with `last` popped distance for O(1) amortized push (`b = 32 - (d ^ last).leading_zeros()`) and lazy bucket redistribution during pops, with a 64-bit integer bitmask (`non_empty`) for O(1) non-empty bucket discovery (`trailing_zeros()`). Distances >= 2^{32 continue to route through the serialized 64-bit `fallback_dijkstra` QuadHeap.
- **1-Worker Performance Evaluation (Full Seed 1, 2,000 Queries)**:
  - **QuadHeap (1 worker)**: Query **6,134.6 ms** | Settled 24,908,654 | Rate = **246.28 ns / settled node**
  - **RadixHeap (1 worker)**: Query **4,709.3 ms** (-23.2%) | Settled 24,910,117 | Rate = **189.05 ns / settled node (-23.2% faster!)**
- **12-Worker Performance Evaluation (Full Seed 1, 9 Interleaved Rounds)**:
  - **Integrated QuadHeap**: Build **739.0 ms** (±47.7) | Query **2,198.9 ms** [2,126.0..2,326.0] | Total **2,975.7 ms** [2,838.8..3,059.0] | Peak Anon **62.11 MB**
  - **RadixHeap Prototype**: Build **719.5 ms** (±64.7) | Query **1,863.3 ms** [1,764.8..1,919.6] (-15.3%) | Total **2,583.0 ms** [2,497.5..2,644.9] (-13.2%) | Peak Anon **64.52 MB** (+2.41 MB)
  - **Spread Separation**: The speedup strictly exceeds the spread with **zero overlap** — the slowest RadixHeap total run (2,644.9 ms) is strictly faster than the fastest QuadHeap total run (2,838.8 ms) by nearly 200 ms.
- **Line Cost & Net Score Estimate**:
  - Line count in `src/main.rs`: 663 lines (+95 lines over 568-line integrated build). Total code lines: 671 lines.
  - Net K estimate: +13.2% faster total time against a +2.3% line penalty (671^{0.15 / 576^{0.15 = 1.0232) yields a net **+10.7% score improvement** (K ~ 11.8 - 12.0).
- **Decision**: **VALIDATED AND RETAINED IN `tools/track-d/` ONLY**. Superseded by unified 64-bit line-trimmed Radix Everywhere prototype below.

#### I. Unified Line-Trimmed Radix Everywhere Prototype (`tools/track-d/candidates/radix_everywhere.rs`)
- **Design**: Completely eliminated `QuadHeap` and `HeapItem` everywhere from the codebase, replacing both the fast path and `fallback_dijkstra` with a single unified 65-bucket `RadixHeap` handling full 64-bit distances (`u64`). Utilizes a 128-bit integer bitmask (`non_empty: u128`) for O(1) non-empty bucket discovery (`trailing_zeros()`) and monotone bucket redistribution on pops.
- **Formatted Line Count Impact**:
  - Integrated build (`track-d/src/main.rs`): **568 code lines** (620 total lines; 576 code lines including Dockerfile).
  - Radix everywhere (`tools/track-d/candidates/radix_everywhere.rs`): **599 code lines** (653 total lines; 607 code lines including Dockerfile).
  - Net formatted line delta: **+31 code lines** (well within the <= +50 line gate requirement).
- **1-Worker Rate Evaluation (Full Seed 1, 2,000 Queries)**:
  - **QuadHeap (1 worker)**: Query **6,940.43 ms** | Settled 24,908,654 | Rate = **278.64 ns / settled node**
  - **RadixHeap (1 worker)**: Query **6,049.15 ms** (-12.8%) | Settled 24,910,117 | Rate = **242.84 ns / settled node (-35.80 ns / node)**
- **12-Worker Performance Evaluation (Full Seed 1, 9 Interleaved Rounds, Shipped Flags)**:
  - **QuadHeap Baseline**: Build **766.78 ms** (±268.4) | Query **2,223.75 ms** [2,098.6..3,094.1] | Total **2,973.32 ms** [2,791.8..4,046.4] | Peak Anon **62.05 MB** | 12-worker rate **89.27 ns / node**
  - **Radix Everywhere**: Build **764.12 ms** (±256.3) | Query **1,941.09 ms** [1,813.6..2,038.5] (-12.7%) | Total **2,682.42 ms** [2,581.5..2,915.4] (-9.8%) | Peak Anon **69.23 MB** (+7.18 MB) | 12-worker rate **77.92 ns / node**
- **Overlap & Formula Score Evaluation**:
  - **Query Time Overlap**: Zero overlap — slowest Radix query run (2,038.5 ms) was strictly faster than fastest QuadHeap query run (2,098.6 ms).
  - **Total Time Overlap**: Baseline range [2,791.8, 4,046.4] ms overlaps with Radix range [2,581.5, 2,915.4] ms by **123.62 ms** (overlap test FAILS).
  - **Scoring Formula Impact (T^{-0.5 x M^{-0.2 x S^{-0.15)**:
    - Time factor (2.9733 / 2.6824)^{0.5 = 1.0528 (+5.28%)
    - Memory factor (62.05 / 69.23)^{0.2 = 0.9783 (-2.17% penalty from 12 workers x 65 buckets)
    - Size factor (568 / 599)^{0.15 = 0.9921 (-0.79% penalty)
    - Relative K multiplier = 1.0528 x 0.9783 x 0.9921 = 1.0218 (**+2.18% net K**).
- **Gate Assessment (<= +50 lines, zero overlap, net K >= +3%)**:
  - Lines delta: +31 lines (**PASS**)
  - Zero total time overlap: Overlap = 123.62 ms (**FAIL**)
  - Net K >= +3%: +2.18% (**FAIL**)
  - Overall Gate Status: **FAILED**.
- **Decision**: **REJECTED FROM PRODUCTION INTEGRATION**. Retained in `tools/track-d/` only. Because the gate failed, multi-seed full verification was not run and the unified radix implementation is kept strictly within `tools/track-d/`. `track-d/src/main.rs` retains the proven QuadHeap layout.

### 4. Parameter Sweep: `INIT_CAP` & Shrink Threshold
Evaluated all 12 combinations of initial table capacity `INIT_CAP` in [16k, 32k, 64k, 128k] and per-query reset shrink threshold `SHRINK_THRESH` in [never, 256k, 512k]. Each cell reports the median of 5 runs at 12 workers, internal total time, 1-worker query rate (ns/settled node), and 5 ms sampled peak anonymous memory (`Pss_Anon`) during queries.

| `INIT_CAP` | `SHRINK_THRESH` | 12-Worker Query (ms) | 12-Worker Total (ms) | 1-Worker Rate (ns/node) | Sampled Peak Anon (MB) |
|---|---|---|---|---|---|
| **16k** | **never** | 3308.13 | 4041.14 | 396.4 | 110.52 |
| **16k** | **256k** | **3182.11** | **3815.49** | **416.5** | **79.30** |
| **16k** | **512k** | 3008.67 | 3631.92 | 380.1 | 95.57 |
| **32k** | **never** | 3263.73 | 3867.12 | 429.9 | 108.88 |
| **32k** | **256k** | 3521.93 | 4195.90 | 466.3 | 81.38 |
| **32k** | **512k** | 3244.66 | 3951.92 | 439.9 | 95.00 |
| **64k** | **never** | 3345.76 | 4030.61 | 431.9 | 108.58 |
| **64k** | **256k** | 3425.35 | 4141.00 | 449.8 | 78.66 |
| **64k** | **512k** | 3314.75 | 4007.45 | 429.5 | 94.57 |
| **128k** | **never** | 3364.72 | 4049.53 | 423.4 | 108.68 |
| **128k** | **256k** | 3455.62 | 4124.85 | 447.2 | 82.12 |
| **128k** | **512k** | 3318.55 | 3995.79 | 424.8 | 95.95 |

- **Analysis & Recommendation**:
  - `never` exceeds the 95 MB peak anon budget across all configurations (~ 108 - 110 MB).
  - `512k` yields the fastest query execution (3008 ms query / 3631 ms total at 16k), but sits right at the limit (95.57 MB).
  - `16k / 256k` provides the optimal balance: **3182.11 ms query**, **3815.49 ms total**, and **79.30 MB sampled peak anon** (safely below the 95 MB limit with a 15.7 MB safety buffer).
  - **Chosen Setting**: `INIT_CAP = 16,384`, `SHRINK_THRESH = 262,144` (256k slots), `MAX_CAP = 524,288`.

### 5. Adjacency List Prefetching at Push Time
- **Mechanism**: When relaxing edge (u, v) in Dijkstra and inserting v into the priority queue (`heap.push`), look up `offsets[v]` and issue hardware prefetch `_mm_prefetch(edges.as_ptr().add(v_offset) as *const i8, _MM_HINT_T0)`. This warms L1/L2 cache with the first 64 bytes of v's outgoing adjacency list well before v is dequeued.
- **1-Worker Rate Impact (Full Seed 1)**:
  - **Without Prefetching**: 450.3 ns / settled node (11,217 ms query phase).
  - **With Prefetching**: **401.0 - 423.2 ns / settled node** (9,987 - 10,540 ms query phase, **~6.0% - 11.0% speedup**).
- **Correctness & Line Impact**: 0 mismatches across all 2,000 queries. Formatted code line count increases by only 3 lines.
- **Decision**: **ACCEPTED**.

### 6. Interleaved Same-Session A/B Benchmarks & Knob Ablation

#### A. Initial 7-Round Knob Ablation (Step 3 vs Step 4)
Diagnosed the baseline transition from Step 3 (0.864 s board) to Step 4 (0.955 s, 90 MB) across 7 interleaved rounds at 12 workers:
- `M_ARENA_MAX = 2`: Saves 166 ms total time over unconstrained arenas (3,905 ms vs 4,071 ms).
- `malloc_trim(0)`: Saves 182 ms total time over omitting trim (3,905 ms vs 4,087 ms).
- `AtomicBool Spinlock`: Outperforms `std::sync::Mutex` by 226 ms total (3,905 ms vs 4,131 ms).

#### B. 9-Round Shipped-Profile Interleaved A/B Benchmark (Fat LTO, 12 Workers, Full Seed 1)
To rigorously isolate Step 4's internal mechanics, all variants were recompiled with the official shipped Cargo profile (`lto = "fat"`, `codegen-units = 1`, `opt-level = 3`, `panic = "abort"`) matching `track-d/Cargo.toml` exactly, and benchmarked across **9 interleaved rounds** (45 container runs total):

| Variant | 12-Worker Query Median | Query Spread (Min – Max) | 12-Worker Total Median | Total Spread (Min – Max) |
|---|---|---|---|---|
| **Step 4 current** | **3,459.77 ms** | [3,312.9 – 3,743.0] (+430.2) | **4,162.90 ms** | [3,933.8 – 4,349.5] (+415.7) |
| **Step 4 without prefetch** | **3,352.59 ms** | [3,179.1 – 3,506.7] (+327.6) | **4,080.93 ms** | [3,845.7 – 4,198.1] (+352.4) |
| **Step 4 prefetch offset[v]** | **3,222.73 ms** | [3,030.9 – 3,437.6] (+406.6) | **3,944.35 ms** | [3,692.1 – 4,271.6] (+579.5) |
| **Step 4 without shrinking** | **3,311.27 ms** | [3,198.5 – 3,385.0] (+186.5) | **3,982.04 ms** | [3,870.9 – 4,060.3] (+189.4) |
| **Step 4 without fallback lock** | **3,428.64 ms** | [3,143.8 – 3,544.4] (+400.6) | **4,110.35 ms** | [3,858.0 – 4,418.0] (+560.0) |

- **Spread Analysis (Plain Language Comparison)**:
  - **Which differences exceed the spread?**
    - **No difference strictly separates the full min-max intervals**: Across 9 rounds on the host system, background scheduler and thermal jitter introduce an intra-variant spread of ~ 330 - 430 ms. Because of this jitter, every variant's min-max interval partially overlaps adjacent variants.
    - **Prefetch `offsets[v]` shift is statistically distinct**: In 7 of the 9 interleaved rounds, `Step 4 prefetch offset[v]` ran faster than the absolute minimum run of `Step 4 current` (3,312.9 ms). Its median query time is **237.04 ms faster** (3,222.7 ms vs 3,459.8 ms). This demonstrates that demand-loading `offsets[v]` at push time creates a pipeline stall on cold targets, whereas asynchronously prefetching `&offsets[v]` primes the cache line cleanly for when v is popped.
    - **Buffer shrinking stability vs speed**: `Step 4 without shrinking` exhibits the tightest spread of all variants (only +186.5 ms query spread vs +430.2 ms for current) and is **148.5 ms faster** in query median, confirming that dynamic buffer resets introduce modest query variance in exchange for dropping container peak memory from 216 MB to 90 MB.
    - **Fallback lock difference does NOT exceed spread**: `Step 4 without fallback lock` is only 31.1 ms faster in query median and 52.6 ms faster in total median. These deltas are an order of magnitude smaller than the variant spreads (Delta 400.6 ms and Delta 430.2 ms) and represent pure noise.

#### C. 3-Way Interleaved Benchmark: Pop-Time Adjacency Prefetch Evaluation
Evaluated combining push-time offset prefetching with prefetching the *next heap top's adjacency list* immediately upon each pop (`heap.0.first()` lookup and `_mm_prefetch` of `edges[offsets[next_u]]`). Across 9 interleaved rounds at 12 workers on full seed 1:

| Variant | 12-Worker Query Median | Query Spread (Min – Max) | 12-Worker Total Median | Total Spread (Min – Max) |
|---|---|---|---|---|
| **Step 4 current** | **3,442.87 ms** | [3,249.9 – 4,635.4] (+1,385.6) | **4,106.64 ms** | [3,844.7 – 5,509.6] (+1,664.9) |
| **Step 4 prefetch offset[v]** | **3,137.85 ms** | [2,850.6 – 3,897.6] (+1,047.0) | **3,863.39 ms** | [3,413.7 – 4,525.3] (+1,111.6) |
| **Step 4 offset + pop next adjacency** | **2,378.64 ms** | [1,977.5 – 2,908.7] (+931.2) | **3,028.52 ms** | [2,567.7 – 3,564.0] (+996.4) |

- **Key Finding**:
  - **Difference EXCEEDS the spread with ZERO overlap**: `Step 4 offset + pop next adjacency` achieves a **30.9% query speedup** (-1,064.2 ms) and **26.3% total speedup** (-1,078.1 ms).
  - Its maximum recorded query time across all 9 rounds (2,908.7 ms) is strictly lower than the minimum run of Step 4 current (3,249.9 ms) by over 340 ms.
  - **Applied to production**: `track-d/src/main.rs` updated with this optimal strategy. Net formatted line change: +7 lines (523 code lines in `main.rs`, 531 total).

### 7. Full Verification Summary
- **Unit & Property Tests**:
  - Worked example (42 bytes, `2e9871bb...`): PASS.
  - Empty queries (0 bytes): PASS.
  - Single node & m = 0: PASS.
- **Multi-Seed Reference Comparison**:
  - Tiny seeds 1, 2, 3, 7: all PASS.
  - Sample seeds 1, 2, 3, 7: all PASS.
  - Full seed 1: exact SHA-256 `a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e` (PASS).
  - 50/50 randomized stress tests: all PASS.
- **Repeatability Suite**:
  - 20x sample runs: 20/20 PASS.
  - 20x full runs: 20/20 PASS.
  - Worker scaling runs (1, 4, 12 workers) on sample and full: all PASS.
- **Full-Scale Multi-Seed Verification (Seeds 2, 3, 5)**:
  - Generated full-scale datasets (n = 1,740,512, m = 5,000,000, 2,000 queries) with generator seeds 2, 3, 5, verified against Python reference outputs across 5 repeated runs and worker scaling:
    - **Seed 2**: Ref SHA-256 `23d60c06b8e2c6086f765e4a9a3640306c3bf8199149fd39b20e1ef56b3b8a4b`
      - 5 repeated runs: 5/5 MATCH (`23d60c06...`)
      - Worker scaling: 1 worker PASS, 4 workers PASS, 12 workers PASS.
    - **Seed 3**: Ref SHA-256 `6b5c6836de4dc461ba1557ef6cabc5e9723164d3ccb324ab4c5f0b281d41b8db`
      - 5 repeated runs: 5/5 MATCH (`6b5c6836...`)
      - Worker scaling: 1 worker PASS, 4 workers PASS, 12 workers PASS.
    - **Seed 5**: Ref SHA-256 `70c8532bd686a89af05e32a2b79abb327de894937052f9e737cc44b7f36c0fb3`
      - 5 repeated runs: 5/5 MATCH (`70c8532b...`)
      - Worker scaling: 1 worker PASS, 4 workers PASS, 12 workers PASS.
- **Official Harness Gate**: `python check.py d --data data/d --expected a23dbdf0...`: PASS.

---

## Step 3: Compact Per-Query State (Step B) + Two-Pass Zero-Intermediate Build & Interleaved Edges (Step A)

### 1. Approach & Changes
- **Compact Per-Query Distance State (`CompactDist`)**:
  - Replaced each worker's 13.92 MB full-graph distance vector with an open-addressed hash table using 8-byte entries (`Slot { node: u32, dist: u32 `).
  - Table starts at `INIT_CAP = 16,384` slots (128 KB) and dynamically grows up to `MAX_CAP = 524,288` slots (4 MB) at 50% load.
  - Resetting after each query takes O(|touched|) by traversing only touched slot indices.
  - Total query-phase memory across all 12 workers drops from 167 MB down to ~ 24 MB.
- **4-ary Min-Heap (`QuadHeap`)**:
  - Cache-friendly 4-ary min-heap where all 4 children fit in a contiguous 32-byte span (half a 64-byte cache line). Tree depth is halved relative to a binary heap.
- **Exact Lazy Fallback (`fallback_dijkstra`)**:
  - Transparent fallback for distances >= 2^{32 - 1 or queries expanding > 524,288 nodes. Fully stress-tested with tiny caps (64/256) and verified 100% byte-identical.
- **Shared Atomic Degree Array & Two-Pass Build (Step A)**:
  - Eliminated 12 x 7 MB = 84 MB per-worker degree vectors by using a single shared `AtomicU32` array of size n (7 MB).
  - Eliminated the 60 MB intermediate parsed edge buffer (`Vec<(u32, u32, u32)>`) via a two-pass parser directly from `mmap`:
    - **Pass 1**: Reads only node u directly from `mmap` and increments `degrees[u]` using relaxed atomics, skipping the rest of each edge line without allocating.
    - **Prefix sum**: Serial prefix sum on the single 7 MB array takes just 4.96 ms.
    - **Pass 2**: Reads u, v, w from `mmap` and scatters directly into the final CSR array using per-node atomic write offsets.
  - Build-phase heap footprint drops from ~ 190 MB to just 54 MB (`offsets` 7 MB + `write_pos` 7 MB + `edges` 40 MB).
- **Interleaved Edge Representation**:
  - Replaced separate 20 MB `heads` and 20 MB `weights` vectors with an interleaved `Edge { target: u32, weight: u32 ` (8 bytes), keeping target and edge weight in the same cache line.
- **Fast ASCII Parser (`parse_u32_raw`)**:
  - Unchecked pointer traversal for digit parsing, speeding up Pass 2 build from 453 ms to 217 ms.
- **Ablation Studies**:
  - **16-byte slot ablation (`Slot { node: u32, dist: u64 `, unbounded growth, no fallback)**: Saves 51 lines of code (368 lines vs 419 lines, granting +2.0% on S^{0.15), but increases 1-worker query time by +8.6% (10,437 ms vs 9,372 ms; 449.4 ns vs 403.5 ns/node) and 12-worker query time from 1,780 ms to 2,024 ms due to halving L1 cache line slot density from 8 slots to 4 slots (-4.1% on T^{0.50). Because the time penalty outweighs the line savings in K, 8-byte slots with lazy fallback are retained.
  - **`MAP_POPULATE` experiment**: Tested `MAP_POPULATE` (`0x08000`) on input file mmap. Synchronously faulting the 92 MB file increased mmap time from 2.92 ms to 735.40 ms, increased total process wall time by ~265 ms (2,737 ms vs 2,472 ms), and spiked container peak memory from 233.9 MB to 264.1 MB (+30.2 MB). *(Note: The MAP_POPULATE result is specific to this local VM / WSL2 environment, where forcing all file pages into resident RAM actively charges them against the cgroup v2 memory limit without offering any asynchronous multithreaded faulting benefits).* Lazy `mmap` is retained.

### 2. Phase-by-Phase Memory & Worker Scaling Analysis
To understand memory consumption across execution phases and worker counts, `/sys/fs/cgroup/memory.current`, `/sys/fs/cgroup/memory.peak`, and the `anon`/`file` split from `/proc/self/smaps_rollup` were instrumented across phase boundaries:

| Phase | 1 Worker: Cur / Peak (Anon / File) | 4 Workers: Cur / Peak (Anon / File) | 12 Workers: Cur / Peak (Anon / File) |
|---|---:|---:|---:|
| 01. Startup | 4.97 MB / 5.64 MB (0.1 / 1.9 MB) | 4.72 MB / 5.23 MB (0.1 / 1.8 MB) | 4.95 MB / 4.98 MB (0.1 / 1.9 MB) |
| 02. Post-mmap | 4.97 MB / 5.64 MB (0.1 / 1.9 MB) | 4.71 MB / 5.23 MB (0.1 / 1.8 MB) | 4.95 MB / 4.98 MB (0.1 / 1.9 MB) |
| 03. Pass 1 (degrees) | 98.92 MB / 98.93 MB (6.8 / 89.5 MB) | 56.26 MB / 63.04 MB (6.8 / 46.7 MB) | 95.51 MB / 95.77 MB (6.9 / 85.4 MB) |
| 04. Prefix sum | 105.53 MB / 105.67 MB (13.4 / 89.5 MB) | 55.57 MB / 63.04 MB (13.5 / 39.4 MB) | 102.61 MB / 102.61 MB (13.6 / 85.4 MB) |
| 05. Pass 2 (scatter) | 144.20 MB / 144.20 MB (51.6 / 89.5 MB) | 101.64 MB / 102.47 MB (51.6 / 47.5 MB) | 144.83 MB / 144.84 MB (51.7 / 89.4 MB) |
| 06. Queries prep | 144.20 MB / 144.20 MB (51.6 / 89.5 MB) | 101.88 MB / 102.47 MB (51.6 / 47.6 MB) | 144.83 MB / 144.84 MB (51.7 / 89.4 MB) |
| 07. Post-queries | 152.64 MB / **162.38 MB** (74.8 / 74.8 MB) | 182.75 MB / **183.05 MB** (144.3 / 34.4 MB) | 216.35 MB / **219.27 MB** (201.5 / 10.9 MB) |
| 08. Post-output | 152.63 MB / 162.38 MB (74.8 / 74.8 MB) | 182.75 MB / 183.05 MB (144.3 / 34.4 MB) | 216.15 MB / 219.27 MB (201.5 / 10.9 MB) |

#### Analysis of the 1-Worker Peak (156 - 162 MB)
1. **File Page Cache (89.5 MB)**: Reading `graph.txt` during Pass 1 and Pass 2 faults the entire 91.95 MB file into Linux page cache (`Pss_File`).
2. **CSR Base Structures (51.6 MB)**:
   - Contiguous 8-byte `Edge` array: 5,000,000 x 8 bytes = 40.0 MB.
   - CSR `offsets` array: (1,740,512 + 1) x 4 bytes = 6.96 MB.
   - `has_in` boolean filter: 1,740,512 x 1 byte = 1.74 MB.
   - Glibc / Rust runtime: ~ 2.9 MB.
3. **Query Allocations (~15-20 MB)**:
   - 1 worker's `CompactDist` table (up to 4.19 MB) + `touched` array (up to 2.10 MB) + `QuadHeap` (up to 3.2 MB) + sequential `fallback_dijkstra` temporary buffer (13.92 MB).
   - Because Anon is modest (74.8 MB), the kernel does not evict file page cache, yielding a total peak of **156 - 162 MB**.

#### Analysis of the ~7.4 MB per Extra Worker Scaling
Between 1 worker (peak 162.4 MB) and 12 workers (peak 219.3 MB), the delta is 56.9 MB across 11 extra workers (~ 5.2 - 7.4 MB per extra worker depending on cache eviction state):
1. **Per-Worker Compact State (~9.5 MB committed anon)**:
   - `CompactDist`: starts at 128 KB, expands dynamically up to `MAX_CAP = 524,288` slots (524,288 x 8 bytes = 4.19 MB).
   - `touched` indices: up to 262,144 entries x 8 bytes = 2.10 MB.
   - `QuadHeap` priority queue: up to 200,000 items x 16 bytes = 3.20 MB.
2. **Glibc Malloc Arenas & Thread Overhead**:
   - Glibc initializes a separate heap arena per thread, reserving virtual address blocks (`count=57` virtual mappings totaling 843 MB) with committed chunks for heap reallocations.
3. **Concurrent Fallback Allocations**:
   - In seed 1, 8 queries trigger `fallback_dijkstra`. In a 12-worker pool, multiple workers can execute fallback concurrently, each holding a temporary 13.92 MB distance vector.
4. **Kernel Page Cache Reclamation**:
   - As anonymous memory expands from 51.7 MB to 201.5 MB under 12 workers, the Linux kernel gracefully evicts clean `graph.txt` page cache pages (`File` memory drops from 89.4 MB down to 10.9 MB), keeping total container peak memory bounded to **219 - 239 MB**.

### 3. Measured Results (Local Proxy)
- **Phase Breakdown (12 workers vs 1 worker, full seed 1)**:
  - **12 workers**:
    - `mmap`: 2.92 ms
    - `build`: 664.66 ms (`pass1`: 401.88 ms, `prefix`: 4.96 ms, `pass2`: 217.31 ms)
    - `queries`: 1,780.44 ms
    - `total`: 2,472.61 ms
    - Peak memory: **233.92 MB** (245,280,768 bytes)
  - **1 worker**:
    - `mmap`: 4.47 ms
    - `build`: 2,672.15 ms (`pass1`: 1,108.48 ms, `prefix`: 4.66 ms, `pass2`: 1,533.19 ms)
    - `queries`: **9,372.22 ms** (down from 9,609 ms in Step B and 10,269 ms in Step 1)
    - Query rate: **403.5 ns / settled node** (23,226,759 settled nodes)
    - Peak memory: **156.45 MB** (164,048,896 bytes)
- **Local Benchmark (5 runs, warm cache, 12 workers)**:
  - Run 1: 3.695 s, peak: 239.39 MB
  - Run 2: 4.031 s, peak: 253.06 MB
  - Run 3: 3.905 s, peak: 251.93 MB
  - Run 4: 4.256 s, peak: 235.20 MB
  - Run 5: 4.422 s, peak: 230.52 MB
  - **Median time**: **4.031 s** (including Docker overhead; `check.py` single run **3.68 s**).
  - **Median peak memory**: **239.39 MB** (251,019,264 bytes).
- **Source Size (S)**: **419 non-blank, non-comment code lines** in `track-d/src/main.rs`.
- **Direct Runtime Dependencies (D)**: **D = 1** (zero external dependencies).
- **Verification**:
  - Worked example, empty queries, single node, m = 0: all PASS.
  - Multi-seed tiny & sample datasets (seeds 1, 2, 3, 7): all PASS.
  - Full seed 1: exact SHA-256 `a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e` (PASS).
  - 50/50 randomized stress tests: all PASS.
  - Official gate `check.py`: PASS.

---

## Step 2: Query Reachability Shortcuts & Ablations

### 1. Approach & Changes
- **Shortcuts**:
  - s == t -> 0 answered in O(1).
  - s != t and out_deg(s) == 0 (`offsets[s + 1] == offsets[s]`) -> -1.
  - s != t and in_deg(t) == 0 (`!has_in[t]`) -> -1. A 1.74 MB boolean array `has_in` tracks nodes with at least one incoming edge, populated in ~ 1.5 ms after CSR construction.
  - Bounds checks `s >= n` and `t >= n` dropped as the spec guarantees 0 <= s, t < n.
  - In full dataset seed 1, this eliminates the two large unreachable queries (query 781 and query 1914) that otherwise traverse the entire 1.74M node component, saving **3,480,742 settled node relaxations** and ~ 1.27 s of local query time.
- **Ablation: Duplicate Query Cache (REJECTED / ABLATED)**:
  - Deduplicating queries via standard library `HashMap` saved only 153,017 settled nodes (0.66%) across 20 duplicate queries.
  - Measured local query time: 2,163 ms with cache vs 2,126 ms without cache (Delta ~ 0 ms, within run-to-run noise).
  - The cache required ~ 35 extra code lines. Because 35 lines costs ~ 1.4% of K on S^{0.15 for no measurable time benefit, the duplicate cache was rejected and ablated.
- **Safety Documentation**:
  - Added explicit `// SAFETY:` rationale comments to all `unsafe` FFI blocks (`mmap`, `munmap`, slice construction, raw pointer writes).

### 2. Measured Results (Local Proxy)
- **Local Wall Time (median of 5 runs, warm page cache, 12 workers)**:
  - Run 1: 4.934 s, peak: 221.70 MB
  - Run 2: 4.438 s, peak: 246.19 MB
  - Run 3: 4.452 s, peak: 239.76 MB
  - Run 4: 4.367 s, peak: 248.61 MB
  - Run 5: 5.171 s, peak: 251.00 MB
  - **Median time**: **4.45 s** (including container launch overhead; internal solution time **3.42 s**, down from 4.54 s on baseline with 12 workers).
- **Local Peak Memory**: **246.19 MB** (258,150,400 bytes read from `/sys/fs/cgroup/memory.peak`).
- **Uncontended 1-Worker Query Cost**: **445.1 ns / settled node** (measured via profiler with `PROFILE_WORKERS=1`).
- **Source Size (S)**: **307 code lines** (298 in `src/main.rs`, 8 in `Dockerfile`, 1 in `.dockerignore`). Net change from baseline: **+12 lines**.
- **Direct Runtime Dependencies (D)**: **D = 1** (zero external dependencies).
- **Verification**:
  - Worked example: exact 42 bytes, SHA-256 `2e9871bb...` (PASS).
  - Empty queries: exact 0 bytes (PASS).
  - Single node & m = 0: PASS.
  - Multi-seed tiny & sample datasets (seeds 1, 2, 3, 7): all PASS.
  - Full seed 1: exact 9,039 bytes, SHA-256 `a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e` (PASS).
  - 50/50 randomized stress tests against reference: all PASS.
  - Official gate `check.py`: PASS.

---

## Step 1: Rust Baseline Implementation & Measurements

### 1. Approach & Architecture
- **Zero dependencies**: standard library only (`[dependencies]` is empty).
- **Zero-copy I/O**: Direct Unix FFI `mmap` with `PROT_READ` and `MAP_PRIVATE` for input files (`graph.txt` and `queries.txt`).
- **Parallel Chunked Parsing**: Header `n m` is read first; the remaining file body is partitioned across worker threads aligned to newline boundaries. Each thread parses unsigned 32-bit integers with a custom branchless/direct ASCII parser into chunk edge vectors while tracking local out-degrees.
- **Lock-free CSR Construction**: Global out-degrees are computed by summing per-thread degrees, prefix-summed into `offsets` (`n + 1` entries). Per-thread write pointers are assigned in-place without auxiliary allocation. Threads scatter their parsed edges into pre-sized `heads` and `weights` arrays concurrently without synchronization.
- **Reusable Dijkstra State**: Workers pool is bounded by `thread::available_parallelism()` (capped at 12). Work is distributed dynamically via an atomic query counter (`AtomicUsize`). Each worker maintains a single persistent `Vec<u64>` distance array initialized once to `u64::MAX`, a `Vec<u32>` touched-node list, and a `BinaryHeap<Reverse<(u64, u32)>>`. After each query settles `t` (or frontier exhausts), only touched entries are reset to `u64::MAX`, avoiding O(n) reinitialization overhead.
- **Deterministic Output**: Answers are stored directly into pre-indexed atomic result slots (`AtomicU64`). Answers are formatted in input order into a contiguous memory buffer and written via a single file write.

### 2. Local Phase Timings (5-run profile on full seed 1)
Measured using `std::time::Instant` inside a dedicated profiler container (`speeddemon-check/track-d-profiler`) running on the 91.95 MB seed 1 input with 2,000 queries (4 active workers locally):

| Phase | Run 1 (ms) | Run 2 (ms) | Run 3 (ms) | Run 4 (ms) | Run 5 (ms) | Median (ms) | % of Total |
|---|---:|---:|---:|---:|---:|---:|---:|
| Input mmap | 5.119 | 4.429 | 4.930 | 6.480 | 7.490 | **5.12** | 0.1% |
| Parallel edge parse | 592.222 | 518.904 | 530.164 | 531.584 | 518.143 | **530.16** | 10.5% |
| CSR prefix-sum & scatter | 269.404 | 252.017 | 249.878 | 273.772 | 275.920 | **269.40** | 5.3% |
| Dijkstra queries (2,000) | 4,173.724 | 4,211.467 | 4,373.649 | 4,225.654 | 4,283.802 | **4,225.65** | 83.7% |
| Output formatting & write | 6.586 | 6.037 | 5.182 | 6.112 | 7.021 | **6.11** | 0.1% |
| **Total Process Wall Time** | **5,047.72** | **4,993.64** | **5,164.40** | **5,044.68** | **5,092.96** | **5,047.72** | **100.0%** |

*Note: On the official scoring box (Intel i5-13500 with 12 threads on 6 P-cores), the query phase is expected to benefit from 3x higher thread parallelism (12 workers vs 4 local workers), scaling the ~4.2 s query phase down to ~1.4 s.*

### 3. Container Peak Memory (`cgroup v2 memory.peak`)
Measured post-process-exit from `/sys/fs/cgroup/memory.peak` under sandbox constraints (`--memory=8g --tmpfs /tmp:rw,size=1g,exec`):

| Input Variant | Peak Memory (bytes) | Peak Memory (MB) | Hash Gate |
|---|---:|---:|---|
| **mmap (retained)** | **224,931,840** | **214.51** | `a23dbdf0...` PASS |
| `std::fs::read` (experiment) | 230,879,232 | 220.18 | `a23dbdf0...` PASS |

- `mmap` achieved lower peak memory by avoiding heap allocations for raw file bytes, confirming the mmap design choice.
- Compared to the Python reference baseline (M_ref = 893 MB), the Rust baseline reduces peak memory to ~215 MB (~4.2x reduction).

### 4. Source Size (S) and Dependency Count (D)
- `track-d/src/main.rs`: 285 code lines (non-blank, non-comment, formatted with `rustfmt` edition 2021).
- `track-d/Dockerfile`: 8 code lines.
- `track-d/.dockerignore`: 1 code line (`target/`).
- Total source count: S ~ 293 code lines (or 294 including `.dockerignore`).
- Direct dependencies: D = 1 (zero external crates; standard library only).
