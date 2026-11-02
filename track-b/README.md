# Track B: Index

## 1. Summary

Track B searches a 254 MB corpus of legal-flavoured text containing 250,000 documents. It answers 50,000 queries: 31,000 Boolean queries and 19,000 exact phrase queries. For each query, the program reports the number of matching documents, the sum of their document IDs, and the ten smallest matching IDs.

The corpus and queries are part of the same timed run: reading the files, building an index, answering the queries, and writing the result all count. The [official Track B specification](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/b-index.md) defines the complete contract, including its tokenizer, edge cases and exact output format.

## 2. Input files

Both inputs are read from `/data`.

### 2.1 `docs.txt`

Each line contains a dense decimal document ID, one TAB byte (`0x09`), the document text, and LF. IDs start at zero and increase by one without leading zeroes. The final line also ends with LF. Document text may be empty; otherwise it contains no TAB, LF or CR. The full seed-1 corpus has 250,000 documents and is 253,827,587 bytes.

### 2.2 Tokenization

Tokenization is defined over bytes in the document text, after the TAB and before LF:

1. ASCII uppercase bytes `A` through `Z` are folded to lowercase.
2. A token is a maximal run of ASCII lowercase letters `a` through `z` or digits `0` through `9`.
3. Every other byte is a separator, including every byte of a UTF-8 character.
4. Tokens are numbered in order. Separators do not become tokens and their number or type does not affect adjacency.

There is no Unicode case folding or normalization, stemming, or stopword removal. Thus `007` and `7` are different terms. `habeas-corpus` is two tokens; `court's` is two; and `U.S.C.` is three. A non-ASCII character can split an apparent word: `Zürich` tokenizes as `z`, `rich`, and `naïve` as `na`, `ve`.

### 2.3 `queries.txt`

The query file is ASCII, with one query per line, LF endings, and a final LF. Single spaces appear exactly where shown:

```text
query   = bool / phrase
bool    = "BOOL" SP term *( SP op SP term )
op      = "AND" / "OR" / "NOT"
phrase  = "PHRASE" SP DQUOTE term 1*( SP term ) DQUOTE
term    = 1*32( %x61-7A / %x30-39 )
```

A Boolean query contains one to six terms. A phrase contains two to eight terms. Parentheses are not part of the language. Operators are uppercase; lowercase `and`, `or`, and `not` are ordinary terms in a term position. A single-term Boolean query is valid. Repeated query lines are separate queries and each gets its own output line.

### 2.4 Corpus and query characteristics

The corpus is generated from a seed-dependent, frequency-ranked vocabulary. Common English function words appear frequently, legal terms occur at varied ranks, and the remaining words are synthetic. Documents vary in length from 1 to 999 word units. Word units are joined by spaces and punctuation, so token boundaries cannot be inferred from spaces alone.

Each scale includes deliberate cases: empty text, text with no tokens, non-ASCII bytes, uppercase and title-case text, repeated words, zero-padded numbers, punctuation-joined terms, and all-caps or mixed-case legal phrases. Some documents also contain extreme token and endpoint-like byte patterns. Tokenization must follow the byte rule above for every case.

The query set contains ordinary queries and targeted edge cases. Boolean cases include absent terms, lowercase operator words, `NOT` that empties a non-empty set, operator sequences that distinguish left-to-right evaluation from precedence, digits, UTF-8 splits, repeated words, and punctuation splits. Phrase cases include scrambled terms, absent terms, repeated terms, UTF-8 splits, uppercase text, digits, punctuation boundaries, sentence boundaries, and phrases that do not occur in the required order.

At full scale, the vocabulary's document-frequency bands are:

| Band | Document frequency | Seed-1 term count |
|---|---|---:|
| Very common | `df > N/10` | 112 |
| Common | `N/100 < df <= N/10` | 1,032 |
| Medium | `N/1,000 < df <= N/100` | 9,347 |
| Uncommon | `N/10,000 < df <= N/1,000` | 90,640 |
| Rare | `1 <= df <= N/10,000` | 508,702 |

