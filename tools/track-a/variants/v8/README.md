# Track A candidate v8

This candidate combines the board-tested v7 parser/exit path with v5's exact percentile selection. Both components passed the earlier full correctness suite independently. The combined source passes `cargo fmt --check`; a combined build and fixture sweep are still pending because WSL failed to start with `E_ACCESSDENIED`. No score or timing is claimed. The pinned Docker `FROM` lines and `RUSTFLAGS` are unchanged from v7.

## Board status

The best board result remains v7 at 67.289 points. This candidate must be scored on the live board before it can replace v7. Record the board run ID and full T/M/S/K/points in `track-a/PERF.md` after the run.

## Push

The workspace `track-a/` currently contains this source. After confirming the board build succeeds, commit it as **Fast Percentiles** and push to the `speed-demon` branch.
