# Track B: Performance Log

The first seven run IDs are listed in the order supplied and paired by that order with the chronological Track B changes in Git; Git does not store a direct run-to-commit link. Short commit IDs make each change traceable. Historical measurements are shown only when recorded in the project log or available in the current public board feed. M is shown in decimal MB.

| Step | Change | Run ID | T (s) | M (MB) | Points |
| ---: | --- | --- | ---: | ---: | ---: |
| 1 | Initial Track B Rust index (95589b1) | run-eef9f5261e85 | - | - | - |
| 2 | Parallel scan and query speedup (9970f66) | run-c29a5e5f8214 | 0.550 | 202.0 | 53.010 |
| 3 | AVX2 token scan optimization (b4d1b99) | run-40d91ecb01ed | - | - | - |
| 4 | Phrase boundary correctness fix (5d25379) | run-f72803a9af8d | - | - | - |
| 5 | Hot/cold term index pass (8bbe08a) | run-736196dfb2c8 | - | - | - |
| 6 | Software-pipelined scan (b418356) | run-cd4e1b46ddd0 | - | - | - |
| 7 | Packed pair scan buffers (c565241) | run-77f1a79eef5f | - | - | - |
| 8 | Best Track B result currently on the board | run-3d23ca96fd15 | 0.532 | 160.9 | 54.175 |
| 9 | Inline AVX2 scanner (latest board run; below the best) | run-65c4969654fd | 0.580 | 170.2 | 53.148 |