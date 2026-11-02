# Track A board results

| Role | Commit | Run ID | T | M | S | K | Result |
|---|---|---|---:|---:|---:|---:|---|
| Initial control | `b4d1b99` | Not provided | 0.066 s | 47 MB | 497 | 21.852 | Board result supplied in the mission |
| v4 latency arena | Commit/run ID not supplied | Not supplied | Not supplied | Not supplied | Not supplied | Not supplied | 62.753 points; rejected, below the initial control |
| v2 fast parser | Commit/run ID not supplied | Not supplied | Not supplied | Not supplied | Not supplied | Not supplied | 66.987 points; current best, target 70 not reached |
| v5 partial selection | Commit/run ID not supplied | Not supplied | Not supplied | Not supplied | Not supplied | Not supplied | 66.661 points; rejected, below v2 |
| v3 endpoint table | Commit/run ID not supplied | Not supplied | Not supplied | Not supplied | Not supplied | Not supplied | 66.506 points; rejected, below v2 |
| v1 exit path | Commit/run ID not supplied | Not supplied | Not supplied | Not supplied | Not supplied | Not supplied | 67.034 points; new best, target 70 not reached |
| v7 parser + exit | `73b102f33ad580661005ba129efd396ddf72df0c` | run-eefcfa8a21c2 | 0.062314 s | 47,575,040 B | 526 | 22.171133 | 67.289394 points; best known, below target 71 |
| v8 parser + exit + selection | `ce593ff82c0ef4852183836fa5bf1830d200a521` | run-40f20faa186c | 0.063016 s | 47,259,648 B | 541 | 21.983725 | 67.105065 points; rejected, below v7 |
| c3 shared output | Commit/run ID not supplied | RUN-ID-TBD | Not supplied | Not supplied | Not supplied | Not supplied | 62.596 points; rejected, below v7 |

Do not fill run IDs or board metrics from local measurements. v7 (parser plus exit) remains the best known board result at 67.289. Current push candidate is H142, source SHA256 `4770755B0BD792863ED53D54B44D0F07CEE2F62152A2F38A742F55171B66F138`; it passed the recorded local correctness suite but has no board result. Continue using real board results; the active milestone is at least 71 points.
