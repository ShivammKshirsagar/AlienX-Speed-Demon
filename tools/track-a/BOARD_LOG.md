# Track A board results

| Role | Commit | Run ID | T | M | S | K | Result |
|---|---|---|---:|---:|---:|---:|---|
| Initial control | `b4d1b99` | Not provided | 0.066 s | 47 MB | 497 | 21.852 | Board result supplied in the mission |
| v4 latency arena | Commit/run ID not supplied | Not supplied | Not supplied | Not supplied | Not supplied | Not supplied | 62.753 points; rejected, below the initial control |
| v2 fast parser | Commit/run ID not supplied | Not supplied | Not supplied | Not supplied | Not supplied | Not supplied | 66.987 points; current best, target 70 not reached |
| v5 partial selection | Commit/run ID not supplied | Not supplied | Not supplied | Not supplied | Not supplied | Not supplied | 66.661 points; rejected, below v2 |
| v3 endpoint table | Commit/run ID not supplied | Not supplied | Not supplied | Not supplied | Not supplied | Not supplied | 66.506 points; rejected, below v2 |
| v1 exit path | Commit/run ID not supplied | Not supplied | Not supplied | Not supplied | Not supplied | Not supplied | 67.034 points; new best, target 70 not reached |
| v7 parser + exit | Commit/run ID not supplied | Not supplied | Not supplied | Not supplied | Not supplied | Not supplied | 67.289 points; new best, target 70 not reached |

Do not fill run IDs or board metrics from local measurements. Current local push candidate is v8 (v7 parser/exit plus v5 percentile selection), prepared for the author to push to the `speed-demon` branch after its combined build/check. v7 remains the best known board result at 67.289 until v8 is scored. Continue using real board results; the active milestone is at least 70 points.
