# compact-core Format Specification

Status: v1.0 stabilization draft

This document defines the file-format contracts that v1.0 must preserve. All
multi-byte integers are little-endian unless a section explicitly says
otherwise. Decoders must reject truncated fields, unsupported versions, invalid
checksums, and count/length values that exceed implementation limits.

## Common Rules

- Magic bytes identify the format family.
- The version byte after the magic must match the known format version.
- Unknown versions are not decoded speculatively.
- Length fields are authoritative only after bounds checks against the input.
- Checksums are computed over the payload named by the format section.
- UTF-8 names must decode as valid UTF-8.
- Unknown flags are unsupported unless documented by a future version.

Known top-level formats:

| Format | Magic | Version | Purpose |
| --- | --- | --- | --- |
| CMP1 | `CMP1` | `1` | Single framed codec payload |
| CMP2 | `CMP2` | `2` | Sequential streaming JSONL blocks |
| CMP3 | `CMP3` | `3` | Typed columnar JSONL file |
| CMP4 | `CMP4` | `4` | Queryable columnar JSONL file with EOF footer |

## CMP1 Frame

CMP1 wraps one codec payload.

Layout:

```text
magic        4 bytes   "CMP1"
version      1 byte    1
codec        1 byte
payload_len  8 bytes   u64
crc32        4 bytes   u32 over payload
payload      N bytes
```

Codec IDs:

| ID | Codec |
| --- | --- |
| `0x01` | RLE |
| `0x02` | Delta + Varint `u64` |
| `0x03` | Huffman, reserved until implemented |
| `0x04` | LZ77, reserved until implemented |
| `0x10` | Column block |

Validation:

- Header must be complete before reading fields.
- Magic must be exactly `CMP1`.
- Version must be `1`.
- Codec ID must be known, even when the selected codec is not implemented at a
  higher layer.
- `payload_len` must fit in memory and match the remaining input exactly.
- CRC32 must match the payload bytes.

## Shared Column Block

CMP1 JSONL column-block payloads and CMP2 row-group payloads share this internal
column layout.

Layout:

```text
magic          4 bytes   "CBL1"
column_count   4 bytes   u32
column[0..N]
```

Column entry:

```text
name_len       2 bytes   u16
name           N bytes   UTF-8
codec          1 byte
row_count      8 bytes   u64
payload_len    8 bytes   u64
payload         N bytes
```

Validation:

- Column count must be bounded by implementation limits and the remaining byte
  range.
- Column names must match the schema selected by the reader.
- All decoded columns in one row group must have the same row count.
- Payload codec must match the schema column codec.

## CMP2 Streaming JSONL Blocks

CMP2 stores JSONL as independently decodable row groups. Each row group is a
CMP1 frame whose codec is `ColumnBlock` and whose payload is a CMP2 block
payload.

File header:

```text
magic          4 bytes   "CMP2"
version        1 byte    2
flags          1 byte    0
header_len     4 bytes   u32
header_payload N bytes
```

Current `header_len` is `0`. Current flags are `0`.

Block payload before CMP1 framing:

```text
magic              4 bytes   "BLK1"
block_index        8 bytes   u64
first_row_index    8 bytes   u64
row_count          8 bytes   u64
raw_size           8 bytes   u64
column_block_len   8 bytes   u64
column_block       N bytes   shared column block
```

Footer index:

```text
magic         4 bytes   "IDX1"
block_count   8 bytes   u64
entry[0..N]
```

Footer entry:

```text
block_index        8 bytes   u64
encoded_offset     8 bytes   u64, offset of the CMP1 block frame
row_count          8 bytes   u64
uncompressed_size  8 bytes   u64
compressed_size    8 bytes   u64, CMP1 frame length
checksum           4 bytes   u32 over the block payload before CMP1 framing
```

Validation:

- Header magic/version/flags must match supported values.
- Every block frame must pass CMP1 validation.
- Every block frame must use codec ID `0x10`.
- Block payload magic must be `BLK1`.
- Block indexes and first-row indexes must be contiguous.
- Block payload checksum must match footer metadata.
- Sealed readers must validate the footer index and reject trailing bytes after
  the footer.
