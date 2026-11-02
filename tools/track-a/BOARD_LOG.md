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
| H174 parallel output | `8c1a1e1e2d1908240e2b2eea5ae9a0b5611f5a57` | run-4dc5f1362b5c | 0.061107 s | 54,181,888 B | 559 | 21.616049 | 66.738816 points; gates passed, rejected below v7 K |
| H175 streaming parallel writes | `25679c4324fa4a301c01cbd584c766d874e7856a` | run-cb0b856f78a8 | 0.076954 s | 41,390,080 B | 575 | 20.242277 | 65.312968 points; passed gates but time regressed; rejected |
| H176 batched output | `6717f04953005925102fde2ba02a375b32c00293` | run-5d0099fd4be7 | 0.079591 s | 46,931,968 B | 560 | 19.487246 | 64.487523 points; gates passed, rejected for time regression |
| H177 capacity-reserved parallel output | `1e2eb13b0f33b242b3b37736130f0f3105c002af` | run-6d2621482e1e | 0.067338 s | 50,778,112 B | 575 | 20.772502 | 65.874440 points; passed gates; extra sizing pass cost time |
| H178 rough output reserve | 249b11156def0eccfd0305cfcf61577aa474f69b | run-8199196f223f | 0.059838 s | 52,527,104 B | 559 | 21.979985 | 67.101370 points; rejected below v7 |

Do not fill run IDs or board metrics from local measurements. v7 (parser plus exit) remains the best known board result at 67.289. H178 scored below v7; the active source has been restored to board-proven v7 (SHA256 `2FD6CA6CFA5551C735C3A2D2D812D767D1334FA60570302F480D2E157960AE17`). Continue using real board results; the active milestone is at least 71 points.

| v7 restore repeat | 66be859cf79f1e836d99272c550a730361ced921 | run-a53fa3e56639 | 0.065110 s | 47,505,408 B | 526 | 21.696220 | 66.819204 points; same v7 source, below historical best 67.289394 |

| H179 fast integer output | Pending commit | RUN-ID-TBD | TBD | TBD | TBD | TBD | Pairwise decimal formatter atop v7; boundary equivalence checked; board result pending |

| H179 fast integer output | 04c3fa8776c2265fb4c915876dbd4d6449c87fcb | run-018d7b51d241 | 0.063230 s | 47,517,696 B | 544 | 21.904427 | 67.026595 points; correctness passed, rejected below v7 |

| H180 two-block AVX2 scan | Pending commit | RUN-ID-TBD | TBD | TBD | TBD | TBD | One scanner change atop v7; board result pending |
