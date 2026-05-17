# compact-core Project Knowledge

This document captures the stable knowledge, concepts, and study roadmap needed to build `compact-core` well. It is a project knowledge base, not a formal specification.

## Purpose

Use this file to:

- Track what needs to be learned while building the project.
- Capture key compression and systems concepts in one place.
- Keep durable knowledge separate from day-to-day implementation notes.
- Build a shared vocabulary for compression, binary formats, Rust implementation, benchmarking, and performance engineering.

## What This Project Requires

To build `compact-core` as a production-grade, reusable compression engine across multiple languages, the learning can be grouped into these major layers:

1. Foundation: bits, bytes, memory layout, buffers, streaming.
2. Mathematics for compression: entropy, probability, coding theory basics.
3. Compression primitives: RLE, Delta, Varint, ZigZag, bit packing, CRC32.
4. Compression algorithms: Huffman, LZ77, LZ4-like design, Snappy-like tradeoffs, ANS/FSE later.
5. Systems engineering: file format, framing, checksums, streaming, chunking, parallelism.
6. Rust and performance engineering: ownership, slices, traits, safe/unsafe boundaries, profiling, SIMD.
7. Productization: CLI, FFI, Go binding, benchmark tool, documentation, test corpus.

---

# 1. Foundation

If this layer is weak, compression work becomes algorithm-stitching without real control over performance or format design.

Compression is not only about encoding symbols. It is also about:

- Moving memory efficiently.
- Choosing stable binary layouts.
- Handling corrupt input safely.
- Keeping encode/decode perfectly symmetric.
- Understanding how CPU and memory behavior affect throughput.

## 1.1 Bit, Binary, and Memory Layout

### Why this matters

A compression engine spends most of its time doing three things:

1. Reading bytes.
2. Transforming bytes or typed values.
3. Writing bytes back efficiently.

The core skill is understanding how data is represented at the bit, byte, memory, and CPU level.

If this foundation is weak, the compressor may still work correctly, but it will be slow, hard to debug, and difficult to optimize.

### Bit and byte basics

A bit is the smallest unit of data.

```text
bit = 0 or 1
```

A byte is 8 bits.

```text

1 byte = 8 bits
```

Rust represents raw binary data with:

```rust
&[u8]
Vec<u8>

```

A `u8` is one byte.

```rust
let b: u8 = 255;
```

The same value can be viewed in different representations:

```text
binary:  11111111
decimal: 255
hex:     0xFF
```

These are only different ways to view the same bits.

### Decimal, binary, hex, and bytes

Decimal is for humans.

```text
1710000000
```

Binary is how machines represent values.

```text
01100101100100111111000100000000

```

Hex is a compact human-readable form of binary.

```text
0x65 0x53 0xF1 0x00

```

Bytes are how data is stored or transmitted.

Example:

```rust
let x = 1710000000u64;
let bytes = x.to_le_bytes();
```

This converts one `u64` number into 8 bytes.

Important distinction:

```text
u64     = logical number
[u8; 8] = byte representation of that number

```

Compression algorithms often move between these two forms.

### Bitwise operations

Bitwise operations work directly on bits. They are essential for:

- CRC32
- Varint
- Bit packing
- Huffman coding
- Flags
- Frame headers

- SIMD masks

#### AND: `&`

Used to test or extract bits.

```rust

let x = 0b1011u8;
let lowest_bit = x & 1;
```

Result:

```text
1011
0001

----
0001
```

Use case:

```rust
if crc & 1 != 0 {
    // lowest bit is 1
}
```

#### OR: `|`

Used to set bits.

```rust
let x = 0b0100u8;

let y = x | 0b0001;
```

Result:

```text
0100

0001
----
0101
```

Use case:

```rust
byte | 0x80
```

In varint, this sets the continuation bit.

#### XOR: `^`

Used to flip or combine bits.

```rust
let x = 0b1010u8;

let y = 0b1100u8;
let z = x ^ y;
```

Result:

```text
1010
1100
----
0110
```

Use cases:

- CRC32
- XOR delta
- Checksums
- Simple reversible transforms

#### NOT: `!`

Flips all bits.

```rust
let x = 0b00001111u8;
let y = !x;
```

Result:

```text
11110000
```

CRC32 uses final bit inversion:

