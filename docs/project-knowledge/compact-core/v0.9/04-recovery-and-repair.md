# Recovery and Repair

## Safety Contract

Repair is copy-on-write. `storage::repair::plan` inspects source bytes and binds
the decision to the source length and CRC32. `execute` rejects the operation if
the bytes changed after planning. It returns a new buffer and never mutates the
input.

## CMP2 Recovery

CMP2 blocks are independent CMP1 frames with checksums and sequential block/row
indexes. Recovery scans from the fixed header and stops at the first truncated,
corrupt, non-sequential, or unknown record. Bytes after that boundary are not
trusted, even if they contain a later sequence that looks valid.

The repair operation:

1. Preserves the valid header and contiguous valid block prefix.
2. Discards only an invalid tail when one exists.
3. Rebuilds `IDX1` from metadata authenticated by each block frame.
4. Validates the generated sealed stream before returning it.

A valid unsealed append stream can be sealed without discarding data. A valid
sealed CMP2 file produces a no-op plan.

## CMP4 Recovery

CMP4 recovery scans row groups from the checked header. For each group it
validates sequential row-group and row indexes, column metadata lengths, value
counts, unique names, statistics metadata, payload ranges, and the full
row-group checksum. It reconstructs footer column offsets and statistics from
the authenticated metadata.

A damaged or missing footer can be rebuilt without losing valid row groups. If
a row group is corrupt, recovery stops before that group and discards it,
everything after it, and the old footer. Searching for later `RGB4` bytes is
forbidden because a marker inside corrupt payload data is not a trusted boundary.

## CLI

```text
compact repair input.cmp --dry-run
compact repair input.cmp --output repaired.cmp
```

The output path must differ from the input path. Atomic replacement and `fsync`
policy remain the caller's responsibility.

CMP1 and CMP3 remain non-repairable because they are single storage units:
checksum failure leaves no independently authenticated prefix to preserve.