An ordinary phrase is drawn from consecutive tokens in a document, so it matches at least one document. Most phrases contain a rare anchor term; some deliberate edge cases contain an absent term or repeated terms. The query order is shuffled. `sample` has one tenth of each full query category; `tiny` includes a minimum number of edge cases.

## 3. Task

A term matches a document if it occurs at least once in that document's token sequence. Repeated occurrences do not add duplicate document matches. A term that occurs nowhere has an empty result set.

### 3.1 Boolean queries

A Boolean query starts with the document set for its first term. Each following operator is applied to the running result strictly from left to right, with no precedence:

- `AND t`: intersect the current result with the documents containing `t`.
- `OR t`: union the current result with the documents containing `t`.
- `NOT t`: subtract the documents containing `t` from the current result. `NOT` is binary set difference.

For example, `a OR b AND c` means `(a OR b) AND c`. A single-term query returns the documents containing that term.

### 3.2 Phrase queries

A phrase matches a document if the document's tokens contain the phrase terms in the same order at consecutive token positions. The bytes between tokens do not matter: spaces, punctuation, hyphens, em dashes and other separators all have the same role. A phrase may cross sentence punctuation, but it never crosses a document boundary. Repeated phrase terms must also be repeated consecutively in the document.

### 3.3 Empty documents

An empty document or a document with no tokens matches no Boolean term or phrase. The query still produces its normal output line.

## 4. Output

The program writes `/out/results.txt`, with exactly one line per query in the same order as `queries.txt`. A line contains:

```text
<count> <sum> <id_1> <id_2> ... <id_m>
```

- `count` is the number of matching documents.
- `sum` is the sum of every matching document ID, not only the IDs printed on the line.
- `id_1 ... id_m` are the `min(10, count)` smallest matching IDs in ascending order.
- No matches are written as `0 0`.

The file is ASCII, uses LF line endings, ends in one LF, and has no trailing spaces. Numbers are decimal with no leading zeroes. The sum fits in an unsigned 64-bit integer at every scale.

### Worked example

In the following small corpus, `<TAB>` represents one TAB byte:

```text
0<TAB>The surety posted bail; the BAIL BOND was forfeited.
1<TAB>Writ of Habeas-Corpus: see 42 U.S.C. § 1983 (bail bond).
2<TAB>
3<TAB>Zürich bail—bond, bail bail bond. Case 007.
4<TAB>In the matter of the estate. Bond not posted, No. 7.
```

The token sequences are:

| Document ID | Tokens |
|---:|---|
| 0 | `the surety posted bail the bail bond was forfeited` |
| 1 | `writ of habeas corpus see 42 u s c 1983 bail bond` |
| 2 | *(none)* |
| 3 | `z rich bail bond bail bail bond case 007` |
| 4 | `in the matter of the estate bond not posted no 7` |

For the queries below, the exact result lines are:

| Query | Result |
|---|---|
| `BOOL bail AND bond` | `3 4 0 1 3` |
| `BOOL bail` | `3 4 0 1 3` |
| `BOOL estate OR surety AND bail` | `1 0 0` |
| `BOOL bond NOT bail` | `1 4 4` |
| `BOOL bail NOT bail` | `0 0` |
| `BOOL zurich` | `0 0` |
| `BOOL rich AND z` | `1 3 3` |
| `BOOL not AND bond` | `1 4 4` |
| `BOOL 7` | `1 4 4` |
| `BOOL the OR writ OR case` | `4 8 0 1 3 4` |
| `PHRASE "bail bond"` | `3 4 0 1 3` |
| `PHRASE "bail bail bond"` | `1 3 3` |
| `PHRASE "bond bail"` | `1 3 3` |
| `PHRASE "habeas corpus"` | `1 1 1` |
| `PHRASE "u s c 1983"` | `1 1 1` |
| `PHRASE "estate bond"` | `1 4 4` |
| `PHRASE "forfeited writ"` | `0 0` |
| `PHRASE "in the matter of"` | `1 4 4` |

