# Testing and Benchmarks

## Required Correctness Tests

Posting-list tests must cover:

- Empty posting lists.
- Strictly increasing docIDs.
- Duplicate or decreasing docID rejection.
- Strictly increasing positions inside each posting.
- Duplicate or decreasing position rejection.
- Checksum mismatch rejection.
- Skip metadata inspect.
- Seek hit and miss cases.

Dictionary tests must cover:

- Sorted term metadata.
- Empty term rejection.
- Duplicate or unsorted term rejection.
- Missing term lookup.
- Single-term posting decode without decoding unrelated terms.
- Nested posting-list inspect.
- Corrupted dictionary checksum.

Query tests must cover:

- Single-term docID listing.
- AND intersection.
- Adjacent phrase checks from positions.
- Top-k ranking by term frequency.
- `(term, doc_id)` contains check through posting-list seek.

CLI tests must cover:

- `search-encode`.
- `search-inspect`.
- `search-lookup`.
- `search-seek`.
- `search-bench`.

## CLI Fixture Format

The v0.5 CLI uses a simple line-based fixture format:

```text
term doc_id positions
```

`positions` is comma-separated:

```text
brown 1 1
brown 3 4,8
fox 1 2,9
fox 2 1
```

Use `-` for an empty position list:

```text
anchor 42 -
```

The CLI groups terms, sorts term keys lexicographically, sorts postings by
docID within each term, and lets the core encoder reject duplicate docIDs or
invalid position order.

## Benchmark Signals

v0.5 benchmarks should measure:

- Raw posting bytes versus compressed dictionary bytes.
- Single-term lookup latency.
- `(term, doc_id)` seek latency.
- AND query latency over two or more posting lists.
- Top-k scan latency over representative terms.
- Skip-step tradeoff: metadata size versus seek latency.

Current CLI benchmark command:

```sh
compact search-bench postings.txt --skip-step 16 --top-k 5
```

It reports term count, posting bytes, raw input bytes, encoded bytes,
compression ratio, encode time, inspect time, and top-k term-frequency scan
time. Criterion can replace CLI-level timing later if measurement noise is too
high.
