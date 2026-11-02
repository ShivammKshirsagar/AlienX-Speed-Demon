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
| H142 compact group index | `6dec450` | RUN-ID-TBD | TBD | TBD | 555 | TBD | Locally committed, awaiting board submission |
| H142 compact group index (scored) | `1e0632050363f93262caef45d42b84f1b04c56a8` | run-dc3e534c6b5b | 0.0848 s | 38,584,320 B | 555 | 19.659857 | 64.679018 points; rejected below v7 |
| H83 CRC32C group index | `74c7df4534f2fa8b86367c9f884f5d6b70605c67` | run-95ac2b92059b | 0.082354 s | 43,110,400 B | 558 | 19.496224 | 64.497525 points; rejected below v7, all gates passed |
| v7 repeat | `c9447d4004ca09c2231e72e46bf738f1cf00ca89` | run-a30ce72b346b | 0.074608 s | 47,198,208 B | 526 | 20.294518 | 65.368937 points; below previous v7 result |
| v7 fast parser + exit (restored best) | Source `2FD6CA6CFA5551C735C3A2D2D812D767D1334FA60570302F480D2E157960AE17` | Existing run-eefcfa8a21c2 | 0.062314 s | 47,575,040 B | 526 | 22.171133 | 67.289394 points; best known, restored locally |
| H173 H142 groups with 12/12 workers | `2b1c989bca2ceb5810c0a6b9424df491da1128ac` | run-388df169828f | 0.072067 s | 39,657,472 B | 555 | 21.209343 | 66.326361 points; rejected below v7 best; all gates passed |
| H174 H173 with parallel output and `_exit` | Pending commit | RUN-ID-TBD | TBD | TBD | 559 | TBD | Active board candidate; fixed equal-hour output blocks; source SHA256 `C3790BADD24852E879AB513CB6F09720F53E67E7E07FFEF13488DFAA0234AA59` |

Do not fill run IDs or board metrics from local measurements. v7 (parser plus exit) remains the best known board result at 67.289. Current push candidate is H142, source SHA256 `4770755B0BD792863ED53D54B44D0F07CEE2F62152A2F38A742F55171B66F138`; it passed the recorded local correctness suite but has no board result. Continue using real board results; the active milestone is at least 71 points.
