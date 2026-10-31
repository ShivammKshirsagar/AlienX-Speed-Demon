# Track B: Index

This Rust program interns the query terms, then scans the corpus twice. Newline-aligned workers count distinct term/document pairs, then fill sorted sparse postings and atomic bitmaps for common terms. Boolean operators are evaluated strictly left to right. Phrase queries start from the rarest term and verify consecutive tokens against the original document bytes. Query blocks run on up to 12 workers and are concatenated in input order.

## Dependencies

None. The implementation uses the Rust standard library and Linux `mmap` for the read-only corpus.

## With another 24 hours

The WSL median remains above the target. I would profile the remaining query phase and memory residency, optimize only measured hot paths, and rerun the full digest and held-out samples after each retained change.
