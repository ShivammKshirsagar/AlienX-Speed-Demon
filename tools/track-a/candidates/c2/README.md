# Track A: Ingest

The program maps `requests.log` read-only and divides its bytes into up to twelve newline-aligned ranges. Each worker parses its range, interns endpoint slices borrowed from the input mapping, and gathers latency values and 5xx counts by hour and endpoint. The local tables are combined by hour, percentile work is split across workers, and output is formatted in parallel. When available, a shared output mapping lets workers copy disjoint output ranges in parallel; positioned writes are the fallback. Endpoint ordering is bytewise, percentile ranks follow the specification, and counts and error rates use integer arithmetic.

The endpoint table uses a deterministic hasher. Hash collisions remain safe because complete endpoint bytes are compared. AVX2 scans delimiters when the release target supports it, with a scalar fallback for other builds and range tails. The process exits with `_exit` after output so it does not spend time destroying data that is no longer needed.

## Dependencies

None. The solution uses Rust's standard library and small system calls for file mapping and positioned output writes.

## With another 24 hours

I would validate the selected shared-mmap candidate on the scoring Linux image. The later flat-state/persistent-pool variants remain available in `tools/track-a/candidates/` for board comparison, but their larger source and RSS reduced the current local estimated K despite faster parser/scatter phases.
