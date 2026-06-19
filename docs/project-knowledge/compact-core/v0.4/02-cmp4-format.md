# CMP4 Format

## Header

The file starts with:

```text
magic:        4 bytes  "CMP4"
version:      1 byte   4
flags:        1 byte   0 for v0.4
payload_len:  4 bytes  little-endian u32
payload:      payload_len bytes
```

Flags must be rejected when unknown. Silent flag ignoring is unsafe because a
future flag may change how offsets, checksums, or encryption work.

## Body

The body contains row groups. Each row group contains column metadata and column
payloads. v0.4 should preserve the v0.3 decoder contract: codecs are selected
per column and every decoder parameter is persisted.

## Footer

The footer contains query metadata:

- Total row count.
- Row group logical ranges.
- Row group byte ranges.
- Per-column metadata ranges.
- Per-column payload ranges.
- Per-column statistics bytes.

The footer is checksummed independently. This lets metadata scans fail before
they trust offsets.

## Trailer

The file ends with a fixed-width trailer:

```text
footer_offset:    8 bytes little-endian u64
footer_len:       8 bytes little-endian u64
footer_checksum:  4 bytes little-endian u32
magic:            4 bytes "IDX4"
```

Readers only need the final 24 bytes to locate the footer.
