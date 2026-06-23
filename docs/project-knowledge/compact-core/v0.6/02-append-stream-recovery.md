# Append Stream Recovery

An append stream has this shape:

```text
[CMP2 stream header]
[CMP1 frame containing BLK1 block]
[CMP1 frame containing BLK1 block]
...
```

There is intentionally no `IDX1` footer.

## Recovery Algorithm

1. Validate the stream header.
2. Read one frame header.
3. If the frame header is partial, stop and report a truncated tail.
4. Read the full frame payload.
5. If the payload is partial, stop and report a truncated tail.
6. Decode and checksum the frame.
7. Parse the `BLK1` block metadata.
8. Validate sequential block index and first row index.
9. Record metadata and continue.
10. Return `valid_len`, block metadata, totals, and whether a bad tail exists.

The caller can truncate or ignore bytes after `valid_len`.

## Why Corruption Stops Recovery

Append recovery is conservative. If one block checksum fails, later bytes are
not trusted even if they look like frames. This prevents accidental replay after
data corruption.

## Replay

Replay uses the recovered valid prefix only:

```text
recover -> prefix[..valid_len] -> sequential block reader -> JSONL output
```

This means a crash during the last write cannot corrupt earlier committed
blocks.
