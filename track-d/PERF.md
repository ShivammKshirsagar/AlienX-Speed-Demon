# Track D: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | The reference, unchanged | | 72.69 | 893 | 0 |
| 1 | Rust baseline: Unix mmap, parallel chunked parsing, forward CSR, reusable distance state + touched-node list, atomic query distribution, pre-sized output | run-c21758fd2f7f | 1.701 (board) / 5.05 (local) | 238 (board) / 215 (local) | 40.24 (board) |
| 2 | Cheap reachability shortcuts ($s == t$, $\text{out\_deg}(s) == 0$, $\text{in\_deg}(t) == 0$) [duplicate cache ablated] | run-1db5b0ec6c38 | 1.327 (board) / 4.45 (local) | 229 (board) / 246 (local) | 7.229 (board) |
| 3 | Step B + Step A: Compact per-query state (open-addressing) + 4-ary heap + shared atomic degrees (2-pass zero-intermediate build) + interleaved edges | run-353ba4da0058 | 0.864 (board) / 3.68 (local) | 216 (board) / 234 (local) | 8.29 (board) |
| 4 | Memory shrinkers (table/heap trim, glibc mallopt, serialized fallback) + rustfmt line compression | [Board run ID TBD] | 3.70 - 4.02 (local proxy) | 171.24 (local proxy) | TBD |

*Note: Step 0 numbers (72.69 s, 893 MB) are the organisers' official scoring-box baseline. Steps 1-3 include official live board measurements (Step 3: 0.864 s, 216 MB, 558 lines, K 8.29). Step 4 figures (3.70-4.02 s local check.py, 171.24 MB peak memory, 512 rustfmt lines) are local measurements on an AMD Ryzen 5 4600H under Docker Desktop (WSL2 cgroup v2, 12 workers) and serve as a local proxy; board run ID is marked TBD.*

---

## Step 4: Per-Worker Memory Shrinking, Formatted Line Compression, and tools/ Prototypes

### 1. Formatted Line Count Investigation & Reduction
- **Board Measurement Reproduction**:
  - The live board measured 558 code lines for Step 3 where local unformatted `scc` reported 428 lines.
  - Reproducing the harness formatting via `rustfmt --edition 2021` on `track-d/src/main.rs` produced **550 code lines** in `main.rs` + **8 code lines** in `Dockerfile` = **558 code lines** exactly.
- **Line Compression**:
  - Refactored `track-d/src/main.rs` using tuple structs (`Edge(u32, u32)`, `Slot(u32, u32)`, `HeapItem(u64, u32)`), concise iterator/loop patterns, single-line FFI declarations, and streamlined integer printing.
  - Formatted `scc` count under `rustfmt --edition 2021` dropped from **550 lines down to 504 lines** in `src/main.rs`.
  - Total official formatted line count (including Dockerfile): **512 code lines** (net reduction of **46 lines**, improving $S^{0.15}$ factor by +1.3%).

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
- **Design**: Built forward CSR and reverse CSR from input graph. Interleaved forward search from $s$ and backward search from $t$ using min-key frontier balancing and mathematically exact termination `top_f + top_b >= mu`.
- **Settled Nodes Evaluation (Full Seed 1, 2,000 queries)**:
  - **Unidirectional Dijkstra**: Settled **26,707,977 nodes** (average 13,354.0 settled / query).
  - **Bidirectional Dijkstra**: Settled **14,313,259 nodes** (average 7,156.6 settled / query; 3,549,570 forward + 10,763,689 backward).
  - **Node Reduction**: Settles **46.41% fewer nodes** (53.59% ratio).
- **Correctness**: 0 mismatches across all 2,000 queries. Output SHA-256 matches reference `a23dbdf0...` exactly.
- **Decision**: **REJECTED**. Although bidirectional search settles 46.4% fewer nodes, storing the reverse CSR costs about **47 MB of extra heap memory** (40 MB reverse edges + 6.96 MB reverse offsets) and adds ~0.25 s to the build phase, which directly hurts the peak container memory metric ($M$). Unidirectional search with forward-only CSR is retained.

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

### 4. Parameter Sweep: `INIT_CAP` & Shrink Threshold
Evaluated all 12 combinations of initial table capacity `INIT_CAP` $\in [16\text{k}, 32\text{k}, 64\text{k}, 128\text{k}]$ and per-query reset shrink threshold `SHRINK_THRESH` $\in [\text{never}, 256\text{k}, 512\text{k}]$. Each cell reports the median of 5 runs at 12 workers, internal total time, 1-worker query rate (ns/settled node), and 5 ms sampled peak anonymous memory (`Pss_Anon`) during queries.

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
  - `never` exceeds the 95 MB peak anon budget across all configurations ($\approx 108 - 110\text{ MB}$).
  - `512k` yields the fastest query execution ($3008\text{ ms}$ query / $3631\text{ ms}$ total at 16k), but sits right at the limit ($95.57\text{ MB}$).
  - `16k / 256k` provides the optimal balance: **3182.11 ms query**, **3815.49 ms total**, and **79.30 MB sampled peak anon** (safely below the $95\text{ MB}$ limit with a 15.7 MB safety buffer).
  - **Chosen Setting**: `INIT_CAP = 16,384`, `SHRINK_THRESH = 262,144` (256k slots), `MAX_CAP = 524,288`.

