# Track A: Ingest

The program maps `requests.log` read-only and divides its bytes into up to twelve newline-aligned ranges. Each worker scans the fixed-width timestamp fields, interns endpoint slices borrowed from the mapping, and gathers latency values and 5xx counts by hour and endpoint. The local tables are combined by hour, percentile work is split across workers, and output is formatted in parallel. Endpoint ordering is bytewise, percentile ranks follow the specification, and counts and error rates use integer arithmetic.

The endpoint table uses a deterministic hasher. Hash collisions remain safe because complete endpoint bytes are compared. AVX2 scans delimiters when the release target supports it, with a scalar fallback for other builds and for the tail of each range.

## Dependencies

None. The solution uses Rust's standard library and small system calls for file mapping and positioned output writes.

## With another 24 hours

I would profile on the scoring Linux image and optimize the measured parse and merge costs first. The packed-record rewrite was checked against full and sample references, but its 15-run minimum was slower than the saved baseline and its RSS proxy was higher, so the baseline remains shipped. Further changes should pass the same correctness checks and demonstrate a repeatable end-to-end gain before replacing it.
