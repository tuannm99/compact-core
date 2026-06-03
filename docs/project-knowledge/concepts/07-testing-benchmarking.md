# 9. Projects Worth Reading

## Beginner

- `miniz`
- `lz4`

## Intermediate

- `snappy`
- `brotli`

## Advanced

- `zstd`
- `libdeflate`

Study order:

```text
lz4 -> snappy -> brotli -> zstd -> libdeflate
```

Do not only read code. Also study:

- Format docs.
- Block layouts.
- Error handling.
- Benchmark methodology.
- Decoder safety.

---

# 10. Testing Strategy

Compression code requires strong tests.

## 10.1 Roundtrip Tests

Most important test pattern:

```text
decode(encode(input)) == input
```

Use for:

- RLE.
- Delta.
- Varint.
- ZigZag.

- Huffman.
- LZ77.
- Full frame pipeline.

## 10.2 Known Vectors

Use known test vectors where available.

CRC32:

```text
input: "123456789"
expected: 0xCBF43926
```

Varint:

```text
0
1
127
128
u64::MAX
```

ZigZag:

```text
0  -> 0
-1 -> 1

1  -> 2
-2 -> 3
2  -> 4
```

## 10.3 Malformed Input Tests

Decoder must reject invalid input.

Test:

- Truncated frame.
- Bad magic.
- Unsupported version.

- Unsupported codec.

- Checksum mismatch.

- Odd-length RLE payload.

- Truncated varint.
- Varint overflow.
- Invalid Huffman bitstream.

## 10.4 Property Tests

Later, use property-based tests.

Idea:

```text
for random input:
    decode(encode(input)) == input
```

Useful crate:

```text
proptest

```

## 10.5 Corpus Tests

Keep real test files:

```text
testdata/
  repeated.bin
  random.bin
  logs.jsonl
  timestamps.u64le
  text.txt
```

Run all codecs against relevant corpus files.

---

# 11. Benchmarking Strategy

Benchmark both speed and ratio.

## 11.1 Metrics

Track:

```text
original_size
compressed_size
compression_ratio
encode_time
decode_time
encode_MBps
decode_MBps
```

Compression ratio:

```text
compressed_size / original_size
```

Space saving:

```text
1 - compressed_size / original_size
```

## 11.2 Dataset Categories

Use different data categories because codecs behave differently.

```text
repeated bytes      -> RLE should be strong
random bytes        -> compression should not help
json logs           -> LZ/Huffman should help
timestamp series    -> Delta+Varint should help
already compressed  -> likely skip or store raw

```

## 11.3 Important Lesson

A codec that looks good on one dataset may be bad on another.

Compression is data-distribution-dependent.

---