```rust
!crc
```

#### Left shift: `<<`

Moves bits left.

```rust
let x = 0b00000001u8;
let y = x << 3;
```

Result:

```text

00001000
```

Equivalent to multiplying by powers of two if no overflow happens.

#### Right shift: `>>`

Moves bits right.

```rust
let x = 0b00001000u8;
let y = x >> 3;
```

Result:

```text
00000001
```

Used in:

- CRC32
- Varint decode
- Bit packing
- Extracting high or low bits

### Masks

A mask is a bit pattern used to select or modify specific bits.

Example:

```rust
let x = 0x1234u16;
let low_byte = x & 0x00FF;
```

Result:

```text
0x0034
```

Common masks:

```text
0xFF   = lowest 8 bits
0x7F   = lowest 7 bits
0x80   = highest bit in a byte
0xFFFF = lowest 16 bits
```

Varint uses:

```rust
value & 0x7F
```

to take 7 data bits.

It uses:

```rust
byte | 0x80
```

to mark that more bytes follow.

### Endianness

Endianness defines byte order for multi-byte values.

Example number:

```text
0x12345678
```

Big-endian:

```text
12 34 56 78
```

Little-endian:

```text
78 56 34 12

```

Rust helpers:

```rust

u32::to_le_bytes()
u32::from_le_bytes()

u32::to_be_bytes()
u32::from_be_bytes()
```

For a file format, always define endianness explicitly.

Good default rule:

```text
Use little-endian unless there is a strong reason not to.

```

Why it matters:

If encoder writes little-endian but decoder reads big-endian, data becomes corrupted.

Example:

```rust
let x = 1000u32;
let bytes = x.to_le_bytes();
let decoded = u32::from_le_bytes(bytes);
```

This is safe because both encode and decode agree on byte order.

### Memory layout

Memory layout means how values are placed in memory.

Example:

```rust
struct Point {
    x: u32,
    y: u32,
}
```

Logically:

```text

Point { x: 1, y: 2 }
```

In memory, it may look like:

```text
01 00 00 00 02 00 00 00
```

For compression, memory layout matters because:

- Raw bytes are what get written to disk.
- CPU performance depends on contiguous memory.
- Struct layout may contain padding.
- Different platforms may represent layout differently unless controlled.

Do not blindly serialize structs by copying raw memory.

Prefer explicit serialization:

```rust
out.extend_from_slice(&value.to_le_bytes());
```

### Alignment

Alignment means values are usually stored at memory addresses that match their size.

Example:

```text
u32 prefers address divisible by 4
u64 prefers address divisible by 8
```

Aligned access is usually faster.

Misaligned access may be slower or invalid on some architectures.

Rust usually protects you from unsafe misaligned access.

When working with raw bytes:

```rust
let bytes: &[u8] = ...;
```

This is safe:

```rust
u64::from_le_bytes(bytes[0..8].try_into().unwrap())
```

This can be dangerous in unsafe code:

```rust
*(ptr as *const u64)
```

For compression code, prefer safe parsing unless optimization requires otherwise.

### Struct padding

Compilers may insert padding between fields to satisfy alignment.

Example:

```rust
struct Example {
    a: u8,
    b: u32,
}
```

Logical size:

```text
u8  = 1 byte
u32 = 4 bytes
total expected = 5 bytes
```

Actual size may be:

```text
8 bytes
```

Because padding is inserted after `a`.

Memory shape:

```text
[a][pad][pad][pad][b][b][b][b]

```

Why this matters:

If you serialize raw struct memory, padding bytes may leak uninitialized data or create unstable formats.

Bad idea:

```text
serialize raw struct memory
```

Better idea:

```text
write each field explicitly

```

Example:

```rust
out.push(example.a);
out.extend_from_slice(&example.b.to_le_bytes());
```

### Struct packing

Packed structs remove padding.

Rust:

```rust
#[repr(packed)]
struct Packed {
    a: u8,

    b: u32,
}
```

This can reduce size but may create misaligned fields.

Packed structs are dangerous for performance and safety.

For file formats, better approach:

```text
Define the wire format manually.
```

Example:

```text
[magic: 4 bytes]

[version: 1 byte]
[codec: 1 byte]

[payload_len: u64 little-endian]
[checksum: u32 little-endian]
[payload bytes]
```

