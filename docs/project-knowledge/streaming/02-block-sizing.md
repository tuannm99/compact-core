# Block Sizing

There are two common block size policies.

## Row Count Limit

Flush after N rows:

```text
max_rows_per_block = 10_000
```

Pros:

- Easy to reason about.
- Stable row-group sizes.
- Good for JSONL and columnar metadata.

Cons:

- Memory can still grow if one row is huge.

## Byte Size Limit

Flush after buffered input reaches N bytes:

```text
max_uncompressed_bytes_per_block = 8 MiB
```

Pros:

- Better memory control.
- Handles variable-size strings better.

Cons:

- More bookkeeping.
- Row count per block varies.

## Recommended v0.2 Policy

```text
flush when rows >= max_rows_per_block
or uncompressed bytes >= max_uncompressed_bytes_per_block
```

Initial defaults:

```text
max_rows_per_block = 10_000
max_uncompressed_bytes_per_block = 8 MiB
```

These are conservative defaults, not final performance tuning.
