# Track A board candidates

The source snapshots are standalone Rust/Docker build contexts. Dockerfiles retain the pinned base images and `RUSTFLAGS`. Values are local WSL measurements, not board results. Board IDs are intentionally absent until the author runs each candidate.

## c1 — flat-state, persistent-pool, fast-parser variant

Copied from `track-a/`. It includes `_exit`, shared mapped output, flat per-worker state and parallel scatter, the persistent worker team, fast parsing, and partial selection.

| Phase | Instrumented wall ms |
|---|---:|
| Parse | 60.004 |
| Merge/scatter | 37.455 |
| Selection | 4.342 |
| Format | 4.469 |
| Write | 13.189 |
| Main to exit | 130.127 |

Its latest recorded direct comparison was 105.656 ms min / 123.060 ms median, with about 52,586 KB max RSS proxy. Seed-1 full hash was checked in alternating A/B runs. Worked example, empty input, and sample seed 2 were checked after the pool/parser changes. The final seed/reference/fixture sweep is pending because WSL launch currently fails with `Wsl/Service/E_ACCESSDENIED`.

## c2 — persistent pool before fast parser

This is the earlier persistent-pool version from `steps/step-f`; it is a distinct comparison candidate. Its A/B result against its preceding version was 115.007 ms min / 128.132 ms median, max RSS proxy 52,586 KB. The full phase table was not captured for this snapshot. Seed-1 full hashes and quick example/empty/sample-2 checks were recorded; the full final fixture sweep is pending.

| Phase | Wall ms |
|---|---:|
| Parse / merge-scatter / selection / format / write / main-to-exit | Not captured for this exact snapshot |

## c3 — selected shared-mmap candidate

This is the B2 version currently copied to `track-a/`. It includes `_exit(0)` and shared mapped output with positioned-write fallback. Its 20-run A/B result was 82.814 ms min / 105.503 ms median, max RSS proxy 50,034 KB. Rust plus Dockerfile S is 539 by the local nonblank/noncomment count. No matching phase table was captured for this exact snapshot. Seed-1 hashes passed all 20 A/B runs; the final reference and edge-fixture sweep is pending.

| Phase | Wall ms |
|---|---:|
| Parse / merge-scatter / selection / format / write / main-to-exit | Not captured for this exact snapshot |

Using the supplied board conversion `T_board = 0.55 × WSL min`, the local RSS proxy converted from KiB to MiB, and the supplied formula, its estimated K is about 25.7 (about 70.5 points). This remains a local estimate; board T/M/S determine the real score.

## 2026-11-01 ruler results

The final ruler ran 30 interleaved rounds each of baseline, n1, n2, and n3, checking the expected seed-1 hash on every run. The three candidates also passed the full fixture/reference suite, but all failed the estimated-K gate. They are retained here as reviewable experiment snapshots only; do not copy them over `track-a/` or push them. `track-a/src` remains the baseline snapshot2 (S 497).

- **n1:** one-pass packed records, local endpoint tables, parallel prefix/scatter; S 970; median cgroup `memory.peak` 292.32 MB; all requested checks passed; CPU min/median 1.435/1.759 s, wall 1.045/1.312 s, relative K 0.970; rejected (min CPU gain 13.9%, K lower); push steps: none, keep snapshot2; commit suggestion: **No commit**.
- **n2:** n1 plus previous-endpoint cache; S 970; median cgroup `memory.peak` 293.03 MB; all requested checks passed; CPU min/median 1.467/1.704 s, wall 1.116/1.252 s, relative K 0.959; rejected (min CPU gain 11.94%, below 12% gate, K lower); push steps: none, keep snapshot2; commit suggestion: **No commit**.
- **n3:** two-pass count/fill with replayed chunk ownership; S 967; median cgroup `memory.peak` 277.01 MB; all requested checks passed; CPU min/median 1.724/2.048 s, wall 1.099/1.301 s, relative K 0.895; rejected (min CPU 3.5% slower, K lower); push steps: none, keep snapshot2; commit suggestion: **No commit**.