This is clearer and safer than relying on struct layout.

### Cache lines

CPU does not read memory one byte at a time.

It loads memory in chunks called cache lines.

Common cache line size:

```text
64 bytes
```

If data is contiguous, CPU can process it efficiently.

Good:

```rust
Vec<u8>
Vec<u64>
```

Bad:

```text

many heap allocations

pointer chasing
linked lists
random memory access

```

Compression benefits from contiguous memory because algorithms scan data sequentially.

Examples:

- RLE scans byte by byte.
- LZ77 scans sliding windows.

- Huffman counts frequencies.
- CRC32 scans payload bytes.

All of them benefit from cache-friendly memory access.

### Branch prediction

CPU tries to predict which branch will run.

Example:

```rust
if byte == previous {
    count += 1;
} else {
    flush_run();
}
```

If the pattern is predictable, CPU runs fast.

If the branch is random, CPU may mispredict and stall.

Compression code often has many branches:

- match or literal
- run continues or ends
- varint continuation or stop
- checksum loop
- Huffman tree traversal

Performance-sensitive codecs try to reduce unpredictable branches.

Random data is harder to compress and often worse for branch prediction.

Repeated data is easier because branch behavior is predictable.

### SIMD basics

SIMD means Single Instruction, Multiple Data.

Instead of comparing one byte at a time, SIMD compares many bytes at once.

Scalar comparison:

```text
compare 1 byte
compare next byte
compare next byte
...
```

SIMD comparison:

```text

compare 16/32/64 bytes at once
```

SIMD is useful for:

- Finding repeated bytes.
- Scanning for delimiters.
- Comparing match candidates.
- Accelerating CRC/checksum.
- Bit packing.
- Copy loops.

Common SIMD families:

```text

SSE
AVX2
AVX-512

NEON
```

Rust can use SIMD through:

```rust
std::arch
portable_simd, when available/stable enough
specialized crates
```

For this project, do not start with SIMD.

First implement correct scalar versions.

Then optimize hot loops after profiling.

---

## 1.2 Buffers and Stream Processing

Most compression systems follow this model:

```text
input stream -> transform -> encode -> output stream
```

A compressor should not require loading a huge file into memory.

It should eventually support:

- File input.
- Memory input.
- Network input.
- Chunked processing.
- Streaming encode/decode.

### Buffer

A buffer is a temporary memory area used to hold data while reading, writing, or transforming.

Example:

```rust
let mut buf = vec![0u8; 64 * 1024];
```

A good compressor usually works with fixed-size buffers or chunks.

Common chunk sizes:

```text
4 KiB
64 KiB
256 KiB
1 MiB
4 MiB
```

Smaller chunks:

- Lower memory usage.
- Better latency.
- Worse compression ratio if cross-chunk matches are unavailable.

Larger chunks:

- Better compression opportunity.
- More memory usage.
- More latency before output.

### Chunking

Chunking splits input into blocks.

Example:

```text
input file
-> block 0
-> block 1
-> block 2
```

Each block can be compressed independently or with shared dictionary context.

Independent blocks are easier for:

- Parallel compression.
- Random access.
- Corruption isolation.

But they may reduce compression ratio because patterns across block boundaries are lost.

### Ring buffer

A ring buffer is a fixed-size circular buffer.

It is useful when only recent data matters.

LZ77 uses a sliding window, which can be implemented with a ring buffer.

Example:

```text

window size = 32 KiB
only last 32 KiB of data is searchable
```

When new bytes arrive, old bytes are overwritten.

### Sliding window

A sliding window keeps recent bytes so the encoder can find repeated sequences.

LZ77 mental model:

```text
[previous bytes in window][current position][future bytes]
```

If current bytes already appeared earlier, encode a backreference:

```text
(distance, length)
```

Example:

```text

ABCABCABC
-> literal ABC
-> match(distance=3, length=6)
```

### Zero-copy

Zero-copy means avoiding unnecessary memory copies.

Bad:

```text
read into buffer A
copy to buffer B

copy to buffer C
encode
```

Better:

```text
read into buffer
encode from same slice
write output
```

In Rust, slices help:

```rust
fn encode(input: &[u8]) -> Vec<u8> {
    // read from input without copying it first
}
```

Zero-copy is a goal, not a starting requirement. Correctness comes first.

