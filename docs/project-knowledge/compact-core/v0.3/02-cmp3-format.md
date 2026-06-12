# CMP3 Format

## File Layout

```text
[CMP3 file header]
[row-group frame 0]
[row-group frame 1]
...
[IDX2 footer index]
```

The exact footer magic may be revised during implementation, but it must not
reuse `IDX1` if the entry layout changes.

## File Header

Minimum header:

```text
magic             4 bytes  "CMP3"
version           1 byte   3
flags             1 byte
header_length     u32
header_payload    header_length bytes
```

The schema may remain external for the first implementation. If the header
payload is empty, decode still requires `--schema`.

Unknown mandatory flags must return `Unsupported`. Reserved bits must not be
ignored silently.

## Row-Group Frame

Logical row-group fields:

```text
block_magic
block_index
first_row_index
row_count
raw_jsonl_size
column_count
column_chunks
checksum
```

Each row group remains independently checksummed.

## Column Chunk

Minimum column chunk metadata:

```text
name_length
name
value_type
nullable
selected_codec
value_count
null_count
raw_size
compressed_size
codec_metadata_length
codec_metadata
payload
```

`value_count` is the logical row count for the column, including nulls.

`compressed_size` covers the encoded payload. The surrounding parser must use
checked arithmetic before allocating or slicing.

## Codec Metadata

Examples:

- Bitpack: bit width and encoded non-null value count.
- Boolean bitmap: logical bit count.
- Prefix string: reset mode and string count.
- Dictionary: dictionary entry count.
- Stored: value framing mode.

The decoder must reject:

- Unknown codec IDs.
- Invalid codec metadata length.
- Bit width greater than 64.
- Null count greater than value count.
- Payload lengths beyond remaining bytes.
- Duplicate or missing schema columns.
- Trailing bytes not defined by the format.

## Footer Index

The footer stores row-group offsets and summary sizes. Column statistics may be
stored in row-group metadata first; duplicating all statistics in the footer is
not required for v0.3.

Fast footer lookup using an EOF pointer remains optional until random access is
implemented.

