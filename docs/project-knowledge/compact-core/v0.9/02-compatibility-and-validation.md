# Compatibility and Validation

## Detection

`storage::detect` reads only the fixed magic and version fields. It returns a
typed `StorageFormat`, allowing callers to select a version-specific reader
without duplicating byte comparisons.

## Negotiation

`CompatibilityPolicy` declares the oldest and newest format a reader accepts.
`storage::negotiate` rejects files outside that range before decoding. This is
reader compatibility, not crate semantic-version compatibility.

## Validation Depth

- CMP1: exact frame length, codec identifier, and payload checksum.
- CMP2: stream header, every block frame and checksum, sequential indexes and
  row offsets, equality between scanned blocks and the footer index, and no
  trailing bytes after a footer. The report distinguishes a sealed file from a
  valid unsealed append stream.
- CMP3: header, row-group checksum, metadata lengths, value counts, statistics,
  payload ranges, and trailing bytes.
- CMP4: header and footer checksum, index ranges, contiguous logical and physical
  row groups, row-group headers, and every row-group checksum.

Schema-aware decode remains a separate stronger check because a storage file
does not currently embed the complete external schema.