### mmap

`mmap` maps a file into virtual memory so it can be accessed like a byte slice.

Benefits:

- Convenient for random access.
- Avoids manual read loops.
- Can rely on OS page cache.

Risks:

- Page faults can affect latency.
- Error handling can be tricky.
- Not always ideal for streaming.

For `compact-core`, normal buffered I/O is enough at first. Add `mmap` later if needed.

### Streaming I/O

Streaming compression means encode/decode while data arrives.

Example:

```text
read chunk
compress chunk
write chunk
repeat
```

Useful for:

- Large files.
- Kafka messages.
- WAL segments.
- Network packets.
- Log ingestion.

Streaming design requires careful state handling:

- Partial input.
- Partial output.

- End-of-stream marker.
- Decoder validation.
- Frame boundaries.

---

# 2. Mathematics for Compression

No need to go full academic, but the fundamentals matter.

The most important idea:

```text
Compression is about representing predictable data with fewer bits.
```

## 2.1 Entropy

Entropy measures uncertainty or information content.

Formula:

```text
H(X) = -sum(p_i * log2(p_i))
```

Meaning:

- The more predictable the data, the lower the entropy.
- Lower entropy usually means better compression potential.
- High-entropy data is hard or impossible to compress well.

Examples:

```text

AAAAAAAAAAAA
```

Very predictable. Compresses well.

```text
x8J!2Lm#Qp9@
```

Less predictable. Harder to compress.

Encrypted or random data usually has high entropy.

### Entropy intuition

If a symbol always appears, it carries little new information.

If many symbols appear with equal probability, each symbol carries more information.

Example:

```text
A A A A A A A A
```

A compressor can encode this with a short representation.

Example:

```text
A X 7 Q M 2 Z !
```

There is less pattern to exploit.

## 2.2 Probability Distribution

Compression depends heavily on symbol probability.

Must understand:

- Frequency tables.
- Histograms.
- Symbol probability.
- Adaptive probability.

This is the foundation for:

- Huffman coding.
- Arithmetic coding.
- ANS.
- Range coding.

### Frequency table

Example input:

```text
AAABBCCCC

```

Frequency table:

```text
A: 3
B: 2
C: 4
```

Probability:

```text
A: 3/9

B: 2/9
C: 4/9
```

Huffman coding uses these frequencies to assign shorter codes to more frequent symbols.

## 2.3 Prefix Codes

A prefix code is a code where no code is the prefix of another code.

Good:

```text
A = 0
B = 10
C = 11
```

Bad:

```text

A = 0
B = 01
```

Because `0` is a prefix of `01`, decoding becomes ambiguous.

Huffman coding creates prefix codes.

## 2.4 Entropy vs Compression Ratio

Entropy gives a theoretical lower bound.

Compression ratio is the practical result.

```text
compression ratio = compressed_size / original_size
```

Example:

```text
original   = 1000 bytes
compressed = 250 bytes

ratio      = 0.25
```

Or as reduction:

```text
space saved = 75%
```

Real compressors cannot always reach entropy limits due to:

- Metadata overhead.
- Block boundaries.
- Simple models.
- Speed tradeoffs.
- Format constraints.

## 2.5 Modeling

A compressor has a model of the data.

Examples:

- RLE model: repeated adjacent symbols are common.
- Delta model: current number is close to previous number.
- LZ77 model: repeated byte sequences occur nearby.
- Huffman model: some symbols appear more often than others.

Better model means better compression potential.

Wrong model can make data larger.

Example:

```text
Apply delta to random text bytes
```

This is usually a bad model.

---

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

# 5. Systems Engineering

This is what separates a toy compressor from a production-grade engine.

## 5.1 File Format Design

A compression format must define how bytes are organized.

Need to design:

- Magic bytes.
- Versioning.
- Codec ID.
- Flags.
- Metadata.
- Blocks.
- Checksums.
- Footer or index, if needed.

Example shape:

```text
CMP1
[header]

[blocks]
[checksum]

```

More concrete frame:

```text
[magic][version][codec][flags][raw_len][payload_len][checksum][payload]
```

Each field must define:

```text
size
meaning
endianness
validation rule
```

Example:

