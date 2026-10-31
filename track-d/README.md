# Track D: Traverse

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/d-traverse.md).

## Approach

Our solution is implemented in Rust using only the standard library and built into a minimal digest-pinned Docker container.

1. **Zero-copy I/O**: Input files (`graph.txt` and `queries.txt`) are mapped into memory via raw Unix `mmap` with read-only permissions (`PROT_READ`, `MAP_PRIVATE`).
2. **Parallel Parsing**: The graph header `n m` is read first to size data structures. The edge lines are partitioned across worker threads aligned to newline boundaries. Each thread parses unsigned 32-bit integers using a fast ASCII number parser while tracking local node out-degrees.
3. **Lock-Free Forward CSR**: Node degrees are aggregated across workers and prefix-summed into an `offsets` array (`n + 1` entries). Per-worker write pointers are derived in-place, allowing worker threads to scatter their parsed edges into contiguous `heads` and `weights` arrays concurrently without locks or intermediate edge structures.
4. **Reusable Dijkstra State**: Queries are dispatched dynamically to a pool of worker threads (up to 12 workers) using an atomic query counter. Each worker maintains persistent search state: a `u64` distance array initialized once to infinity, a `touched` node list, and a standard binary heap. Settling queries with Dijkstra stops immediately when target $t$ is popped. After each query, only nodes in the `touched` list are reset to infinity, eliminating $O(n)$ re-initialization overhead.
5. **Deterministic Output**: Answers are stored directly into pre-indexed atomic result slots. Answers are formatted in input order into a single contiguous output buffer and written to `/out/distances.txt`.

## Dependencies

None (standard library only).

## With another 24 hours

1. **Fast-path query short-circuits**:
   - Immediate zero-distance answer when $s == t$.
   - Degree-based reachability pruning: if $s \neq t$ and $s$ has out-degree 0 or $t$ has in-degree 0, return -1 without searching.
2. **Bidirectional Dijkstra**:
   - Construct reverse CSR alongside forward CSR.
   - Alternate search frontiers between source and target, terminating when the sum of minimum frontier distances reaches the best meeting distance.
3. **Query Deduplication**:
   - Cache results for repeated query pairs.
4. **Locality Layout**:
   - Renumber nodes by BFS or Hilbert order to improve CPU cache hit rates during graph traversal.
