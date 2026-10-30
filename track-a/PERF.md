# Track A: performance log

Measurements below were collected on the available Windows development host, not the scoring i5-13500 Linux container. Wall time includes process start through exit. Memory is peak process working set; mapped input pages are included, unlike the board's stated preloaded-input treatment. The reference row is a local run measured by the same PowerShell harness. These measurements are useful for local comparisons but are not a board result.

| Step | Change | Board run ID | T (s) | M (MB) | Result |
|---|---|---|---:|---:|---|
| 0 | Organiser Python reference, unchanged | TBD | 51.906 | 198.9 | Full seed 1 hash matched |
| 1 | Rust: read-only mapping, parallel parsing, local endpoint interning, parallel group sorting, one deterministic output pass | TBD | 0.427 median of 5 | 275.6 median of 5 | Kept; all five full seed 1 hashes matched |

The local Rust runs were 0.398, 0.413, 0.427, 0.432, and 0.466 seconds. Every run wrote SHA-256 `495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3`.

## Correctness runs

- Worked example from `specs/a-ingest.md`: byte-for-byte pass.
- Seed 2, sample: Rust and Python reference both produced `fd26778fa5e70fac2905a0a0f0142bd26ca4e6f510dfe182a881e525286ff12f`.
- Seed 3, sample: Rust and Python reference both produced `db28ff42849be9f28fcc7cbac23b6c292e2c0810c054056a2bdbffe6f7321dac`.
- Seed 7, sample: Rust and Python reference both produced `39bfccc5334c93cbe0c762f5e6c3300e91ca2e266567561da3176c038cee49f7`.
- Seed 1, full: Rust and Python reference both produced `495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3`.
- Seed 2, full: Rust and Python reference both produced `fe3469ff108066bd7b65f8239bbe27615467b99db182b79919c572c3d2910127`.

## K estimate

Using the event's reference values (32.05 s, 228 MB), the local Rust median (0.427 s), the Windows working-set proxy (275.6 MB), zero dependencies, and 303 current physical lines across the Rust source, Dockerfile, and `.dockerignore` gives an indicative `K ≈ 6.48` (about 40.6 machine points). This is not a valid board score: the timing host and memory metric differ from the scoring environment, and `scc` plus the organiser's Rust formatter were unavailable here, so the scored source-line count is not verified.

No performance experiments were reverted. The board-equivalent `check.py` run remains unverified. An offline Docker build was attempted, but Docker tried to pull the pinned base images despite `--network=none`; I interrupted it to honor the no-network constraint. The local binary was checked directly against the organiser reference instead.
