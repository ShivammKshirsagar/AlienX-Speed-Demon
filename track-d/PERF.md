# Track D: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | The reference, unchanged | | 72.69 | 893 | 0 |
| 1 | Rust baseline: Unix mmap, parallel chunked parsing, forward CSR, reusable distance state + touched-node list, atomic query distribution, pre-sized output | TBD | 5.05 (local proxy) | 215 (local proxy) | TBD |

*Note: Step 0 numbers (72.69 s, 893 MB) are the organisers' official scoring-box baseline. Step 1 figures (5.05 s, 215 MB) are local measurements under Docker Desktop (WSL2 cgroup v2, 4 worker threads active locally) and serve as a local proxy; board run ID is marked TBD and will be updated from live board runs.*

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

### 5. Indicative $K$ Estimate
Using the scoring formula $K = (T_\text{ref}/T)^{0.50} \times (M_\text{ref}/M)^{0.20} \times (S_\text{ref}/S)^{0.15} \times (D_\text{ref}/D)^{0.15}$:
- Baselines: $T_\text{ref} = 72.69\text{ s}$, $M_\text{ref} = 893\text{ MB}$, $S_\text{ref} \approx 62$ (Python reference), $D_\text{ref} = 1$.
- Local inputs: local wall proxy $T \approx 5.05\text{ s}$ (with 4 workers; estimated $\le 2.0\text{ s}$ with 12 workers on scoring box), local memory proxy $M \approx 215\text{ MB}$, $S \approx 293$, $D = 1$.
- Conservative local proxy estimate (with $T = 5.05\text{ s}$):
  $$K_\text{local} \approx \left(\frac{72.69}{5.05}\right)^{0.50} \times \left(\frac{893}{215}\right)^{0.20} \times \left(\frac{62}{293}\right)^{0.15} \times 1.0 \approx 3.79 \times 1.33 \times 0.79 \approx 3.98$$
- Estimated scoring box projection (with $T \approx 2.0\text{ s}$ on 12 threads):
  $$K_\text{proj} \approx \left(\frac{72.69}{2.00}\right)^{0.50} \times \left(\frac{893}{215}\right)^{0.20} \times \left(\frac{62}{293}\right)^{0.15} \times 1.0 \approx 6.03 \times 1.33 \times 0.79 \approx 6.34$$
*(All $K$ figures are estimates. Official score depends on dedicated scoring box timings and board run IDs.)*
