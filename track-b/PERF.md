# Track B: performance log

All timings below are native WSL2 runs with `/data` on the WSL ext4 filesystem, the input cache warmed by `cat /data/docs.txt >/dev/null`, and execution pinned with `taskset -c 0-11`. Each retained variant has five full seed-1 runs; every run produced the expected SHA-256 `f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927`. RSS is the Linux `/usr/bin/time -v` maximum in KB and includes resident mmap pages. Board run IDs are unavailable locally; each remains `TBD`.

| Step | Change | Board run ID | Median wall (s) | User (s) | Sys (s) | Max RSS (KB) | Major/minor faults | Phase medians: parse / pass A / pass B / query / output (ms) | Result |
|---|---|---|---:|---:|---:|---:|---:|---|---|
| 0 | Single-threaded scans and query evaluation | TBD | 52.53 | 52.30 | 0.20 | 332668 | 0 / 25967 | 37 / 5591 / 7115 / 39729 / not instrumented | Baseline |
| 1 | Parallel query blocks, up to 12 workers | TBD | 17.50 | 51.65 | 0.29 | 337276 | 10 / 27245 | 46 / 5964 / 7359 / 3837 / 1 | Kept; 66.7% faster, full hash passed |
| 2 | Newline-aligned parallel corpus scans and disjoint postings fills | TBD | 5.54 | 56.18 | 0.22 | 375780 | 11 / 37794 | 42 / 586 / 1071 / 3736 / 1 | Kept; 68.3% faster, full hash passed |
| 3 | In-place bitmap updates and 16-bit result sum tables | TBD | 5.72 | 57.63 | 0.26 | 375808 | 12 / 37782 | 37 / 617 / 1050 / 3970 / 1 | Reverted; 3.2% slower than step 2 despite full hash pass |

Final source, after adding `/out/results.txt` output, ran three consecutive full seed-1 checks through the Linux WSL binary. All three hashes matched; median was 5.86 s wall, 58.63 s user, 0.30 s system, 375424 KB max RSS, and 13 major / 37760 minor faults. Median phases were 43 ms query parse, 640 ms pass A, 1105 ms pass B, 4102 ms query evaluation, and 6 ms output. Rust plus Dockerfile is 648 lines. The local formula proxy is K˜3.41 using this WSL median, 375.424 MB RSS, and S=648; it is not a board score.

The worked example matched both stdout and `results.txt`. Held-out sample comparisons against the Python reference passed:

- Seed 2: `86b92e3ceda664396bad5cabc3809b447de0610282eee484605fd23cbd12fea6`
- Seed 3: `d62a5112e713dd915c35129f31190d9566b8e76ed8615cf96bcd893e4ee4d197`

Reproduction from WSL after building with `RUSTFLAGS='-C target-cpu=x86-64-v3' cargo build --release --offline`:

```sh
cat /data/docs.txt >/dev/null
bash '/mnt/c/GitHub Desktop/AlienX-Speed-Demon/tools/track-b/profile-five.sh' parallel-scan '/mnt/c/GitHub Desktop/AlienX-Speed-Demon/track-b/target/release/solution'
```

Final scoring command requested by the brief: `python check.py b --data data\b --expected f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927`.
