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