The phrase `"estate bond"` crosses the period in document 4. `"forfeited writ"` cannot match across the boundary between documents 0 and 1. Document 3 contains `007`, not `7`, so `BOOL 7` matches document 4 only.

## 5. Dataset sizes and checksums

Seed-1 dataset measurements from the specification:

| | Full | Sample | Tiny |
|---|---:|---:|---:|
| Documents | 250,000 | 25,000 | 1,000 |
| `docs.txt` bytes | 253,827,587 | 25,263,400 | 1,000,131 |
| Tokens | 36,276,657 | 3,613,148 | 143,405 |
| Distinct tokens | 609,833 | 378,185 | 46,109 |
| Distinct term/document pairs | 29,798,541 | 2,971,398 | 117,843 |
| Queries | 50,000 | 5,000 | 1,000 |
| `queries.txt` bytes | 1,763,534 | 177,987 | 35,793 |
| `results.txt` bytes | 1,459,301 | 109,450 | 11,919 |

Seed-1 full-file SHA-256 values:

| File | SHA-256 |
|---|---|
| `docs.txt` | `6cd461670d6c39f9c5211162577532d3648edeab12340c71cc7d2ab5f18e66c8` |
| `queries.txt` | `77ef7ffc59afde051006b02a2ce8e4c090fbbfbf101acd6f862d0f8e298d39f9` |
| Expected `results.txt` | `f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927` |

## 6. Approach

The program reads and parses the query file first, interns distinct query terms, and builds a lookup table for them. It memory-maps `docs.txt` read-only and asks Linux to prepare the mapped pages for reading. The index contains only terms referenced by the queries; it does not need to store the entire corpus vocabulary.

The corpus is divided into newline-aligned ranges for up to 12 worker threads. Each worker tokenizes its documents with AVX2 masks over 64-byte blocks. ASCII uppercase is folded, ASCII letters and digits form tokens, and all other bytes are separators. The scanner records document line offsets so phrase checks can later read the original text.

Before the complete scan, a sample estimates which queried terms are frequent. The selected terms use document bitmaps; remaining terms use posting lists. Sampling chooses the storage representation only. The full corpus scan records matches for every indexed term, so a sampled frequency does not determine query correctness. Each document contributes at most once per term, even when a token repeats in its text.

During scanning, workers update common-term bitmaps and collect pairs for the remaining terms. Pairs are packed when term and worker-local document IDs fit in the compact representation; a full-width representation handles ranges that exceed it. The pairs are sorted by term and scattered into the posting lists. This makes each cold posting list sorted by document ID. Common-term and cold-term sets are then available through a shared list/bitmap interface.

Boolean queries are evaluated from left to right. Sorted posting lists use merge operations; queries involving bitmaps use word-wise operations and reusable worker scratch. The evaluator preserves document order so counts, sums and the first ten IDs can be produced exactly.

For each phrase, the evaluator chooses its rarest term as an anchor, intersects candidates with the other phrase terms' document sets, then checks the exact token sequence in the original document text. The verifier checks token boundaries and walks to neighboring tokens, so punctuation between tokens is ignored while token order and adjacency remain exact. The scan is bounded to one document's text.

Query workers dynamically claim blocks of 64 queries. Each block builds output in memory; blocks are sorted back into query order and concatenated. The program then writes the complete result file to `/out/results.txt`.

## 7. Dependencies

None. The Rust project has no external crates. It uses the standard library, `std::arch` AVX2 intrinsics, and Linux interfaces for read-only memory mapping and memory advice. The Docker build is offline and targets x86-64-v3.

## 8. With another 24 hours

I would profile the full run on the scoring hardware and use the largest measured phase to choose the next change. I would first investigate the cost of phrase candidate filtering and exact verification. Then I would test whether more compact posting storage can reduce peak memory without slowing Boolean operations. Each candidate would be checked against the worked example, byte-tokenization and phrase-boundary cases, sample outputs, and the full seed-1 digest. I would keep a change only if it remains correct and improves the board result.