### 5. Adjacency List Prefetching at Push Time
- **Mechanism**: When relaxing edge $(u, v)$ in Dijkstra and inserting $v$ into the priority queue (`heap.push`), look up `offsets[v]` and issue hardware prefetch `_mm_prefetch(edges.as_ptr().add(v_offset) as *const i8, _MM_HINT_T0)`. This warms L1/L2 cache with the first 64 bytes of $v$'s outgoing adjacency list well before $v$ is dequeued.
- **1-Worker Rate Impact (Full Seed 1)**:
  - **Without Prefetching**: $450.3\text{ ns / settled node}$ ($11,217\text{ ms}$ query phase).
  - **With Prefetching**: **$401.0 - 423.2\text{ ns / settled node}$** ($9,987 - 10,540\text{ ms}$ query phase, **~6.0% - 11.0% speedup**).
- **Correctness & Line Impact**: 0 mismatches across all 2,000 queries. Formatted code line count increases by only 3 lines.
- **Decision**: **ACCEPTED**.

### 6. Full Verification Summary
- **Unit & Property Tests**:
  - Worked example (42 bytes, `2e9871bb...`): PASS.
  - Empty queries (0 bytes): PASS.
  - Single node & $m = 0$: PASS.
- **Multi-Seed Reference Comparison**:
  - Tiny seeds 1, 2, 3, 7: all PASS.
  - Sample seeds 1, 2, 3, 7: all PASS.
  - Full seed 1: exact SHA-256 `a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e` (PASS).
  - 50/50 randomized stress tests: all PASS.
- **Repeatability Suite**:
  - 20x sample runs: 20/20 PASS.
  - 20x full runs: 20/20 PASS.
  - Worker scaling runs (1, 4, 12 workers) on sample and full: all PASS.
- **Official Harness Gate**: `python check.py d --data data/d --expected a23dbdf0...`: PASS.

---

## Step 3: Compact Per-Query State (Step B) + Two-Pass Zero-Intermediate Build & Interleaved Edges (Step A)

### 1. Approach & Changes
- **Compact Per-Query Distance State (`CompactDist`)**:
  - Replaced each worker's $13.92\text{ MB}$ full-graph distance vector with an open-addressed hash table using 8-byte entries (`Slot { node: u32, dist: u32 }`).
  - Table starts at `INIT_CAP = 16,384` slots (128 KB) and dynamically grows up to `MAX_CAP = 524,288` slots (4 MB) at 50% load.
  - Resetting after each query takes $O(|\text{touched}|)$ by traversing only touched slot indices.
  - Total query-phase memory across all 12 workers drops from $167\text{ MB}$ down to $\approx 24\text{ MB}$.
- **4-ary Min-Heap (`QuadHeap`)**:
  - Cache-friendly 4-ary min-heap where all 4 children fit in a contiguous 32-byte span (half a 64-byte cache line). Tree depth is halved relative to a binary heap.
- **Exact Lazy Fallback (`fallback_dijkstra`)**:
  - Transparent fallback for distances $\ge 2^{32} - 1$ or queries expanding $> 524,288$ nodes. Fully stress-tested with tiny caps (64/256) and verified 100% byte-identical.
- **Shared Atomic Degree Array & Two-Pass Build (Step A)**:
  - Eliminated $12 \times 7\text{ MB} = 84\text{ MB}$ per-worker degree vectors by using a single shared `AtomicU32` array of size $n$ (7 MB).
  - Eliminated the 60 MB intermediate parsed edge buffer (`Vec<(u32, u32, u32)>`) via a two-pass parser directly from `mmap`:
    - **Pass 1**: Reads only node $u$ directly from `mmap` and increments `degrees[u]` using relaxed atomics, skipping the rest of each edge line without allocating.
    - **Prefix sum**: Serial prefix sum on the single 7 MB array takes just $4.96\text{ ms}$.
    - **Pass 2**: Reads $u, v, w$ from `mmap` and scatters directly into the final CSR array using per-node atomic write offsets.
  - Build-phase heap footprint drops from $\approx 190\text{ MB}$ to just $\mathbf{54\text{ MB}}$ (`offsets` 7 MB + `write_pos` 7 MB + `edges` 40 MB).
