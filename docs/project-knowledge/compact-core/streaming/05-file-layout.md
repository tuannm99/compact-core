# File Layout

v0.2 needs a real multi-block file format.

## Option A: Header + Frames Until EOF

Layout:

```text
[file header]
[block frame]
[block frame]
[block frame]
EOF
```

Pros:

- Simple streaming writer.
- No need to seek.
- Works with pipes.

Cons:

- No footer index.
- Inspect must scan all blocks to count them.
- Partial reads by offset are hard.

## Option B: Header + Frames + Footer Index

Layout:

```text
[file header]
[block frame 0]
[block frame 1]
[block frame 2]
[footer index]
[footer pointer]
```

Pros:

- Fast inspect.
- Block index available.
- Future random access.

Cons:

- Writer must track block offsets.
- Usually requires seekable output or footer buffering.
- More format complexity.

Recommended v0.2 implementation:

```text
start with Option A for streaming correctness
add an optional footer index once block writing is stable
```

But the v0.2 DoD requires a block index. Therefore the final v0.2 release should
include either:

- a footer index for seekable files, or
- an inline index stream that can be collected while scanning.

## Proposed v0.2 File Header

The current v0.1 frame has magic `CMP1`.

For multi-block files, add a file-level header before frames.

Example:

```text
magic:      4 bytes  "CMP2"
version:    1 byte   2
flags:      1 byte
header_len: 4 bytes little-endian
header:     JSON/YAML/binary schema metadata
```

Keep it simple for v0.2:

```text
CMP2
version = 2
block_count unknown until footer or scan
schema stored externally for now
```

Because the CLI already accepts `--schema`, v0.2 does not have to embed the
schema immediately. Embedding the schema is useful later for portability.

## Proposed v0.2 Block Frame

Each block should contain enough metadata for inspection.

Logical fields:

```text
block_index
first_row_index
row_count
uncompressed_jsonl_bytes
compressed_payload_bytes
column_count
column metadata
payload
checksum
```

Some of this can live inside the existing frame payload.

Minimum v0.2 block payload:

```text
block_magic       "BLK1"
block_index       u64
first_row_index   u64
row_count         u64
raw_size          u64
column_block      bytes
```

Then wrap it with the existing CRC32 frame.

This gives:

- frame checksum for corruption detection
- block metadata for inspect
- row count for decode validation
- block index for sequential scan

## v0.2 Footer Index

Current v0.2 uses a persisted footer index after all block frames:

```text
[file header]
[block frame 0]
[block frame 1]
[block frame 2]
[index footer]
```

Footer layout:

```text
index_magic       4 bytes  "IDX1"
block_count       u64
entries           repeated block_count times
```

Each index entry:

```text
block_index       u64
encoded_offset    u64
row_count         u64
raw_size          u64
compressed_size   u64
checksum          u32
```

The footer is written sequentially, so the writer does not need `Seek`. Decode
still reads blocks in order and stops cleanly when it reaches `IDX1`. Inspect
scans block frames, validates the footer structure, and reports whether the
stream has a footer index.

This is not a random-access footer pointer yet. Fast seek-to-index is deferred
until the queryable storage phases need it.

## Column Chunk Metadata

Column chunk metadata describes each column inside a block.

Minimum fields:

```text
column name
type
codec
row count
compressed payload length
raw/logical value count
```

Useful later:

```text
min value
max value
null count
dictionary size
uncompressed byte size
compressed byte size
checksum per column
```

For v0.2, do not add all future stats yet.

Add enough for:

- inspect
- validation
- row reconstruction
- benchmark reporting
