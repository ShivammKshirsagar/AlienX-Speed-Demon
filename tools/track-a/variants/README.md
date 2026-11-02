# Track A board variants

Each variant is a complete offline Docker build context derived from `baseline-snapshot2`. Base source SHA-256: `71cf21e17af0ff9b2e672943c3444c346ba5915f493dee68f8424820a59a2157`. H173 (SHA-256 `cf8b969ca1c6324bb9e5d22e8cffc5f36f9ebb161b08c7b7916e30ff8b9863a7`, S=555) scored 66.326361 and is rejected below v7. H174 is now the output-path candidate; the best remains 67.289394.

| Variant | Change | Rust + Docker S estimate | Max RSS proxy | Checks | Exact push procedure |
|---|---|---:|---:|---|---|
| v1 | Skip destructors after output; call `_exit(0)` | 499 | 41,942 KB | 12/12 passed | Copy all contents of `tools/track-a/variants/v1/` over `track-a/`, replacing files; commit **Exit path**. |
| v2 | Arithmetic hour, 8-byte SWAR latency, paired AVX2 space/LF masks | 524 | 41,962 KB | 12/12 passed | Copy all contents of `tools/track-a/variants/v2/` over `track-a/`, replacing files; commit **Fast parser**. |
| v3 | Growing linear-probe endpoint table; full key comparison on matches | 530 | 41,982 KB | 12/12 passed | Copy all contents of `tools/track-a/variants/v3/` over `track-a/`, replacing files; commit **Endpoint table**. |
| v4 | Two-pass per-worker latency arena removes per-group parse-time Vec growth | 528 | 36,862 KB | 12/12 passed | Copy all contents of `tools/track-a/variants/v4/` over `track-a/`, replacing files; commit **Arena storage**. |
| v5 | Trivial n=1, sort through 48, exact `select_nth_unstable` ranks above 48 | 512 | 42,062 KB | 12/12 passed | Copy all contents of `tools/track-a/variants/v5/` over `track-a/`, replacing files; commit **Partial selection**. |
| v6 | 48 newline-aligned chunks with an atomic worker queue | 519 | 49,634 KB | 12/12 passed; **excluded** | Do not push: proxy exceeded base by 7,136 KB. |
| v7 | v1 exit path + v2 fast parser, bundled as one board candidate | 526 | 42,022 KB | 12/12 passed | Copy all contents of `tools/track-a/variants/v7/` over `track-a/`, replacing files; commit **Fast parse exit**. |
| v8 | v7 plus v5 exact percentile selection | Not measured | Not measured | Board scored 67.105; below v7 | Rejected; v7 remains fallback. |
| c3 | Shared-mapped output with positioned-write fallback | See `tools/track-a/candidates/c3/README.md` | See candidate README | Board score 62.596; rejected below v7. |
| H142 | Compact groups, cached endpoint hash, worker split, huge-page hint | S=555 | 64.679018 points | Prior full-hash and fixture checks passed | Rejected; run-dc3e534c6b5b, below v7. |
| H83 | CRC32C hash plus avalanche, 9 parse workers, 5 post workers | S=558 | 64.497525 points | Board gates passed | Rejected; run-95ac2b92059b. |
| H173 | H142 compact group design with 12 parse/12 post workers | S=555 | 66.326361 points | Board gates passed | Rejected; run-388df169828f. |
| H174 | H173 compact groups plus parallel fixed-hour output and `_exit` | S=559 | 66.738816 points | Board gates passed | Rejected below v7; run-4dc5f1362b5c. |
| H175 | H174 exact output offsets with per-hour streaming parallel writes | S=575 | Not claimed | `cargo fmt --check` passed | Current candidate; board run pending; source SHA256 `1de674070ef9782588ed669c3a95d7393698a75444b31f99acdb8a7ce0198aee`. |

v6 passed correctness but failed the required at-or-below-base memory limit, so it is not included as a pushable `v6/` folder. v7 combines two previously isolated changes because the parse optimization targets the measured dominant phase and the exit change adds no second scan. v7 retained the pinned Docker `FROM` lines and `RUSTFLAGS`; no local timing result was used to choose it. Its memory proxy is one full seed-1 Docker process high-water sample minus the 238,836,234-byte input mapping. Use paired board runs as the final judge.
