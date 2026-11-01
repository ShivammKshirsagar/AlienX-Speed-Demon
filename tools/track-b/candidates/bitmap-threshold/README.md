# Bitmap break-even candidate

- Change: classify a query term as a bitmap only when sampled document frequency reaches the u32-list/bitmap space break-even; sampling changes representation only, and the full scan fills exact results.
- Build: offline Docker build passed; plain `cargo fmt` applied.
- Same-session interleaved full seed-1 results (5 runs): total min/median 878.401/942.884 ms; `memory.peak` median/max 146,321,408/147,185,664 B; S=1,000.
- Relative estimated K vs baseline (S=998, T min=951.967 ms, M median=146,227,200 B): ×1.0406; clears the 3% threshold.
- Checks: worked example PASS; phrase-boundary PASS; full seed-1 hash PASS (`f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927`).
- Samples 2–5: not run; the B sample inputs/reference outputs are absent from this workspace and Docker volumes.
- To board-test: copy this folder's contents over `track-b/`, then push that commit.
