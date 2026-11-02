# Track A: Performance Log

H-numbers are internal experiment labels; the change descriptions say what each version tried. Run IDs identify the live-board results. Memory is shown in decimal MB. Rows without a confirmed board result are omitted. The final source is restored to H184.

| Step | Change | Run ID | T (s) | M (MB) | Points |
| ---: | --- | --- | ---: | ---: | ---: |
| 0 | Reference, unchanged | - | 32.05 | 228 | 0 |
| 1 | v7 parser and fast exit path | run-eefcfa8a21c2 | 0.062 | 47.6 | 67.289 |
| 2 | v8 adds percentile selection | run-40f20faa186c | 0.063 | 47.3 | 67.105 |
| 3 | H142 compacted the group index | run-dc3e534c6b5b | 0.085 | 38.6 | 64.679 |
| 4 | H83 used CRC32C to index groups | run-95ac2b92059b | 0.082 | 43.1 | 64.498 |
| 5 | v7 resubmission | run-a30ce72b346b | 0.075 | 47.2 | 65.369 |
| 6 | H173 split H142 groups across 12 workers | run-388df169828f | 0.072 | 39.7 | 66.326 |
| 7 | H174 wrote output in parallel and exited immediately | run-4dc5f1362b5c | 0.061 | 54.2 | 66.739 |
| 8 | H175 streamed parallel output writes | run-cb0b856f78a8 | 0.077 | 41.4 | 65.313 |
| 9 | H176 wrote output in batches | run-5d0099fd4be7 | 0.080 | 46.9 | 64.488 |
| 10 | H177 reserved space before parallel output | run-6d2621482e1e | 0.067 | 50.8 | 65.874 |
| 11 | H178 estimated output space in advance | run-8199196f223f | 0.060 | 52.5 | 67.101 |
| 12 | v7 restored and resubmitted | run-a53fa3e56639 | 0.065 | 47.5 | 66.819 |
| 13 | H179 formatted output numbers faster | run-018d7b51d241 | 0.063 | 47.5 | 67.027 |
| 14 | H181 corrected the two-block AVX2 line scan | run-0a505087a037 | 0.061 | 47.5 | 67.400 |
| 15 | H182 parsed four latency digits together | run-5643c5c53bec | 0.061 | 47.6 | 67.349 |
| 16 | H183 cached endpoint lookup fingerprints | run-23f2062c4422 | 0.054 | 46.3 | 68.618 |
| 17 | **H184 used integer group keys in the hash table (final source)** | **run-590815088b72** | **0.054** | **46.2** | **68.685** |
| 18 | H185 stored small groups inline | run-6164043f1eca | 0.064 | 48.7 | 66.288 |
| 19 | H186 used sparse hash maps for groups | run-6f3fc7b14c3a | 0.059 | 56.6 | 67.117 |
| 20 | H187 stored sparse groups as sorted vectors | run-7ece9888b7cf | 0.057 | 51.9 | 67.734 |
| 21 | H188 cached the last endpoint lookup | run-7008c4a3e7b5 | 0.055 | 46.4 | 68.481 |
| 22 | H189 simplified endpoint fingerprints | run-aded64840da9 | 0.060 | 46.7 | 67.516 |
| 23 | H190 adapted the single-block line scanner | run-74b711209003 | 0.055 | 46.5 | 68.530 |
| 24 | H191 parsed latency with a fixed-width method | run-7d38c5531254 | 0.055 | 46.1 | 68.469 |
| 25 | H192 forced inlining of output formatting | run-9559d5e4b4e2 | 0.075 | 46.5 | 65.086 |
| 26 | H193 preallocated endpoint storage | run-7c71f97384e7 | 0.077 | 46.4 | 64.865 |
| 27 | H194 counted statuses without branches | run-b8235acb0b79 | 0.059 | 46.6 | 67.677 |
| 28 | H195 inlined the AVX2 line scanner | run-70228de89dfe | 0.059 | 46.7 | 67.745 |