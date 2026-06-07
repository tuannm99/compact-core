# v0.2 Implementation Plan

## Current Status

Phase 1 is implemented:

- `CMP2` stream header.
- `BLK1` block payload inside checksum-verified v1 frames.
- `BlockOptions` with default `10,000` rows and `8 MiB`.
- Row-group extraction from the v0.1 one-shot JSONL encoder.
- `JsonlBlockWriter<W: Write>`.
- `JsonlBlockReader<R: Read>`.
- `encode_jsonl_stream<R: BufRead, W: Write>`.
- `decode_jsonl_stream<R: Read, W: Write>`.
- CLI schema encode/decode uses streaming helpers.
- CLI schema encode supports `--block-rows` and `--block-bytes`.
- `compact inspect` understands `CMP2` and shows block metadata.
- CLI integration tests cover streaming encode/decode, inspect, bench, block
  sizing, and invalid block options.
- `compact bench` uses streaming encode/decode and reports throughput metrics.
- Generated 10,000-row CLI validation covers multi-block roundtrip and inspect.
- Corruption isolation tests cover later-block checksum failure while preserving
  earlier block decode.
- Persisted sequential `IDX1` footer index is written after block frames and
  reported by inspect.

Still pending for v0.2 release:

- Optional column metadata in stream inspect.
- Manual 10 GB scale validation.

Recommended order:

1. Define `BlockOptions`. Done.

```rust
pub struct BlockOptions {
    pub max_rows_per_block: usize,
    pub max_uncompressed_bytes_per_block: usize,
}
```

2. Define row group buffer. Done in the writer as a bounded raw JSONL buffer.

```rust
struct RowGroupBuffer {
    columns: Vec<ColumnValues>,
    row_count: usize,
    raw_bytes: usize,
}
```

3. Extract current one-shot column-block encode logic so it can encode one row
   group. Done via `encode_jsonl_row_group`.

4. Add `JsonlBlockWriter<W: Write>`. Done.

5. Add `encode_jsonl_stream<R: BufRead, W: Write>`. Done.

6. Add `JsonlBlockReader<R: Read>`. Done.

7. Add `decode_jsonl_stream<R: Read, W: Write>`. Done.

8. Update CLI to use stream functions. Done for schema encode/decode.

9. Add inspect block metadata. Done for `CMP2` scan-time metadata.

10. Add benchmarks and large generated test. Done for streaming benchmark and
    10,000-row generated CLI validation. Manual 10 GB validation is still
    pending before release.

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
index: persisted sequential IDX1 footer, plus scan-time validation
blocks: independently decodable
dictionary: reset per block
decode corruption: stop by default
inspect corruption: stop at first invalid frame for now
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
