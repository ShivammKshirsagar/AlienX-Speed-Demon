# Pair-buffer ownership candidate

- Change: scatter workers take ownership of each sorted pair buffer and drop it on worker completion.
- Build: offline Docker build passed; plain `cargo fmt` applied.
- Same-session interleaved full seed-1 results (5 runs): total min/median 911.846/963.464 ms; `memory.peak` median/max 146,022,400/146,530,304 B; S=1,000.
- Relative estimated K vs baseline (S=998, T min=951.967 ms, M median=146,227,200 B): ×1.0217; rejected by the 3% threshold.
- Checks: worked example PASS; phrase-boundary PASS; full seed-1 hash PASS (`f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927`).
- Samples 2–5: not run; the B sample inputs/reference outputs are absent from this workspace and Docker volumes.
- To board-test: copy this folder's contents over `track-b/`, then push that commit.
