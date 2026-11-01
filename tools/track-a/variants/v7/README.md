# Track A candidate v7

This candidate combines the previously isolated v1 exit path and v2 parser change: arithmetic hour parsing, an 8-byte SWAR latency parse, paired AVX2 delimiter masks, then `_exit(0)` immediately after output writes. It has no dependency or input-specific behavior. The pinned Docker `FROM` lines and `RUSTFLAGS` are unchanged.

## Gate results

- Built offline from this folder's Docker context.
- Passed all 12 checks: worked example, empty input, single line, 12-worker chunk boundaries, equal timestamps, hour boundaries, 5,000 endpoints against the reference, sample seeds 2/3/7, full seed 2 against the reference, and full seed 1 hash `495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3`.
- Full seed 1 Docker `VmHWM`: 275,260 KB; input mapping: 238,836,234 bytes (233,238.5 KB); mapping-subtracted RSS proxy: 42,022 KB. Control proxy: 42,498 KB; candidate is below the control + 3 MB gate.
- Estimated organizer source size: S=524 (control S=497; limit S<=557).
- No local timing claim is made. WSL is unavailable, and Docker bind-mount time is not used to rank candidates.

## Manual board push

The active `track-a/` remains the control. For candidate C, replace the contents of `track-a/` with the complete contents of this `v7/` folder, commit as **Fast parse exit**, then push. Record the resulting board run ID and T/M/S/K in `tools/track-a/BOARD_LOG.md`. Restore the exact control before candidate K and its following C run. Do not treat this candidate as a winner until the adjacent C/K/C board comparison clears the mission gate.