```text

magic       = 4 bytes: "CMP1"
version     = 1 byte
codec       = 1 byte
flags       = 1 byte
raw_len     = u64 little-endian
payload_len = u64 little-endian
checksum    = u32 little-endian
payload     = payload_len bytes
```

Without a defined format, decode cannot know what the bytes mean.

## 5.2 Versioning

Versioning allows format evolution.

Example:

```text
version = 1
```

Decoder behavior:

- If version is supported, decode.
- If version is unknown, return an unsupported version error.

Do not silently decode unknown versions.

## 5.3 Codec IDs

A frame should identify the codec or pipeline used.

Example:

```text
0x01 = RLE
0x02 = DeltaVarintU64

0x03 = Huffman
0x04 = LZ77Huffman
```

Decoder uses codec ID to dispatch:

```rust
match codec {
    Codec::Rle => decode_rle(payload),
    Codec::DeltaVarintU64 => decode_delta_varint_u64(payload),

    _ => Err(CompactError::UnsupportedCodec),
}
```

## 5.4 Checksums and Integrity

Must understand:

- CRC32.
- xxHash.
- cityhash.

Why it matters:

- Corruption must be detectable.

- Invalid input must fail safely.
- A storage format without integrity strategy is incomplete.

Minimum requirement:

```text
CRC32 over compressed payload
```

Better later:

```text
CRC32 over header + payload
or separate header checksum and block checksum
```

## 5.5 Decode Safety

Decoder must never trust input.

Validate:

- Magic bytes.
- Version.
- Codec ID.
- Payload length.

- Raw length.
- Checksum.

- Malformed varint.
- Truncated frame.
- Decompressed size limit.

Important rule:

```text
Decode must return errors, not panic.

```

## 5.6 Streaming Compression

Streaming compression is often more important than one-shot file compression.

Relevant systems:

- Kafka.

- ClickHouse-style ingestion.
- WAL.
- Network packets.
- Log processing.

Streaming design:

```text
read chunk
compress chunk
write frame
repeat
```

Decoder:

```text
read frame
validate frame
decode payload
emit output
repeat
```

## 5.7 Parallel Compression

Important later-stage topics:

- Chunk parallelism.

- Dictionary sharing.
- Thread pools.
- Lock-free queues.
- Ordered output reconstruction.

Simplest model:

```text
split input into independent chunks
compress chunks in parallel
write chunks in original order

```

Tradeoff:

- Faster throughput.
- Potentially worse compression ratio if chunks cannot share history.

---

# 6. Rust and Performance Engineering

Because this project is implemented in Rust, the language model matters as much as the algorithm model.

## 6.1 Core Rust

Must be solid on:

- Ownership.

- Borrowing.

- Slices.
- Lifetimes.
- Enums.
- Traits.
- Error handling.
- Iterators.
- `Vec` capacity.
- `Result` and `?`.

Compression APIs should prefer slices:

```rust
fn encode(input: &[u8]) -> Vec<u8>
```

Decode should return `Result`:

```rust
fn decode(input: &[u8]) -> Result<Vec<u8>, CompactError>
```

Because compressed input may be malformed.

## 6.2 Error Type

Use a project error enum.

Example:

```rust
pub enum CompactError {
    InvalidFormat(&'static str),
    UnsupportedCodec(u8),
    UnsupportedVersion(u8),
    ChecksumMismatch,
    UnexpectedEof,
    VarintOverflow,
}
```

Avoid panics in library code.

## 6.3 Traits

Traits are useful for reusable codec abstractions.

Example:

```rust
pub trait Codec {
    fn encode(&self, input: &[u8]) -> Result<Vec<u8>, CompactError>;
    fn decode(&self, input: &[u8]) -> Result<Vec<u8>, CompactError>;
}
```

But do not over-abstract too early.

First build simple functions.

Then extract traits when patterns are clear.

## 6.4 Safe Rust First

Default rule:

```text
Use safe Rust first.
```

Reasons:

- Easier to test.
- Easier to review.
- Fewer memory safety bugs.
- Good enough for initial learning.

## 6.5 Unsafe Rust

High-performance compression engines often need some `unsafe`, especially around:

- Pointer arithmetic.
- Manual memory access.
- SIMD-heavy fast paths.
- Bounds-check elimination.

Project constraint:

```text
Keep unsafe isolated and justified.
Prefer safe Rust by default.
```

