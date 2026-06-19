# Projection and Scan API

## Projection

Projection means selecting a subset of columns before decoding. The scan planner
should map requested column names to footer entries, then read only those column
metadata and payload ranges.

Important behavior:

- Unknown projected columns should return a clear error.
- Empty projection should be valid for metadata-only scans.
- Projection must preserve output row order.
- Projection must not decode unselected column payloads.

## Scan API Shape

The core API should separate planning from execution:

```text
inspect_footer(file) -> FooterIndex
plan_scan(index, projection, predicate) -> ScanPlan
execute_scan(file, schema, plan) -> rows or column batches
```

This makes metadata tests cheap and avoids mixing offset validation with codec
decode logic.

## Streaming Compatibility

Streaming writers can append row groups and record offsets as they go. At close,
they write the footer and trailer. Streaming readers can still sequentially scan
the body, while query readers use the footer for random access.
