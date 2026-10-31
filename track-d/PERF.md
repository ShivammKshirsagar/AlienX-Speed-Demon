# Track D: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | The reference, unchanged | | 72.69 | 893 | 0 |
| 1 | Rust baseline: Unix mmap, parallel chunked parsing, forward CSR, reusable distance state + touched-node list, atomic query distribution, pre-sized output | run-c21758fd2f7f | 1.701 (board) / 5.05 (local) | 238 (board) / 215 (local) | 40.24 (board) |
| 2 | Cheap reachability shortcuts ($s == t$, $\text{out\_deg}(s) == 0$, $\text{in\_deg}(t) == 0$) [duplicate cache ablated] | TBD | 4.45 (local proxy) | 246 (local proxy) | TBD |

*Note: Step 0 numbers (72.69 s, 893 MB) are the organisers' official scoring-box baseline. Step 1 includes official live board measurements (run-c21758fd2f7f: 1.701 s, 238 MB, S=295, K=6.377) alongside earlier 4-worker local measurements (5.05 s, 215 MB). Step 2 figures (4.45 s container time / 3.42 s internal solution time, 246 MB) are local measurements on an AMD Ryzen 5 4600H under Docker Desktop (WSL2 cgroup v2, 12 workers) and serve as a local proxy; board run ID is marked TBD.*

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
