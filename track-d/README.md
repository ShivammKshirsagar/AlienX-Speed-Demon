# Track D: Traverse

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/d-traverse.md).

## Approach

Our solution is implemented in Rust using zero runtime dependencies (standard library only) and built into a minimal digest-pinned Docker container.

1. **Zero-copy I/O**: Input files (`graph.txt` and `queries.txt`) are mapped into memory via raw Unix `mmap` with read-only permissions (`PROT_READ`, `MAP_PRIVATE`).
2. **Two-Pass Build & Shared Degree Array**:
   - Instead of per-worker degree vectors ($12 \times 7\text{ MB} = 84\text{ MB}$) and intermediate parsed edge buffers ($60\text{ MB}$), a shared `AtomicU32` array of size $n$ (7 MB) tracks degrees across threads.
   - **Pass 1**: Worker threads parse edge lines directly from `mmap` and increment `degrees[u]` with relaxed atomics, accounting for 1 word for weights $< \text{escape\_w}$ and 2 words for large weights.
   - **Prefix Sum**: Serial prefix sum on the single 7 MB array runs in under 5 ms to produce CSR `offsets`.
   - **Pass 2**: Worker threads parse $u, v, w$ from `mmap` using a fast ASCII parser and scatter directly into the final CSR array. Peak build-phase memory drops to just 54 MB.
3. **Packed 4-byte Edges with Runtime Bit Split & Escape Words**:
   - Target bits are dynamically derived at runtime from $n$: $T_{\text{bits}} = 32 - (n - 1)\text{.leading\_zeros()}$, with the remaining bits assigned to weight $W_{\text{bits}} = 32 - T_{\text{bits}}$ (e.g., $T_{\text{bits}} = 21, W_{\text{bits}} = 11$ for $n = 1,740,512$).
   - Edges with weight $< 2^{W_{\text{bits}}} - 1$ pack target and weight into a single 4-byte word: $(v \ll W_{\text{bits}}) \mid w$.
   - Rare large weights ($\ge 2^{W_{\text{bits}}} - 1$) store an escape sentinel followed by the uncompressed $u32$ weight in the subsequent word.
   - This cuts CSR edge array memory from 40 MB down to ~20 MB (16 edges per 64-byte cache line) and reduces peak anonymous memory by 18.2 MB.
4. **Cheap Reachability Shortcuts**: $s == t$ is answered instantly as 0. Unreachable targets with $\text{in\_deg}(t) == 0$ or sources with $\text{out\_deg}(s) == 0$ are answered as -1 without launching Dijkstra, eliminating massive whole-component traversals on unreachable queries.
5. **Compact Per-Query State & 4-ary Min-Heap**:
   - Instead of allocating a 14 MB flat `u64` array per worker ($12 \times 14 = 167\text{ MB}$), each worker maintains a compact open-addressed hash table with 8-byte slots (`Slot { node: u32, dist: u32 }`) that grows dynamically from 16,384 slots (128 KB) up to 524,288 slots as the settled set expands.
   - A 4-ary min-heap (`QuadHeap`) halves tree depth and parent sift-up comparisons compared to a binary heap, with children laid out contiguously within a cache line.
6. **Serialized Lazy Fallback**:
   - For rare queries reaching distances $\ge 2^{32}$ or exceeding the hash table capacity, an exact 64-bit fallback Dijkstra is lazily invoked.
   - Concurrent fallback executions are serialized via an atomic spinlock (`FALLBACK_LOCK`), guaranteeing that at most a single temporary 14 MB fallback vector exists in memory across all 12 workers.
7. **Buffer Shrinking & Glibc Heap Tuning (`mallopt`)**:
   - **Dynamic Buffer Shrinking**: After each query, touched slot values are reset in $O(|\text{touched}|)$. If the table grew to $\ge 256\text{k}$ slots or heap capacity exceeded 4096, buffers shrink back to `INIT_CAP` (16k) and 1024 items, eliminating memory creep across queries.
   - **Glibc Tuning via `mallopt`**: Sets `M_TRIM_THRESHOLD` and `M_MMAP_THRESHOLD` to 64 KB and caps `M_ARENA_MAX` to 2 arenas, preventing 12 worker threads from hoarding independent glibc memory arenas. Explicit `malloc_trim(0)` releases freed build memory and fallback buffers back to the OS.
8. **Two-Tier Hardware Prefetching**:
   - **Push-time offset prefetch**: When relaxing edge $(u, v)$ and pushing $v$ to the 4-ary min-heap, issuing an asynchronous `_mm_prefetch` for `&offsets[v]` primes the node's offset address without incurring a demand-load pipeline stall.
   - **Pop-time next adjacency prefetch**: Immediately upon popping node $u$, peering at the next element in the 4-ary min-heap (`heap.0.first()`) and issuing an `_mm_prefetch` for its outgoing adjacency list `edges[offsets[next_u]]` warms L1/L2 cache during $u$'s edge relaxation loop.
9. **Deterministic Output**: Answers are stored directly into pre-indexed atomic result slots, formatted in input order with an in-place ASCII digit converter into a contiguous memory buffer and written to `/out/distances.txt`.

## Dependencies

Zero dependencies (standard library only).

## With another 24 hours

1. **Radix Heap with Shrinking Bucket Buffers**:
   - Monotone bucket queue (radix heap) with 64/65 buckets providing amortized $O(1)$ operations via bitwise operations. In testing, this reduced 1-worker query time from 278.6 ns/node to 242.8 ns/node (-12.8%). Allocating compact shared or dynamically trimmed bucket vectors per thread would keep peak memory within budget.
2. **Bidirectional Dijkstra**:
   - Construct reverse CSR alongside forward CSR. Alternate search frontiers between source and target, terminating when the sum of minimum frontier distances reaches the best meeting distance.
3. **Locality Layout**:
   - Renumber nodes by BFS or Hilbert order to improve CPU cache hit rates during graph traversal.
4. **Transparent Huge Pages (MADV_HUGEPAGE)**:
   - Back the CSR offsets and edges buffers with 2 MB-aligned huge pages via `madvise(MADV_HUGEPAGE)` to eliminate dTLB miss stalls during graph traversals.
