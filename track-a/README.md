# Track A: Ingest

The current candidate maps `requests.log` read-only and divides it into up to nine newline-aligned parser ranges. Each parser interns endpoint slices with a CRC32C hash plus avalanche and exact byte equality, then uses a compact direct index for hour/endpoint groups. Five workers handle the later merge, percentile, and output phases.

AVX2 scans delimiters when the release target supports it, with a scalar fallback for other builds and for the tail of each range. Percentile ranks and error rates use exact integer arithmetic.

## Dependencies

None. The solution uses Rust's standard library and small system calls for file mapping and positioned output writes.

## Board status

H83 is the current board candidate. H142 scored 64.679 points and was rejected. The best known board result remains 67.289 points; the active target is at least 71.
