# Testing Plan for v0.2

Required tests:

- Empty input produces valid empty stream.
- One row produces one block.
- Many rows produce multiple blocks.
- Final partial block is flushed on finish.
- Decode output is byte-identical for JSONL fixture.
- Block row counts match decoded rows.
- Configured row limit is respected.
- Configured byte limit is respected.
- Truncated file returns error.
- Corrupted block checksum returns error.
- Inspect shows block metadata.
- Decode does not buffer all blocks.

Important malformed cases:

- Invalid file magic.
- Unsupported version.
- Truncated block header.
- Payload length larger than remaining bytes.
- Column count mismatch.
- Column row count mismatch.
- Invalid UTF-8 string payload.
- Varint overflow inside a block.
