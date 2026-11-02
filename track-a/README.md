# Track A: Ingest

The current source is the previously board-scored v7 candidate: it maps `requests.log` read-only, divides it into up to twelve newline-aligned parser ranges, uses the arithmetic/SWAR parser with paired AVX2 delimiter masks, and exits immediately after writing output. Its best measured board result is 67.289394 points.

AVX2 scans delimiters when the release target supports it, with a scalar fallback for other builds and for the tail of each range. Percentile ranks and error rates use exact integer arithmetic.

## Dependencies

None. The solution uses Rust's standard library and small system calls for file mapping and positioned output writes.

## Board status

H83 scored 64.497525 points and was rejected. The best known board result remains 67.289 points; the active target is at least 71.
