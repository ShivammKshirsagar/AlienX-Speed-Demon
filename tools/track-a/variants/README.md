# Track A board variants

Each delivered folder is a complete offline Docker build context derived independently from `baseline-snapshot2`. Base source SHA-256: `71cf21e17af0ff9b2e672943c3444c346ba5915f493dee68f8424820a59a2157`. The restored `track-a/src/main.rs` matches that base. Base board S is 497; variant S estimates add the measured source-code line delta. RSS proxy is Docker Linux `VmHWM` minus the 238,836,234-byte input mapping; base proxy was 42,498 KB. All delivered variants were built offline and passed all 12 checks: worked example, empty, single line, chunk boundaries with 12 CPUs, equal timestamps, hour boundaries, 5,000 endpoints, sample seeds 2/3/7, full seed 2 vs reference, and the full seed 1 expected hash.

| Variant | Single change | Rust + Docker S estimate | Max RSS proxy | Checks | Exact push procedure |
|---|---|---:|---:|---|---|
| v1 | Skip destructors after output; call `_exit(0)` | 499 | 41,942 KB | 12/12 passed | Copy all contents of `tools/track-a/variants/v1/` over `track-a/`, replacing files; commit **Exit path**. |
| v2 | Arithmetic hour, 8-byte SWAR latency, paired AVX2 space/LF masks | 524 | 41,962 KB | 12/12 passed | Copy all contents of `tools/track-a/variants/v2/` over `track-a/`, replacing files; commit **Fast parser**. |
| v3 | Growing linear-probe endpoint table; full key comparison on matches | 530 | 41,982 KB | 12/12 passed | Copy all contents of `tools/track-a/variants/v3/` over `track-a/`, replacing files; commit **Endpoint table**. |
| v4 | Two-pass per-worker latency arena removes per-group parse-time Vec growth | 528 | 36,862 KB | 12/12 passed | Copy all contents of `tools/track-a/variants/v4/` over `track-a/`, replacing files; commit **Arena storage**. |
| v5 | Trivial n=1, sort through 48, exact `select_nth_unstable` ranks above 48 | 512 | 42,062 KB | 12/12 passed | Copy all contents of `tools/track-a/variants/v5/` over `track-a/`, replacing files; commit **Partial selection**. |
| v6 | 48 newline-aligned chunks with an atomic worker queue | 519 | 49,634 KB | 12/12 passed; **excluded** | Do not push: proxy exceeded base by 7,136 KB. |

v6 passed correctness but failed the required at-or-below-base memory limit, so it is not included as a pushable `v6/` folder. All six retained the pinned Docker `FROM` lines and `RUSTFLAGS`; no timing results were used to choose candidates. The memory figure is a single full seed-1 process high-water sample per image, with a repeat base/v6 sample; use the board as the final judge.
