# 4. Compression Algorithms

This is the real core.

## 4.1 Beginner Algorithms

### RLE

Start with RLE because it is simple and teaches binary encode/decode discipline.

Expected API:

```rust
pub fn encode_rle(input: &[u8]) -> Vec<u8>
pub fn decode_rle(input: &[u8]) -> Result<Vec<u8>, CompactError>
```

### Delta + Varint

After RLE, implement typed numeric compression.

Expected API:

```rust
pub fn encode_delta_u64(values: &[u64]) -> Vec<u64>
pub fn decode_delta_u64(values: &[u64]) -> Vec<u64>
```

Then compose:

```text
values -> delta -> varint -> bytes
bytes -> varint -> delta decode -> values
```

### Dictionary Encoding

Dictionary encoding maps repeated values to smaller IDs.

Example:

```text
ERROR

ERROR
WARN

ERROR
```

Dictionary:

```text
ERROR -> 1
WARN  -> 2

```

Encoded:

```text
1 1 2 1
```

Useful for:

- Low-cardinality strings.
- Columnar storage.
- Log levels.
- Country codes.
- Status names.

## 4.2 Real Compression

### Huffman Coding

Huffman coding assigns shorter bit codes to more frequent symbols.

Must learn:

- Frequency table.
- Priority queue.
- Tree construction.
- Prefix codes.
- Canonical Huffman.
- Bit packing.
- Decode table.

Simple example:

```text
A appears often -> short code
Z appears rarely -> long code
```

Huffman requires bit-level output.

So implement BitWriter/BitReader before serious Huffman work.

### LZ77

LZ77 replaces repeated byte sequences with backreferences.

Core idea:

```text
ABCABCABC
-> literal ABC
-> copy distance=3, length=6
```

Must understand:

- Sliding windows.
- Match finding.

- Backreferences.
- Literal tokens.
- Match tokens.
- Window boundaries.
- Hash chains.

Naive match finder is acceptable first.

Optimize later.

### LZ4-like Engine

LZ4 is worth studying because it is:

- Simpler than Zstandard.

- Practical.

- Fast.
- Close to production-minded engineering.

LZ4-like design teaches:

- Literal runs.
- Match lengths.
- Offsets.
- Fast decode.
- Block format.
- Hash table match finder.

### Snappy-like Tradeoffs

Snappy is useful for studying system-oriented compression:

- Low CPU cost.
- Acceptable compression ratio.
- Simple design.
- Fast decode.

This is useful for backend systems where latency matters more than max ratio.

## 4.3 Advanced Compression

### Arithmetic Coding

Arithmetic coding encodes symbols by narrowing a probability range.

It can be more efficient than Huffman but is more complex.

Study after Huffman is stable.

### ANS and FSE

ANS means Asymmetric Numeral Systems.

FSE means Finite State Entropy.

These are modern entropy coding foundations.

Why it matters:

- Used in modern compressors such as Zstandard internals.
- Balances speed and compression ratio.
- More complex than Huffman.

### Zstandard Internals

Zstandard is a must-read implementation family.

Study:

- Block format.
- Literals.
- Sequences.
- FSE.
- Huffman.
- Match finder.

- Repeat offsets.
- Dictionary trainer.
- Streaming API.

Do not try to clone Zstandard early.

Study it to learn architecture and tradeoffs.

---

