# Track A optimization loop state

- Goal remains at least 80 board points; local target K_pred >= 41 and S <= 560. No local number is a board result.
- Current best shipped source H164 includes the H159 path plus #[inline(always)] on push_uint. SHA256 DBB4E2CCBD8AB7CAE22DCAF68B934C6FE3662BA571C2848A2F3E3442F46F0C65; S=560.
- H164 same-session named-volume comparison vs H159 (15 pairs): min wall 124.511/127.894 ms, CPU-at-min 616.607/476.585 ms; median wall 138.077/134.701 ms, median CPU 596.635/590.097 ms; RSS proxy 34,325.5/34,161.5 KB. Direct named-volume baseline calibration (15 interleaved pairs): H164 T_pred˜0.0492 s, M_pred˜38.532 MB, S=560, K_pred˜25.880 (~70.649 estimated machine points). Local estimate only; no board run.
- H164 passed worked/empty/single/chunk-boundary, samples 2/3/7, edge/exact, 5,000 endpoints, full seed 2 Python reference and 20 full seed-1 hashes; cargo fmt --check passed.
- H160-H163 were rejected and logged in tools/track-a/EXPERIMENTS.md. S budget is fully used; next structural candidate must remove counted lines or replace existing code while staying <=560.
- Goal remains unmet. Continue only under track-a/ and tools/track-a/, no dependencies/data-specific behavior/git/push, pinned Dockerfile FROM and RUSTFLAGS unchanged, no diagnostics in shipped code.
- H164 tools-only phase sample was noisy: parse 97.532 ms, merge/scatter 25.731 ms, selection 20.055 ms, output 43.924 ms, total 187.397 ms; parse remains largest, and timers inflate total.
- H165 endpoint equality inlining was rejected: same-session min-metric K gain was only ~0.84% after RSS, below threshold; H164 remains current best.
- H166 (8 parser workers) was rejected vs H164: median wall/CPU and min-based estimate regressed; RSS proxy +108 KB.
- H167's parser inlining was rejected after median regressions and a lower direct-baseline estimate; H168's previous-endpoint cache, H169 CRC32C hash and H170 sparse-hour rows were also rejected (timing or S cap). H164 remains best shipped.
- H171 and H172 were rejected. Search paused at the author's request; best push-ready source remains H164 in track-a/src, hash DBB4E2CCBD8AB7CAE22DCAF68B934C6FE3662BA571C2848A2F3E3442F46F0C65. Direct local baseline estimate: T˜0.0492 s, M˜38.532 MB, S=560, K˜25.880 (~70.649 estimated points); not a board result. 80-point goal remains unmet.

- Board result received: H164 run-0e251929474d scored 65.398 pts, below b4d1b99's 66.975. Restored track-a/src from baseline-snapshot2 (SHA256 71CF21E17AF0FF9B2E672943C3444C346B); current source is the previously board-scored baseline pending any new run.

- New active target from the author: reach at least 70 points on the real board; do not stop at local estimates. H164's 65.398 board result is a rejected regression. The next push candidate is the prebuilt, independently checked v4 per-worker latency arena (source SHA256 FE5969EE5867965AB43712C7B9E595D4C49A6549424E732D24D4F1F92D13CD05). track-a/src/main.rs now matches v4; cargo fmt --check passes. No board result exists yet. Keep only a candidate that scores above the current real-board best, record its board ID/metrics, and use the next already-checked isolated variant only after recording this result. Do not use local timing estimates as evidence of board improvement.

- Priority clarification: the active milestone is strictly 70 real-board points. The 80-point goal is deferred and must not be resumed until a board result of at least 70 points is recorded. No board/scoreboard integration is available in this workspace; v4 is awaiting its board run.

- Board result: v4 scored 62.753 points (run ID and T/M/S/K not supplied), below the 66.975 baseline; reject it. Prepared next isolated candidate v2 (arithmetic/SWAR/AVX2 line parser), source SHA256 98D431ACF0BE0B55FB7D6CDC65CDD4031EE2FC882F3D9C4D4F15FD17B73162C0; cargo fmt --check passes. The only active milestone remains 70 real-board points; 80 is deferred.

- Board result: v2 scored 66.987 points (run ID and T/M/S/K not supplied), a 0.012-point gain over the 66.975 control but still below the 70 target. Prepared next isolated candidate v5 (partial percentile selection), source SHA256 BA278BEF42D32D49409735326CCF0405FA04997B0A6BAB02401F9E5368ABC35C; cargo fmt --check passes. v2 remains the best known board result until v5 is scored. Continue toward 70; 80 remains deferred.
