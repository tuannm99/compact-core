# Corruption, Backpressure, and Memory

## Corruption Isolation

Current v0.1 behavior:

```text
one corrupted frame -> whole file fails
```

v0.2 should isolate corruption to a block.

Example:

```text
block 0 ok
block 1 ok
block 2 checksum mismatch
block 3 ok
```

Decoder modes:

### Strict Mode

Stop at first corrupted block.

Good for:

- exact decode
- CI
- safe default CLI behavior

### Inspect Mode

Report corrupted block and continue if resynchronization is possible.

Good for:

- debugging
- partial recovery
- storage inspection

Recommended v0.2:

- `decode` defaults to strict mode.
- `inspect` reports block status and continues only if the format allows finding
  the next block boundary safely.

Resynchronization is hard if block lengths are corrupted.

That is why a footer index is useful: it gives known offsets for each block.

## Backpressure-Safe Writer

Backpressure means the output writer may not accept bytes immediately.

In synchronous Rust `Write`, backpressure appears as:

- partial writes hidden by `write_all`
- blocking
- I/O errors

Use:

```rust
writer.write_all(&frame)?;
```

Do not assume:

```rust
writer.write(&frame)? == frame.len()
```

Writer API should not buffer unbounded data if the downstream writer is slow.

For v0.2 sync implementation:

```text
one row group buffer in memory
one encoded block buffer in memory
write_all block
clear buffers
```

Async backpressure belongs to a later phase unless v0.2 explicitly adopts
`tokio::io::AsyncWrite`.

## Memory Budget Rules

Memory should be bounded by:

```text
schema size
current row group values
encoded current block
small read/write buffers
```

Not by:

```text
input file size
output file size
all decoded rows
all encoded blocks
```

Rule of thumb:

```text
peak memory ~= row_group_raw_bytes + encoded_block_bytes + overhead
```

Tests should include a generated input larger than the block size and assert
that multiple blocks are produced.

Rust unit tests usually should not assert exact process RSS because it is noisy.
Instead, assert structural boundedness:

- writer flushes after configured row limit
- only current row group is retained
- decode can process block by block
