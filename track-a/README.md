# Track A: Ingest

H175 streamed one hour per write and scored 65.312968 points (0.076954 s, 41,390,080 B); the memory savings did not repay the sizing pass and small writes. H176 batches the parallel output buffers into two waves, cutting retained output memory without a sizing pass or 72 small writes. H176 is S=560; board result pending.

AVX2 scans delimiters when the release target supports it, with a scalar fallback for other builds and for the tail of each range. Percentile ranks and error rates use exact integer arithmetic.

## Dependencies

None. The solution uses Rust's standard library and small system calls for file mapping and positioned output writes.

## Board status

H83, H173, H174, and H175 remain below v7's best 67.289394. H176 targets output-buffer memory while retaining parallel writes; the goal remains 71.
