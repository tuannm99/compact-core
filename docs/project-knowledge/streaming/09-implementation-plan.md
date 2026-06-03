# v0.2 Implementation Plan

Recommended order:

1. Define `BlockOptions`.

```rust
pub struct BlockOptions {
    pub max_rows_per_block: usize,
    pub max_uncompressed_bytes_per_block: usize,
}
```

2. Define row group buffer.

```rust
struct RowGroupBuffer {
    columns: Vec<ColumnValues>,
    row_count: usize,
    raw_bytes: usize,
}
```

3. Extract current one-shot column-block encode logic so it can encode one row
   group.

4. Add `JsonlBlockWriter<W: Write>`.

5. Add `encode_jsonl_stream<R: BufRead, W: Write>`.

6. Add `JsonlBlockReader<R: Read>`.

7. Add `decode_jsonl_stream<R: Read, W: Write>`.

8. Update CLI to use stream functions.

9. Add inspect block metadata.

10. Add benchmarks and large generated test.

Do not start with async.

Do not start with parallelism.

Do not start with footer index until sequential blocks work.

## Design Decisions to Make Explicit

Before coding v0.2, answer these:

1. Does v0.2 use a new file magic such as `CMP2`?
2. Is schema embedded in the file or still external via `--schema`?
3. Is the block index required in the first implementation or added after basic
   streaming?
4. Are blocks independently decodable without previous block state?
5. Does dictionary reset per block or share across blocks?
6. Does `decode` stop on first corrupted block?
7. Can `inspect` continue after a corrupted block?
8. Are block sizes configured by rows, bytes, or both?
9. What is the default block size?
10. What metadata is stable public format vs temporary internal detail?

Recommended answers for first v0.2 implementation:

```text
magic: CMP2
schema: external for now
index: collect inline metadata first, footer later
blocks: independently decodable
dictionary: reset per block
decode corruption: stop by default
inspect corruption: report what can be safely read
block limits: rows and bytes
default rows: 10,000
default bytes: 8 MiB
stable metadata: block index, row count, raw size, compressed size
```

## What Not To Do in v0.2

Avoid:

- Async I/O unless sync streaming is already correct.
- Parallel block compression.
- Cross-block dictionary sharing.
- Adaptive codec selection.
- Predicate pushdown.
- Sparse index.
- Random column projection.
- Memory-mapped decode.

Those are future phases.

v0.2 should prove:

```text
large JSONL can encode/decode with bounded memory
blocks are independently validated
CLI no longer buffers whole files
inspect can explain block structure
```
