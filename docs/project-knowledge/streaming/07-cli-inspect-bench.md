# CLI, Inspect, and Benchmarks

## CLI Streaming Behavior

Current CLI behavior:

```rust
fs::read_to_string(input)
compact_core::io::encode_jsonl(&input_text, &schema)
fs::write(output, encoded)
```

v0.2 CLI should become:

```rust
let input = BufReader::new(File::open(input_path)?);
let output = BufWriter::new(File::create(output_path)?);
compact_core::io::encode_jsonl_stream(input, output, schema, options)?;
```

Decode:

```rust
let input = BufReader::new(File::open(input_path)?);
let output = BufWriter::new(File::create(output_path)?);
compact_core::io::decode_jsonl_stream(input, output, schema)?;
```

The CLI should expose block size:

```text
compact encode input.jsonl output.cmp --schema schema.yml --block-rows 10000
compact encode input.jsonl output.cmp --schema schema.yml --block-bytes 8388608
```

## Inspect for Streaming Files

`compact inspect` should show:

```text
version: 2
blocks: 42
total_rows: 420000
total_raw_bytes: ...
total_compressed_bytes: ...
compression_ratio: ...

block 0 rows=10000 raw=... compressed=... status=ok
block 1 rows=10000 raw=... compressed=... status=ok
block 2 rows=10000 raw=... compressed=... status=checksum_mismatch
```

For columns:

```text
column ts codec=DeltaVarintU64 rows=10000 payload_len=...
column level codec=Dictionary rows=10000 dictionary_size=...
```

Inspect should not fully decode all values unless requested.

It should read metadata and checksums.

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
