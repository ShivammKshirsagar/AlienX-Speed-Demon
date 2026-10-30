# Track A: Ingest

The Rust program memory maps `requests.log` read-only and divides it into up to 12 newline-aligned byte ranges. Each worker parses the fixed-width date and hour directly from the timestamp, interns endpoint bytes in a local table, and accumulates latency and 5xx counts by hour and local endpoint ID. The main thread merges those tables, sorts each group's latencies in parallel, and writes groups in hour and unsigned-byte endpoint order. Percentiles use the spec's nearest-rank indices, and all output numbers use integer arithmetic.

The fast hasher is local to this process and only hashes endpoint strings. Hash hits still compare the full endpoint bytes, so collisions do not merge distinct keys. Both Unix and Windows input paths use read-only file mappings; an empty input produces an empty output.

## Dependencies

None. The program uses Rust's standard library and small platform FFI declarations for file mapping.

## With another 24 hours

I would build and benchmark on the scoring Linux image first, then compare 6, 10, and 12 workers and a counting-sort percentile path against the current full sort. I would keep a change only if it improves the correctness-checked board measurements. The local Windows timing and working-set figures are useful for iteration but do not predict the board's cgroup memory or CPU throughput.
