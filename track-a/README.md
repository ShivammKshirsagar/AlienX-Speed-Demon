# Track A: Ingest

The current candidate is H173, derived from H142's compact group storage and changed to use up to twelve parse workers and twelve later-phase workers. It maps `requests.log` read-only, scans newline-aligned ranges, and keeps the pinned x86-64-v3 target. Source S=555; board result pending. The known best remains v7 at 67.289394 points.

AVX2 scans delimiters when the release target supports it, with a scalar fallback for other builds and for the tail of each range. Percentile ranks and error rates use exact integer arithmetic.

## Dependencies

None. The solution uses Rust's standard library and small system calls for file mapping and positioned output writes.

## Board status

H83 scored 64.497525 points and was rejected. H173 is pending; the best known board result remains 67.289 points and the active target is at least 71.
