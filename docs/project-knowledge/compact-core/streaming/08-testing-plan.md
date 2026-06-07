# Testing Plan for v0.2

Required tests:

- Empty input produces valid empty stream. Done in core tests.
- One row produces one block. Done in core tests.
- Many rows produce multiple blocks. Done in core and CLI tests.
- Final partial block is flushed on finish. Done in core tests.
- Decode output is byte-identical for JSONL fixture. Done in CLI tests.
- Block row counts match decoded rows. Done in core and CLI tests.
- Configured row limit is respected. Done in core and CLI tests.
- Configured byte limit is respected. Done in core tests.
- Truncated file returns error. Done in core tests.
- Corrupted block checksum returns error. Done in core tests.
- Later-block corruption leaves earlier blocks independently decodable. Done in
  core tests.
- Full streaming decode stops at the first corrupted later block. Done in core
  tests; callers may already have received bytes from earlier valid blocks.
- Inspect shows block metadata. Done in core and CLI tests.
- Decode does not buffer all blocks.
- Generated 10,000-row JSONL roundtrip across 10 blocks. Done in CLI tests.

Important malformed cases:

- Invalid file magic.
- Unsupported version.
- Truncated block header.
- Payload length larger than remaining bytes.
- Column count mismatch.
- Column row count mismatch.
- Invalid UTF-8 string payload.
- Varint overflow inside a block.

Manual scale validation should be done before v0.2 release with generated input
large enough to exceed normal memory comfort if buffered. Example target:

```text
compact encode generated-10gb.jsonl generated-10gb.cmp --schema schema.yml --block-rows 10000
compact decode generated-10gb.cmp generated-10gb.out.jsonl --schema schema.yml
compact inspect generated-10gb.cmp
```
