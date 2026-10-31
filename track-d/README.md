# Track D: Traverse

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/d-traverse.md).

## Approach

Our solution is implemented in Rust using only the standard library and built into a minimal digest-pinned Docker container.

1. **Zero-copy I/O**: Input files (`graph.txt` and `queries.txt`) are mapped into memory via raw Unix `mmap` with read-only permissions (`PROT_READ`, `MAP_PRIVATE`).
2. **Two-Pass Build & Shared Degree Array**:
   - Instead of per-worker degree vectors ($12 \times 7\text{ MB} = 84\text{ MB}$) and intermediate parsed edge buffers ($60\text{ MB}$), a shared `AtomicU32` array of size $n$ (7 MB) tracks degrees across threads.
   - **Pass 1**: Worker threads parse only node $u$ directly from `mmap` and increment `degrees[u]` with relaxed atomics, skipping the rest of each edge line.
   - **Prefix Sum**: Serial prefix sum on the single 7 MB array runs in under 5 ms to produce CSR `offsets`.
   - **Pass 2**: Worker threads parse $u, v, w$ from `mmap` using a fast ASCII parser and scatter directly into the final CSR array. Peak build-phase memory drops to just 54 MB.
3. **Interleaved Forward CSR**: Edges are stored as contiguous 8-byte `Edge { target: u32, weight: u32 }` structs, keeping target and weight in the same cache line and cutting random memory hops during graph traversal.
4. **Cheap Reachability Shortcuts**: $s == t$ is answered instantly as 0. Unreachable targets with $\text{in\_deg}(t) == 0$ or sources with $\text{out\_deg}(s) == 0$ are answered as -1 without launching Dijkstra, eliminating massive whole-component traversals on unreachable queries.
5. **Compact Per-Query State & 4-ary Min-Heap**: Instead of allocating a 14 MB flat `u64` array per worker ($12 \times 14 = 167\text{ MB}$), each worker maintains a compact open-addressed hash table with 8-byte slots (`Slot { node: u32, dist: u32 }`) that grows dynamically from 16,384 slots (128 KB) up to 524,288 slots as the settled set expands. A 4-ary min-heap (`QuadHeap`) halves tree depth and parent sift-up comparisons compared to a binary heap, with children laid out contiguously within a cache line. For rare queries reaching distances $\ge 2^{32}$ or touching > 524k nodes, an exact 64-bit fallback Dijkstra is lazily invoked.
6. **Per-Worker Memory Shrinking & Glibc Heap Tuning**:
   - **Dynamic Buffer Shrinking**: After each query, touched slot values are reset in $O(|\text{touched}|)$. If the table grew to $\ge 256\text{k}$ slots or heap capacity exceeded 4096, buffers shrink back to `INIT_CAP` (16k) and 1024 items, eliminating memory creep across queries.
   - **Glibc Tuning via `mallopt`**: Sets `M_TRIM_THRESHOLD` and `M_MMAP_THRESHOLD` to 64 KB and caps `M_ARENA_MAX` to 2 arenas, preventing 12 worker threads from hoarding independent glibc memory arenas. Explicit `malloc_trim(0)` releases freed build memory and fallback buffers back to the OS.
   - **Serialized Lazy Fallback**: Concurrent fallback executions are serialized via an atomic spinlock, guaranteeing that at most a single temporary 14 MB fallback vector exists in memory across all 12 workers. Container peak memory drops to **90 MB**.
7. **Two-Tier Hardware Prefetching**:
   - **Push-time offset prefetch**: When relaxing edge $(u, v)$ and pushing $v$ to the 4-ary min-heap, issuing an asynchronous `_mm_prefetch` for `&offsets[v]` primes the node's offset address without incurring a demand-load pipeline stall.
   - **Pop-time next adjacency prefetch**: Immediately upon popping node $u$, peering at the next element in the 4-ary min-heap (`heap.0.first()`) and issuing an `_mm_prefetch` for its outgoing adjacency list `edges[offsets[next_u]]` warms L1/L2 cache during $u$'s edge relaxation loop, cutting 12-worker query time by **30.9%** (down to 2.37 s).
8. **Deterministic Output**: Answers are stored directly into pre-indexed atomic result slots, formatted in input order with an in-place ASCII digit converter into a contiguous memory buffer and written to `/out/distances.txt`.

## Dependencies

None (standard library only).

## With another 24 hours

1. **Bidirectional Dijkstra**:
   - Construct reverse CSR alongside forward CSR.
   - Alternate search frontiers between source and target, terminating when the sum of minimum frontier distances reaches the best meeting distance.
2. **Locality Layout**:
   - Renumber nodes by BFS or Hilbert order to improve CPU cache hit rates during graph traversal.
3. **SIMD ASCII Parsing**:
   - Vectorize newline finding and digit conversion using SSE2/AVX2 intrinsics.
4. **Transparent Huge Pages (MADV_HUGEPAGE)**:
   - Back the 47 MB CSR offsets and edges buffers with 2 MB-aligned huge pages via madvise(MADV_HUGEPAGE) to eliminate dTLB miss stalls during graph traversals.
