# Track A performance log

The complete prior log is preserved in [PERF-history.md](../tools/track-a/PERF-history.md). Historical performance rows below summarize each retained or reverted idea; the linked file has the earlier details and checks.

| Step | Change | Evidence | Board run ID | Result |
|---|---|---|---|---|
| Reference | Organizer Python baseline | T 51.906 s; M 198.9 MB; supplied in the original run record | run-30c2dd2e0904 | Reference |
| A1 | mmap, newline-aligned workers, per-worker endpoint interning | Original full seed 1 hash matched | RUN-ID-TBD | Kept |
| A2 | Parallel group merge | Did not corroborate wall-time gain in phase profile | local WSL measurement | Reverted |
| A3 | Parallel hour-range formatting and positioned writes | Formatting phase fell from 25.560 to 6.509 ms; local 10-run median from 0.290 to 0.245 s | local WSL measurement | Kept |
| A4 | Weight selection ranges by record count | Selection phase fell from 33.547 to 11.871 ms | local WSL measurement | Kept |
| A5 | Hour-major parallel merge | Local median fell from 0.240 to 0.180 s | local WSL measurement | Kept |
| A6 | AVX2 delimiter scan | Local median fell from 0.180 to 0.155 s; hashes matched | local WSL measurement | Kept |
| A7 | Three rank selections | Same direct median; instrumented total increased | local WSL measurement | Reverted |
| A8 | Arithmetic day index | Initial gain did not repeat | local WSL measurement | Reverted |
| A9 | Reserve local latency vectors | No corroborated gain; RSS rose | local WSL measurement | Reverted |
| A10 | Chunked endpoint hashing | Local median 0.155 to 0.120 s | local WSL measurement | Kept |
| A11 | `write_vectored` | Same direct median; no write-phase gain | local WSL measurement | Reverted |
| A12 | Pre-size merged vectors | Merge time and RSS increased | local WSL measurement | Reverted |
| A13 | Move first partial latency vector | Local median 0.120 to 0.110 s | local WSL measurement | Kept |
| A14 | Shared-mmap output copy | Slower with more major faults and higher RSS | local WSL measurement | Reverted |
| A15 | High-bit hash-table indexing | Local median 0.150 vs 0.120 s | local WSL measurement | Reverted |
| A16 | SWAR latency conversion | Parse time unchanged within measurement noise | local WSL measurement | Reverted |
| A17 | Open-addressed endpoint table | Slower despite lower RSS | local WSL measurement | Reverted |
| A18 | Borrow endpoint slices from mmap | Merge profile and RSS improved; hashes matched | local WSL measurement | Kept |
| A19 | Prehashed endpoint keys | Local median 0.130 vs 0.110 s | local WSL measurement | Reverted |
| A20 | Compile-time AVX2 dispatch | Parse phase improved; full hashes and samples matched | local WSL measurement | Kept |
| A21 | Fused delimiter hash and endpoint table | Local median 0.145 vs 0.110 s | local WSL measurement | Reverted |
| A22 | Rotate-xor chunk hasher | Same direct median; lower user time in profile | local WSL measurement | Kept |
| A23 | Dense per-worker counters and packed records with prefix/scatter | 15 alternating full seed 1 runs: baseline min/median 121.142/137.124 ms, candidate 136.079/149.736 ms; RSS proxy max 43,245/50,153 KB; code-line S 497/540. All hashes matched. | local WSL measurement | Reverted; failed the required 10% minimum-wall gain |
| A24 | Buffered `pread` input | Warm ext4 parse and total time were slower than mmap | local WSL measurement | Reverted |

The latest candidate passed the worked example, empty input, sample seeds 2/3/7, full seed 2, boundary fixtures, single-line input, and the 5,000-endpoint ECAP-growth fixture. The final source is the saved mmap baseline because the packed-record rewrite missed the timing gate and raised the RSS proxy. No Docker check was run.
