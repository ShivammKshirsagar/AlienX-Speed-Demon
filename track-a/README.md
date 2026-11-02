# Track A: Ingest

## 1. Summary

Track A reads 3,100,000 HTTP request records covering 72 hours. For each `(hour, endpoint)` pair with traffic, the program reports the request count, p50/p95/p99 latency, the number of 5xx responses, and the 5xx rate in parts per million.

The log is not globally ordered. It represents chunks from eight shards interleaved in arrival order, and the specification gives no bound on how far a line may be out of timestamp order. The program must group all records by their keys and produce a deterministically ordered summary. The [official specification](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md) defines the full contract.

## 2. Input files

The input file is `/data/requests.log`. It is ASCII (and valid UTF-8), has no byte-order mark, header, comments, blank lines or carriage returns, and ends every record with LF. The input grammar is:

```text
line      = ts SP method SP endpoint SP status SP latency LF
ts        = 4DIGIT "-" 2DIGIT "-" 2DIGIT "T" 2DIGIT ":" 2DIGIT ":" 2DIGIT "." 6DIGIT "Z"
method    = "GET" / "POST" / "PUT" / "PATCH" / "DELETE" / "HEAD" / "OPTIONS"
endpoint  = "/" *124echar
echar     = ALPHA / DIGIT / "/" / "-" / "_" / "." / "{" / "}" / ":"
status    = ("1" / "2" / "3" / "4" / "5") 2DIGIT
latency   = "0" / (%x31-39 *9DIGIT)
```

| Field | Meaning and range |
|---|---|
| `ts` | UTC timestamp, exactly 27 bytes with microsecond precision. The fixed span is `2026-10-30T18:00:00.000000Z` through `2026-11-02T17:59:59.999999Z`. |
| `method` | One of seven HTTP methods, from 3 to 7 bytes. It does not affect the result. |
| `endpoint` | Exact path-template bytes, 1 to 125 bytes, beginning with `/`. Case, punctuation, slash placement and placeholders are significant. |
| `status` | Three decimal digits in the range 100 to 599. |
| `latency` | Unsigned 32-bit number of microseconds, from 0 to 4,294,967,295, written without leading zeroes. |

There are 1,842 distinct endpoint templates in the full and sample datasets. The other endpoint names vary by seed; the tiny dataset contains a subset. The catalogue includes short paths such as `/` and a longest path of 125 bytes.

Lines are not sorted by time. Each simulated shard writes non-decreasing timestamps, but the file interleaves chunks from eight shards. Equal timestamps and records from earlier hours may occur after later-hour records, so input position cannot determine output order.

### Input cases covered by the specification

| Case | Required handling |
|---|---|
| Single-request groups | Emit the group; all three percentiles equal its latency. |
| Latency extremes | Correctly handle 0 and 4,294,967,295. |
| Hour boundaries | Assign `HH:59:59.999999Z` and the following `HH:00:00.000000Z` to different hours. |
| Date boundaries | Handle midnight, day changes, and the October-to-November change. |
| Equal timestamps | Keep distinct records, including records separated in the file. |
| Duplicate lines | Count each duplicate as a separate request. |
| Status boundaries | Count 500 through 599 as errors; 499 is not an error. |
| Endpoint extremes | Handle the one-byte `/`, the 125-byte endpoint, and every catalogue entry. |
| Methods | Accept all seven methods; method does not affect grouping or statistics. |

## 3. Task

Each record contributes to the group whose key is `(UTC hour, exact endpoint bytes)`. The timestamp is truncated to its hour. For example, `2026-10-30T18:59:59.999999Z` belongs to hour 18, while `2026-10-30T19:00:00.000000Z` belongs to hour 19.

For each non-empty group, compute:

1. **Count:** number of request records in the group. Identical lines are counted separately.
2. **p50, p95 and p99:** nearest-rank percentiles over all group latencies, retaining duplicates. For percentile `p` and group size `n`, the one-based rank is `ceil(p * n / 100)`, computed as `(p * n + 99) / 100`. The selected percentile is an input latency value.
3. **Errors:** number of records with status from 500 through 599, inclusive. Status 499 is not an error.
4. **Error rate:** `floor(errors * 1,000,000 / count)`, reported as an integer number of parts per million. One error among three requests is `333333` ppm.

The method is ignored for aggregation. Empty hour/endpoint pairs are omitted; only endpoint values that occur in the input are reported.

## 4. Output

The program writes one file: `/out/summary.txt`. Every output line has eight fields separated by single spaces:

```text
hour endpoint count p50 p95 p99 errors error_ppm
```

