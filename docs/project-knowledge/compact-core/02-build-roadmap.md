# 8. Practical Build Roadmap

Given a systems/backend background, this is a reasonable order.

## Stage 0: Skeleton

Build:

- Rust workspace.
- `compact-core` crate.

- `compact-cli` crate.
- Error type.
- Basic test structure.

Expected output:

```text
cargo test
compact --help

```

DoD:

- Workspace builds.
- Empty CLI runs.
- `CompactError` exists.

## Stage 1: Basic Byte Codec

Build:

- RLE encode/decode.
- CRC32.
- Frame v0.

Expected output:

```text

compact encode --codec rle input output
compact decode input output

```

DoD:

- RLE roundtrip exact.
- Malformed RLE returns error.
- Frame has magic/version/codec/payload_len/checksum.
- Decode verifies CRC32.

## Stage 2: Numeric Transform

Build:

- Delta encode/decode for `u64`.
- Varint encode/decode for `u64`.
- ZigZag encode/decode for `i64`.
- Delta + Varint pipeline.

Expected output:

```text
[u64 values]
-> delta
-> varint
-> bytes
-> decode
-> original values
```

DoD:

- Timestamp increasing IDs roundtrip.
- Metrics with positive/negative deltas roundtrip.
- Varint malformed input returns error.

## Stage 3: CLI Usability

Build:

- File encode/decode.

- Inspect command.
- Ratio output.

Commands:

```text
compact encode --codec rle input output
compact decode input output
compact inspect file.cmp
compact encode-u64 --transform delta-varint input.bin output
```

DoD:

- Encode/decode real files.
- Inspect shows codec, raw size, compressed size, ratio, checksum.

## Stage 4: Bit-Level Primitives

Build:

- BitWriter.
- BitReader.
- Bit packing integers.

DoD:

- Write/read arbitrary bits exact.
- Bitpack small deltas.
- Compare varint vs bitpack ratio.

## Stage 5: Huffman

Build:

- Frequency table.
- Huffman tree.
- Canonical Huffman.
- Encode/decode bitstream.

DoD:

- Text roundtrip exact.

- Binary roundtrip exact.
- Malformed bitstream returns error.
- Ratio improves on repetitive text.

## Stage 6: LZ77

Build:

- Sliding window.
- Literal/match token.
- Naive match finder.
- LZ77 decode.

DoD:

- `abcabcabc` becomes literal + match.
- Roundtrip exact.
- Window boundary tests pass.
- No-match data handled.

## Stage 7: Real Pipeline

Build:

- RLE-only pipeline.
- Delta+Varint pipeline.

- LZ77+Huffman pipeline.
- Manual strategy selection.
- Optional auto strategy later.

DoD:

- Config chooses pipeline clearly.

- Delta is not applied without value type.
- Fallback raw/stored block if compressed output is larger than input.

## Stage 8: Benchmarks

Benchmark datasets:

- Repeated bytes.
- Text.
- JSONL logs.
- Timestamp series.
- Random bytes.
- Already compressed data.

Metrics:

- Encode speed MB/s.
- Decode speed MB/s.
- Compression ratio.
- Memory usage rough estimate.

## Stage 9: FFI and Bindings

Build:

- `compact-ffi` C ABI.
- Go binding.
- Optional Python binding.
- Optional WASM.

DoD:

- Go can call encode/decode.
- Roundtrip tests from Go.
- Memory ownership across FFI is documented.

---

