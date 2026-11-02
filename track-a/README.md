# Track A: Ingest

H175 keeps H173's compact groups and 12 workers. It computes exact per-hour output offsets, then formats and writes one hour at a time from each worker, avoiding retained whole-output buffers. Board result pending; H174 scored 66.738816, and the best known remains v7 at 67.289394.

AVX2 scans delimiters when the release target supports it, with a scalar fallback for other builds and for the tail of each range. Percentile ranks and error rates use exact integer arithmetic.

## Dependencies

None. The solution uses Rust's standard library and small system calls for file mapping and positioned output writes.

## Board status

H83, H173, and H174 scored 64.497525, 66.326361, and 66.738816. H175 targets output-buffer memory while retaining parallel writes; the goal remains 71.
