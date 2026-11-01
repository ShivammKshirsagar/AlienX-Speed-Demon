# Track C: Join

## Approach

The solution is a zero-dependency Rust implementation (`std` only, 0 crates) optimized for correctness, memory efficiency, and throughput across 12 worker threads.

1. **Input Mapping & Direct Region Lookup:**
   - `records.csv` is memory-mapped read-only via platform FFI (`mmap`/`munmap`/`madvise`) to eliminate user-space file buffering and heap duplication.
   - `regions.csv` is parsed once into a flat direct-lookup array indexed by `region_id`. Missing, unknown, or out-of-range IDs route directly to the `_UNMATCHED` slot (index 240). Distinct 3-letter country codes are sorted lexicographically at load time so the output groups are emitted in ascending bytewise order with `_UNMATCHED` naturally last.

2. **Chunk Partitioning & Multithreading:**
   - The mapped byte range of `records.csv` is divided into 12 chunks using `std::thread::scope`.
   - By spec and Phase 0 verification, every newline (`\n`) followed by an optional BOM (`\xEF\xBB\xBF`) and an ASCII digit is guaranteed to be a true record start, and internal note newlines only precede lowercase ASCII characters (`[a-z]`). Each thread advances to its first verified record boundary, parsing records that start within its nominal chunk and completing any record crossing its chunk boundary.

3. **In-situ Parsing & Group Aggregation:**
   - Numerical fields (`id`, `region_id`, `amount`) are parsed directly from ASCII bytes using exact integer arithmetic (cents) without float operations.
   - Quoted fields (`name`, `note`) are parsed in-place without heap unescaping: unescaped length is computed directly as raw slice length minus escaped quote count divided by two.
   - Unique names are tracked per country using an open-addressing 64-bit hash table with Fibonacci mixing and FNV-1a 64-bit hashing. Empty quoted and unquoted names map identically to the FNV initial basis.
   - Thread-local country aggregators and hash tables are merged into the final aggregates at completion.

4. **Output Emission:**
   - Aggregates are written with `BufWriter` directly to `/out/aggregates.txt` with formatted integer cents (`[-]int.2digits`), preserving standard library formatting and LF terminators.

## Dependencies

None. Pure Rust standard library only (`dependency_count = 0`).

## With another 24 hours

With another 24 hours, I would:
- Add SIMD-accelerated field parsing (using AVX2/AVX-512 via `std::arch::x86_64` to locate commas, quotes, and newlines in 32-byte blocks).
- Implement a 2-pass parallel name interning table to compress the per-thread hash sets and reduce memory merge overhead further.
- Fine-tune chunk balance dynamically using a work-stealing queue or atomic chunk cursor for unevenly sized record distributions.