- **Interleaved Edge Representation**:
  - Replaced separate 20 MB `heads` and 20 MB `weights` vectors with an interleaved `Edge { target: u32, weight: u32 }` (8 bytes), keeping target and edge weight in the same cache line.
- **Fast ASCII Parser (`parse_u32_raw`)**:
  - Unchecked pointer traversal for digit parsing, speeding up Pass 2 build from 453 ms to 217 ms.
- **Ablation Studies**:
  - **16-byte slot ablation (`Slot { node: u32, dist: u64 }`, unbounded growth, no fallback)**: Saves 51 lines of code (368 lines vs 419 lines, granting +2.0% on $S^{0.15}$), but increases 1-worker query time by +8.6% (10,437 ms vs 9,372 ms; 449.4 ns vs 403.5 ns/node) and 12-worker query time from 1,780 ms to 2,024 ms due to halving L1 cache line slot density from 8 slots to 4 slots (-4.1% on $T^{0.50}$). Because the time penalty outweighs the line savings in $K$, 8-byte slots with lazy fallback are retained.
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
   - Contiguous 8-byte `Edge` array: $5,000,000 \times 8\text{ bytes} = \mathbf{40.0\text{ MB}}$.
   - CSR `offsets` array: $(1,740,512 + 1) \times 4\text{ bytes} = \mathbf{6.96\text{ MB}}$.
   - `has_in` boolean filter: $1,740,512 \times 1\text{ byte} = \mathbf{1.74\text{ MB}}$.
   - Glibc / Rust runtime: $\approx 2.9\text{ MB}$.
3. **Query Allocations (~15-20 MB)**:
   - 1 worker's `CompactDist` table (up to 4.19 MB) + `touched` array (up to 2.10 MB) + `QuadHeap` (up to 3.2 MB) + sequential `fallback_dijkstra` temporary buffer (13.92 MB).
   - Because Anon is modest (74.8 MB), the kernel does not evict file page cache, yielding a total peak of **156 - 162 MB**.

#### Analysis of the ~7.4 MB per Extra Worker Scaling
Between 1 worker (peak 162.4 MB) and 12 workers (peak 219.3 MB), the delta is 56.9 MB across 11 extra workers ($\approx 5.2 - 7.4\text{ MB}$ per extra worker depending on cache eviction state):
1. **Per-Worker Compact State (~9.5 MB committed anon)**:
   - `CompactDist`: starts at 128 KB, expands dynamically up to `MAX_CAP = 524,288` slots ($524,288 \times 8\text{ bytes} = \mathbf{4.19\text{ MB}}$).
   - `touched` indices: up to 262,144 entries $\times 8\text{ bytes} = \mathbf{2.10\text{ MB}}$.
   - `QuadHeap` priority queue: up to 200,000 items $\times 16\text{ bytes} = \mathbf{3.20\text{ MB}}$.
2. **Glibc Malloc Arenas & Thread Overhead**:
   - Glibc initializes a separate heap arena per thread, reserving virtual address blocks (`count=57` virtual mappings totaling 843 MB) with committed chunks for heap reallocations.
3. **Concurrent Fallback Allocations**:
   - In seed 1, 8 queries trigger `fallback_dijkstra`. In a 12-worker pool, multiple workers can execute fallback concurrently, each holding a temporary $13.92\text{ MB}$ distance vector.
4. **Kernel Page Cache Reclamation**:
   - As anonymous memory expands from 51.7 MB to 201.5 MB under 12 workers, the Linux kernel gracefully evicts clean `graph.txt` page cache pages (`File` memory drops from 89.4 MB down to 10.9 MB), keeping total container peak memory bounded to **219 - 239 MB**.

### 3. Measured Results (Local Proxy)
- **Phase Breakdown (12 workers vs 1 worker, full seed 1)**:
  - **12 workers**:
    - `mmap`: 2.92 ms
    - `build`: 664.66 ms (`pass1`: 401.88 ms, `prefix`: 4.96 ms, `pass2`: 217.31 ms)
    - `queries`: 1,780.44 ms
    - `total`: 2,472.61 ms
    - Peak memory: **233.92 MB** ($245,280,768$ bytes)
  - **1 worker**:
    - `mmap`: 4.47 ms
    - `build`: 2,672.15 ms (`pass1`: 1,108.48 ms, `prefix`: 4.66 ms, `pass2`: 1,533.19 ms)
    - `queries`: **9,372.22 ms** (down from 9,609 ms in Step B and 10,269 ms in Step 1)
    - Query rate: **403.5 ns / settled node** (23,226,759 settled nodes)
    - Peak memory: **156.45 MB** ($164,048,896$ bytes)
