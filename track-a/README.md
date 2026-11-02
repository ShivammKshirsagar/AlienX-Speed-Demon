# Track A: Ingest

H175 and H176 reduced output memory but scored 65.312968 and 64.487523 because output time regressed. H177 returns to H174's single-wave parallel path and reserves each buffer from active rows; this adds an output-size pass but should avoid geometric buffer growth.

AVX2 scans delimiters when the release target supports it, with a scalar fallback for other builds and for the tail of each range. Percentile ranks and error rates use exact integer arithmetic.

## Dependencies

None. The solution uses Rust's standard library and small system calls for file mapping and positioned output writes.

## Board status

H83, H173, H174, H175, and H176 remain below v7's best 67.289394. H177 targets lower memory without reducing output parallelism; the goal remains 71.