Every unsafe block should answer:

- Why is unsafe needed?
- What invariant makes it safe?
- Is there a test covering this path?

- Is there a benchmark proving it matters?

## 6.6 Allocation Discipline

Compression code should avoid unnecessary allocations.

Use:

```rust
Vec::with_capacity(...)
```

Avoid repeated small allocations inside hot loops.

Bad:

```rust
for item in items {
    let tmp = Vec::new();
}

```

Better:

```rust
let mut out = Vec::with_capacity(estimated_size);
```

## 6.7 Profiling

Do not optimize blindly.

Measure:

- Encode speed.
- Decode speed.
- Compression ratio.
- Allocations.
- CPU hotspots.

Useful tools:

- `criterion` for Rust benchmarks.
- `perf` on Linux.
- Flamegraphs.
- `cargo bench`.

## 6.8 SIMD

SIMD is important for serious throughput work.

Study:

- SSE.

- AVX2.
- NEON.

Typical uses:

- Compare many bytes at once.
- Speed up match finding.
- Accelerate scanning and packing loops.

Do SIMD later.

Correct scalar implementation comes first.

---

# 7. Project Architecture

Recommended high-level workspace:

```text

compact-core/

compact-cli/
compact-ffi/
bindings/go/
schemas/

testdata/
benches/
docs/
```

Recommended `compact-core/src` layout:

```text
src/
  error.rs
  lib.rs
  frame/
    mod.rs
    header.rs
    reader.rs
    writer.rs
  primitives/
    mod.rs
    rle.rs
    delta.rs

    varint.rs

    zigzag.rs

    bitpack.rs
    crc32.rs

  codecs/
    mod.rs
    rle_codec.rs
    huffman.rs
    lz77.rs
  pipeline/
    mod.rs
    delta_varint.rs
    rle_frame.rs
  io/
    mod.rs
    bit_reader.rs
    bit_writer.rs
    chunk_reader.rs
```

## 7.1 Primitive vs Pipeline vs Frame

Main idea:

```text
primitives = small reusable building blocks
pipeline   = how to combine building blocks
frame      = how bytes are stored and validated
```

Example:

```text
Delta      = primitive

Varint     = primitive
CRC32      = primitive
Delta+Varint+Frame = pipeline
```

Do not put schema detection or strategy selection inside primitive loops.

## 7.2 Codec vs Transform vs Encoding

### Transform

A transform changes representation to reduce entropy.

Examples:

- Delta.
- ZigZag.
- XOR.
- BWT.

It may not reduce byte size alone.

### Encoding

Encoding converts values into bytes or bits.

Examples:

- Varint.
- Bit packing.
- Fixed-width little-endian.

### Codec

A codec compresses/decompresses a payload.

Examples:

- RLE.
- Huffman.
- LZ77.
- LZ4-like codec.

RLE is slightly special because it can be viewed as both a simple codec and a transform.

---

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

# 12. Practical Rules for compact-core

Use these rules:

1. Do not rely on raw struct memory as file format.

2. Always define endianness.

3. Keep algorithm-level values and wire-level bytes separate.
4. Delta should operate on typed values, not unknown bytes.

5. Varint should be used after delta when deltas are small.
6. ZigZag should be used when deltas can be negative.
7. Use safe Rust first.
8. Add unsafe/SIMD only after profiling.
9. Validate all decode input.
10. Roundtrip tests are mandatory.
11. Decode must return errors, not panic.
12. Frame format must be explicit and versioned.
13. Use checksums for corruption detection.
14. Benchmark with multiple data types.
15. Do not over-abstract too early.

---

# 13. Most Important Reminder

Compression is not just algorithms.

It is the combination of:

- Algorithms.
- CPU architecture.
- Memory layout.
- Probability.
- File format design.
- Streaming.
- Systems engineering.
- Rust API design.

- Testing discipline.
- Benchmark discipline.

People who build strong compression engines usually become good at:

- Low-level programming.
- Database internals.
- Distributed systems.
- Performance engineering.
- Binary protocol design.

For `compact-core`, the correct mindset is:

```text
Build small correct primitives.
Compose them into explicit pipelines.

Wrap pipelines in safe, versioned frames.
Test roundtrip and malformed input.
Benchmark before optimizing.
```
