# Track A: Ingest

H175, H176, and H177 scored 65.312968, 64.487523, and 65.874440. H177's active-row capacity scan reduced memory but cost time. H178 skips that scan and uses a rough allocation hint while keeping the same parallel output path.

AVX2 scans delimiters when the release target supports it, with a scalar fallback for other builds and for the tail of each range. Percentile ranks and error rates use exact integer arithmetic.

## Dependencies

None. The solution uses Rust's standard library and small system calls for file mapping and positioned output writes.

## Board status

H83, H173, H174, H175, H176, and H177 remain below v7's best 67.289394. H178 tests a cheaper buffer reserve; the goal remains 71.
