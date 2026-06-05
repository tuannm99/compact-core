# CLI, Inspect, and Benchmarks

## CLI Streaming Behavior

Old CLI behavior:

```rust
fs::read_to_string(input)
compact_core::io::encode_jsonl(&input_text, &schema)
fs::write(output, encoded)
```

Current v0.2 CLI behavior:

```rust
let input = BufReader::new(File::open(input_path)?);
let output = File::create(output_path)?;
compact_core::streaming::encode_jsonl_stream(input, output, schema, options)?;
```

Decode:

```rust
let input = File::open(input_path)?;
let output = File::create(output_path)?;
compact_core::streaming::decode_jsonl_stream(input, output, schema)?;
```

The CLI exposes block size:

```text
compact encode input.jsonl output.cmp --schema schema.yml --block-rows 10000
compact encode input.jsonl output.cmp --schema schema.yml --block-bytes 8388608
```

## Inspect for Streaming Files

`compact inspect` should show:

```text
version: 2
format: stream
blocks: 42
total_rows: 420000
total_raw_bytes: ...
total_compressed_bytes: ...
compression_ratio: ...

block 0 rows=10000 raw=... compressed=... status=ok
block 1 rows=10000 raw=... compressed=... status=ok
block 2 rows=10000 raw=... compressed=... status=checksum_mismatch
```

Current implementation shows block-level metadata and validates frame checksums:

```text
version: 2
format: stream
blocks: 2
total_rows: 3
total_raw_bytes: 78
total_compressed_bytes: 263
input_bytes: 78
output_bytes: 263
compression_ratio: 3.3718
block 0 offset=10 rows=2 raw=52 compressed=137 checksum=...
block 1 offset=147 rows=1 raw=26 compressed=126 checksum=...
```

For columns:

```text
column ts codec=DeltaVarintU64 rows=10000 payload_len=...
column level codec=Dictionary rows=10000 dictionary_size=...
```

Inspect does not fully decode values.

It reads block metadata and verifies frame checksums. Column-level stream
metadata is still pending.

## Benchmarks for v0.2

Streaming benchmark should measure:

- input bytes
- output bytes
- compression ratio
- encode MB/s
- decode MB/s
- block count
- block size setting
- peak memory estimate if available

Datasets:

- small JSONL fixture
- repeated log levels
- timestamp-heavy logs
- random-ish messages
- generated large JSONL

Block size matrix:

```text
1,000 rows
10,000 rows
100,000 rows
1 MiB
8 MiB
64 MiB
```

Expected behavior:

- Larger blocks may improve ratio.
- Larger blocks may increase memory.
- Decode should usually be faster than encode.
