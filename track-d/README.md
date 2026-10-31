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
6. **Deterministic Output**: Answers are stored directly into pre-indexed atomic result slots, formatted in input order into a contiguous memory buffer and written to `/out/distances.txt`.

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
