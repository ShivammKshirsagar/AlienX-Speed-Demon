# Track B diagnostic stages

Diagnostic builds exit after a selected pipeline boundary without producing results. `DIAG_STAGE` is diagnostic only; leave it at `0` for the final push. Stage 0 is the normal program and output path.

## Push procedure

1. Select a stage with `bash tools/track-b/set-diag.sh N` (`N` is 1, 2, or 3).
2. Build the board binary from `track-b/` with `RUSTFLAGS='-C target-cpu=x86-64-v3' cargo build --release --offline` and push that diagnostic build using the usual board workflow.
3. Record the board T, M, status, and run ID below. Diagnostic stages intentionally exit before writing query results.
4. Before the final push, run `bash tools/track-b/set-diag.sh 0`, rebuild, and submit the normal solution.

| Stage | Board T | Board M | Status | Run ID |
|---:|---:|---:|---|---|
| 1 | TBD | TBD | Not run | RUN-ID-TBD |
| 2 | TBD | TBD | Not run | RUN-ID-TBD |
| 3 | TBD | TBD | Not run | RUN-ID-TBD |
