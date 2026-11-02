# Track A: Ingest

The current candidate maps `requests.log` read-only and divides it into up to seven newline-aligned parser ranges. Each parser interns endpoint slices with a cached fingerprint and exact byte equality, then uses a compact direct index for hour/endpoint groups. Five workers handle the later merge, percentile, and output phases. It requests huge pages for the input mapping as a best-effort kernel hint; correctness does not depend on the hint.

AVX2 scans delimiters when the release target supports it, with a scalar fallback for other builds and for the tail of each range. Percentile ranks and error rates use exact integer arithmetic.

## Dependencies

None. The solution uses Rust's standard library and small system calls for file mapping and positioned output writes.

## With another 24 hours

The H142 candidate has passed the recorded full-hash and fixture checks but has not yet been scored on the board. The current board best is 67.289 points; the next change should be judged by the real board result, with a target of at least 70.
