# 3. Compression Primitives

Primitives are small reusable building blocks.

They should be correct, tested, and independent.

Recommended module layout:

```text
primitives/
  rle.rs
  delta.rs
  varint.rs
  zigzag.rs
  bitpack.rs
  crc32.rs
```

## 3.1 Run-Length Encoding (RLE)

RLE compresses repeated adjacent values.

Example:

```text
AAAAABBBCC
-> 5A3B2C
```

Binary layout example:

```text
[count][byte][count][byte]...
```

For `u8` count:

```text
max run length = 255

```

If a run is longer than 255, split it:

```text
300 * A
-> 255A 45A
```

### What RLE teaches

- Sequential scanning.
- Stateful encoding.
- Tokenization.
- Encode/decode symmetry.
- Edge cases.
- Malformed input handling.
- Binary framing basics.

### RLE edge cases

Must test:

- Empty input.
- Single byte.
- No repeated bytes.
- Long run > 255.
- Malformed encoded stream with odd length.
- Roundtrip for arbitrary bytes.

## 3.2 Delta Encoding

Delta encoding stores differences between adjacent numeric values.

Example:

```text
100 101 102 103
-> 100 1 1 1
```

The first value is stored as the base.

Each next value is stored as:

```text
delta = current - previous
```

### Important boundary

Delta does not operate on unknown raw bytes.

Wrong mental model:

```text

file bytes -> delta
```

Correct mental model:

```text
file bytes
-> parse typed values
-> delta transform
-> serialize deltas back into bytes
```

### Good data for delta

- Increasing timestamps.
- Sequential IDs.
- Metrics.
- Sorted numeric columns.
- Time-series samples.

### Bad data for delta

- Plain text.

- JPEG/PNG compressed payloads.
- Encrypted data.
- Random bytes.

### Delta and signed values

If values can decrease, deltas can be negative.

Example:

```text
100 98 101
-> base=100, deltas=[-2, 3]
```

Negative deltas should usually be ZigZag encoded before Varint.

## 3.3 Varint

Varint stores integers using a variable number of bytes.

Small integers use fewer bytes.

Example:

```text
1 -> 1 byte
127 -> 1 byte
128 -> 2 bytes
```

Fixed-width `u64` always uses 8 bytes.

Varint is useful after delta because delta often creates small numbers.

Pipeline:

```text
u64 values
-> delta
-> varint
-> bytes
```

### Varint byte layout

Common base-128 varint:

```text
7 data bits per byte

highest bit = continuation flag
```

If highest bit is 1, more bytes follow.

If highest bit is 0, this is the last byte.

Pseudo-layout:

```text
0xxxxxxx                 one-byte value
1xxxxxxx 0yyyyyyy        two-byte value
1xxxxxxx 1yyyyyyy 0zzzzzzz
```

### Varint edge cases

Must test:

- 0
- 1
- 127
- 128
- u64::MAX
- truncated input
- overlong input
- overflow

## 3.4 ZigZag Encoding

Varint works naturally with unsigned integers.

But signed deltas may be negative.

Naively casting negative `i64` to `u64` creates a huge number.

ZigZag maps signed integers to unsigned integers so small negative values stay small.

Mapping:

```text
0  -> 0
-1 -> 1
1  -> 2
-2 -> 3
2  -> 4
```

Pipeline:

```text
i64 values
-> signed delta
-> zigzag
-> varint
-> bytes
```

## 3.5 Bit Packing

Bit packing stores values using only the number of bits required.

Example:

```text
values: 0..7
needed bits per value = 3
```

Instead of storing each value as 8, 32, or 64 bits, store each as 3 bits.

Useful for:

- Small deltas.
- Integer columns.
- Dictionary IDs.
- Boolean flags.
- Huffman bitstreams.

Requires:

- BitWriter.

- BitReader.
- Careful end-of-stream handling.
- Tests for bit boundaries.

## 3.6 CRC32

CRC32 is an integrity checksum.

It detects accidental corruption.

It is not a cryptographic hash.

Use cases:

- Frame payload verification.
- Detect corrupted compressed data.
- Fail safely during decode.

Common CRC32 test vector:

```text

input:  "123456789"
crc32:  0xCBF43926
```

CRC32 should be used in frame decoding:

```text
read frame
compute CRC32(payload)
compare with stored checksum

if mismatch -> return error
```

---

