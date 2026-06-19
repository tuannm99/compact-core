# Testing and Benchmarks

## Unit Tests

Required unit coverage:

- CMP4 header roundtrip and malformed input.
- Footer roundtrip from EOF trailer.
- Footer checksum mismatch.
- Truncated footer and trailer.
- Offset overflow.
- Ranges outside row group.
- Duplicate columns.
- Non-contiguous row groups.
- Binary search row-group lookup.

## Integration Tests

Required integration coverage:

- Encode CMP4, inspect metadata, decode full rows.
- Read one projected column.
- Scan with a predicate that skips at least one row group.
- Metadata-only inspect without schema.
- Corrupted footer fails before payload decode.

## Benchmarks

Required benchmark signals:

- Full scan latency.
- Projected scan latency.
- Predicate-pruned scan latency.
- Metadata-only inspect latency.
- Bytes read for full scan vs projection.

Projection is only proven useful when measured I/O is lower than full scan.