- Append-oriented recovery may read an unsealed prefix and stop before the first
  damaged or truncated block.

## CMP3 Typed Columnar JSONL

CMP3 stores typed column chunks with per-column metadata.

File header:

```text
magic          4 bytes   "CMP3"
version        1 byte    3
flags          1 byte    0
header_len     4 bytes   u32
header_payload N bytes
```

Current `header_len` is `0`. Current flags are `0`.

Column metadata record:

```text
name_len              2 bytes   u16
name                  N bytes   UTF-8
value_type            1 byte
nullable              1 byte    0 or 1
codec                 1 byte
value_count           8 bytes   u64
null_count            8 bytes   u64
raw_size              8 bytes   u64
compressed_size       8 bytes   u64
codec_metadata_len    4 bytes   u32
codec_metadata        N bytes
statistics_len        4 bytes   u32
statistics_metadata   N bytes
```

Value type IDs:

| ID | Type |
| --- | --- |
| `0x01` | `u64` |
| `0x02` | `string` |
| `0x03` | `bool` |

Validation:

- Column name must be non-empty UTF-8.
- Nullable flag must be `0` or `1`.
- `null_count` must not exceed `value_count`.
- Required columns must have `null_count == 0`.
- Codec must be valid for the value type.
- Metadata lengths and compressed payload ranges must remain inside the file.
- Decoders must reject authenticated trailing bytes.

## CMP4 Queryable Columnar JSONL

CMP4 keeps the CMP3 row-group payload model and adds a footer index at EOF so
readers can inspect row groups and locate projected columns without scanning the
whole body.

File header:

```text
magic          4 bytes   "CMP4"
version        1 byte    4
flags          1 byte    0
header_len     4 bytes   u32
header_payload N bytes
```

Current `header_len` is `0`. Current flags are `0`.

Footer body:

```text
total_row_count  8 bytes   u64
row_group_count  8 bytes   u64
row_group[0..N]
```

Row-group footer entry:

```text
row_group_index   8 bytes   u64
first_row_index   8 bytes   u64
row_count         8 bytes   u64
row_group_offset  8 bytes   u64
row_group_len     8 bytes   u64
column_count      4 bytes   u32
column[0..N]
```

Column footer entry:

```text
name_len              2 bytes   u16
name                  N bytes   UTF-8
metadata_offset       8 bytes   u64
metadata_len          8 bytes   u64
payload_offset        8 bytes   u64
payload_len           8 bytes   u64
value_count           8 bytes   u64
null_count            8 bytes   u64
statistics_len        4 bytes   u32
statistics_metadata   N bytes
```

EOF trailer:

```text
footer_offset    8 bytes   u64
footer_len       8 bytes   u64
footer_crc32     4 bytes   u32 over footer body
footer_magic     4 bytes   "IDX4"
```

Validation:

- Header magic/version/flags must match supported values.
- Footer trailer must be present at EOF.
- Footer magic must be `IDX4`.
- Footer byte range must end exactly where the EOF trailer begins.
- Footer CRC32 must match the footer body.
- Row-group indexes and row ranges must be contiguous.
- Row-group and column byte ranges must remain before the footer.
- Column names must be unique within each row group.
- `null_count` must not exceed `value_count`.
- Footer row-group and column counts must be bounded before allocation.

## Compatibility and Recovery

Compatibility:

- Readers negotiate by storage format version, not crate semantic version.
- Known magic with an unexpected version is rejected.
- Future versions should use a new version byte and must not rely on older
  readers ignoring unknown bytes.

Recovery:

- CMP2 recovery can preserve valid blocks before a corrupt or truncated tail.
- CMP4 recovery can rebuild a footer from valid row groups before corruption.
- Repair plans must bind the source bytes they were planned for before writing
  repaired output.
- Repair and migration tools must avoid overwriting source paths or unsafe
  hardlink/symlink destinations.

## Implementation Limits

The Rust implementation enforces allocation and count limits before allocating
large buffers. The exact constants are implementation details until v1.0 API
stability is finalized, but the file-format contract requires decoders to fail
closed when counts, sizes, offsets, or arithmetic exceed supported limits.