- `hour` is formatted as `YYYY-MM-DDTHH:00:00Z`.
- `endpoint` is copied byte-for-byte from the input.
- Numeric fields are unsigned decimal integers without leading zeroes or signs.
- Lines are sorted by hour ascending, then endpoint in unsigned-byte order. A proper prefix sorts before the longer endpoint.
- Each `(hour, endpoint)` appears exactly once. The file has no header or trailing spaces, uses LF line endings, and ends with a newline.

### Worked example

Input:

```text
2026-10-30T19:00:00.000000Z GET /v2/orders/{id} 200 1500
2026-10-30T18:59:59.999999Z GET /v2/orders/{id} 200 1841
2026-10-30T18:00:00.000000Z POST /v2/checkout 503 204117
2026-10-30T18:00:00.000000Z POST /v2/checkout 201 30000
2026-10-30T18:00:00.115332Z GET /health 200 62
2026-10-30T18:00:00.115332Z GET /health 200 62
2026-10-30T18:42:17.000001Z HEAD /health 200 4294967295
2026-10-30T18:30:00.000001Z OPTIONS / 200 0
2026-10-30T18:12:00.500000Z POST /v2/checkout 500 0
2026-10-30T18:12:00.600000Z POST /v2/checkout 499 1500
2026-10-30T18:12:00.700000Z POST /v2/checkout 599 2500
2026-10-30T18:20:00.000000Z GET /v2/orders/{id} 500 900
2026-10-30T18:21:00.000000Z DELETE /v2/orders/{id} 404 77
2026-10-30T19:05:00.000000Z GET /v2/orders_legacy 200 60
2026-10-30T19:05:00.000001Z POST /v2/orders-export 202 20
2026-10-30T19:05:00.000002Z GET /v2/orders 200 10
2026-10-31T23:59:59.999999Z POST /v2/checkout 502 7
2026-11-01T00:00:00.000000Z POST /v2/checkout 503 8
```

Output:

```text
2026-10-30T18:00:00Z / 1 0 0 0 0 0
2026-10-30T18:00:00Z /health 3 62 4294967295 4294967295 0 0
2026-10-30T18:00:00Z /v2/checkout 5 2500 204117 204117 3 600000
2026-10-30T18:00:00Z /v2/orders/{id} 3 900 1841 1841 1 333333
2026-10-30T19:00:00Z /v2/orders 1 10 10 10 0 0
2026-10-30T19:00:00Z /v2/orders-export 1 20 20 20 0 0
2026-10-30T19:00:00Z /v2/orders/{id} 1 1500 1500 1500 0 0
2026-10-30T19:00:00Z /v2/orders_legacy 1 60 60 60 0 0
2026-10-31T23:00:00Z /v2/checkout 1 7 7 7 1 1000000
2026-11-01T00:00:00Z /v2/checkout 1 8 8 8 1 1000000
```

The duplicate `/health` requests count twice. The 499 response is excluded from the error count, while statuses 500, 503 and 599 are included. The record at `18:59:59.999999Z` remains in hour 18 even though the hour 19 record appears first in the input.

## 5. Dataset sizes

Seed 1 provides these scales:

| Scale | Input lines | Input bytes | Output lines | Output bytes |
|---|---:|---:|---:|---:|
| Full | 3,100,000 | 238,836,234 | 126,908 | 10,113,024 |
| Sample | 310,000 | 23,870,290 | 66,153 | 5,185,225 |
| Tiny | 1,000 | 76,509 | 873 | 68,899 |

The expected full-scale seed-1 output SHA-256 is `495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3`.

## 6. Approach

The program opens the input with a read-only memory mapping and divides it into ranges ending at line boundaries. Up to 12 worker threads parse separate ranges, so no request is split between workers. Each worker groups records by hour and endpoint, retaining latency values and a 5xx count. Endpoint slices refer to the mapped input instead of allocating a copy for every string.

On the Linux x86-64 release build, an AVX2 scan locates spaces and line endings. An eight-digit fast path parses latency values of that length; other lengths use the general decimal parser. Each worker builds local endpoint IDs and per-hour group maps while scanning.

After the scan, the program reconciles local endpoint IDs by exact byte comparison and merges each worker's groups into the shared result. It sorts group latencies in parallel and reads the three nearest-rank values. It then sorts endpoints by unsigned byte order, formats disjoint hour ranges in parallel, and writes each formatted range at its assigned file offset. Integer arithmetic is used for percentile ranks and `error_ppm`.

## 7. Dependencies

None. The Rust project has no external crates. It uses the Rust standard library and Linux system interfaces for read-only input mapping and final output handling.

## 8. With another 24 hours

I would profile on the scoring hardware and use those measurements to choose the next changes. First, I would compare sorting every latency in a group with selecting only the three required percentile ranks. Then I would test a compact group representation to reduce per-group allocation overhead. Each candidate would be checked against the worked example, boundary cases, and the full expected output; I would keep it only if the board result improves.