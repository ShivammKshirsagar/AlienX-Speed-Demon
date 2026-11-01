# Track A board loop state

- **Current control:** `b4d1b99`, board result supplied in the mission: T=0.066 s, M=47 MB, S=497, K=21.852. No run ID was supplied for that result. The current `track-a/src/main.rs` is byte-identical to `tools/track-a/baseline-snapshot2/src/main.rs` (SHA-256 `71cf21e17af0ff9b2e672943c3444c346ba5915f493dee68f8424820a59a2157`).
- **Best candidate:** v7 (`tools/track-a/variants/v7/`), the v1 exit path plus v2 fast parser. Local correctness and memory gates passed; there is no board evidence yet, so the control remains the best scored state.
- **Manual mode:** board JSON and remote Git access were unavailable in this environment, and `.git` is read-only here. No commit or push was made. Track B's existing dirty changes were left untouched.
- **Next action:** copy v7 over `track-a/` as candidate C and commit `Fast parse exit`; record its board result. If its first C/K pair is below the required 8% speedup, skip the third push. Otherwise restore the exact control and perform C/K/C with at least five minutes between pushes and the prior result visible before each next push.
- **Preserved alternative:** the v2+v4 parse-arena bundle is archived under `tools/track-a/candidates/parse-arena-bundle/`; it is not selected because v4 rescans each line and may erase the v2 parsing gain.