- **Local Benchmark (5 runs, warm cache, 12 workers)**:
  - Run 1: 3.695 s, peak: 239.39 MB
  - Run 2: 4.031 s, peak: 253.06 MB
  - Run 3: 3.905 s, peak: 251.93 MB
  - Run 4: 4.256 s, peak: 235.20 MB
  - Run 5: 4.422 s, peak: 230.52 MB
  - **Median time**: **4.031 s** (including Docker overhead; `check.py` single run **3.68 s**).
  - **Median peak memory**: **239.39 MB** ($251,019,264$ bytes).
- **Source Size ($S$)**: **419 non-blank, non-comment code lines** in `track-d/src/main.rs`.
- **Direct Runtime Dependencies ($D$)**: **$D = 1$** (zero external dependencies).
- **Verification**:
  - Worked example, empty queries, single node, $m = 0$: all PASS.
  - Multi-seed tiny & sample datasets (seeds 1, 2, 3, 7): all PASS.
  - Full seed 1: exact SHA-256 `a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e` (PASS).
  - 50/50 randomized stress tests: all PASS.
  - Official gate `check.py`: PASS.

---

## Step 2: Query Reachability Shortcuts & Ablations

### 1. Approach & Changes
- **Shortcuts**:
  - $s == t \implies 0$ answered in $O(1)$.
  - $s \neq t$ and $\text{out\_deg}(s) == 0$ (`offsets[s + 1] == offsets[s]`) $\implies -1$.
  - $s \neq t$ and $\text{in\_deg}(t) == 0$ (`!has_in[t]`) $\implies -1$. A 1.74 MB boolean array `has_in` tracks nodes with at least one incoming edge, populated in $\approx 1.5\text{ ms}$ after CSR construction.
  - Bounds checks `s >= n` and `t >= n` dropped as the spec guarantees $0 \le s, t < n$.
  - In full dataset seed 1, this eliminates the two large unreachable queries (query 781 and query 1914) that otherwise traverse the entire 1.74M node component, saving **3,480,742 settled node relaxations** and $\approx 1.27\text{ s}$ of local query time.
- **Ablation: Duplicate Query Cache (REJECTED / ABLATED)**:
  - Deduplicating queries via standard library `HashMap` saved only 153,017 settled nodes (0.66%) across 20 duplicate queries.
  - Measured local query time: $2,163\text{ ms}$ with cache vs $2,126\text{ ms}$ without cache ($\Delta \approx 0\text{ ms}$, within run-to-run noise).
  - The cache required $\approx 35$ extra code lines. Because 35 lines costs $\approx 1.4\%$ of $K$ on $S^{0.15}$ for no measurable time benefit, the duplicate cache was rejected and ablated.
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
- **Local Peak Memory**: **246.19 MB** ($258,150,400$ bytes read from `/sys/fs/cgroup/memory.peak`).
- **Uncontended 1-Worker Query Cost**: **445.1 ns / settled node** (measured via profiler with `PROFILE_WORKERS=1`).
- **Source Size ($S$)**: **307 code lines** (298 in `src/main.rs`, 8 in `Dockerfile`, 1 in `.dockerignore`). Net change from baseline: **+12 lines**.
- **Direct Runtime Dependencies ($D$)**: **$D = 1$** (zero external dependencies).
- **Verification**:
  - Worked example: exact 42 bytes, SHA-256 `2e9871bb...` (PASS).
  - Empty queries: exact 0 bytes (PASS).
  - Single node & $m = 0$: PASS.
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
- **Reusable Dijkstra State**: Workers pool is bounded by `thread::available_parallelism()` (capped at 12). Work is distributed dynamically via an atomic query counter (`AtomicUsize`). Each worker maintains a single persistent `Vec<u64>` distance array initialized once to `u64::MAX`, a `Vec<u32>` touched-node list, and a `BinaryHeap<Reverse<(u64, u32)>>`. After each query settles `t` (or frontier exhausts), only touched entries are reset to `u64::MAX`, avoiding $O(n)$ reinitialization overhead.
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
- Compared to the Python reference baseline ($M_\text{ref} = 893\text{ MB}$), the Rust baseline reduces peak memory to ~215 MB (~4.2x reduction).

### 4. Source Size ($S$) and Dependency Count ($D$)
- `track-d/src/main.rs`: 285 code lines (non-blank, non-comment, formatted with `rustfmt` edition 2021).
- `track-d/Dockerfile`: 8 code lines.
- `track-d/.dockerignore`: 1 code line (`target/`).
- Total source count: $S \approx 293$ code lines (or 294 including `.dockerignore`).
- Direct dependencies: $D = 1$ (zero external crates; standard library only).
