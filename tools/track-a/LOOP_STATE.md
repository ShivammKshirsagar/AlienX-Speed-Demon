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
