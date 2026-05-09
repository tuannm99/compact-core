# Project Knowledge

This document captures the stable knowledge, concepts, and study roadmap needed to build `compact-core` well. It is a project knowledge base, not a formal spec.

## Purpose

Use this file to:

- Track what needs to be learned while building the project
- Capture key compression and systems concepts in one place
- Keep durable knowledge separate from day-to-day implementation notes

## What This Project Requires

To build `compact-core` as a production-grade, reusable compression engine across multiple languages, the learning can be grouped into 5 major layers:

1. Foundation
2. Mathematics for compression
3. Compression algorithms
4. Systems engineering
5. Rust and performance engineering

## 1. Foundation

If this layer is weak, compression work becomes algorithm-stitching without real control over performance or format design.

### Bit, Binary, and Memory Layout

Must understand:

- Bitwise operations
- Endianness
- Alignment
- Memory layout
- Struct packing
- Cache lines
- Branch prediction
- SIMD basics

Why it matters:

- Compression is not just encoding symbols
- It is also moving memory efficiently
- It depends heavily on how bytes are laid out and processed

Core idea:

- Encode patterns into fewer bits
- Reduce entropy where possible
- Minimize unnecessary memory movement

### Buffers and Stream Processing

Most compression systems follow this model:

```text
input stream -> transform -> encode -> output stream
```

Must understand:

- Ring buffers
- Sliding windows
- Chunking
- Zero-copy techniques
- `mmap`
- Streaming I/O

## 2. Mathematics for Compression

No need to go full academic, but the fundamentals matter.

### Entropy

Extremely important concept.

```text
H(X) = -sum(p_i * log2(p_i))
```

Meaning:

- The more predictable the data, the lower the entropy
- Lower entropy usually means better compression potential

Examples:

- `AAAAAAAAAAAA` compresses very well
- `x8J!2Lm#Qp9@` is much harder to compress

### Probability Distribution

Must understand:

- Frequency tables
- Histograms
- Symbol probability
- Adaptive probability

This is the foundation for:

- Huffman coding
- Arithmetic coding
- ANS
- Range coding

## 3. Compression Algorithms

This is the real core.

### Phase A: Beginner Algorithms

#### Run-Length Encoding (RLE)

Example:

```text
AAAAABBBCC -> 5A3B2C
```

What it teaches:

- Tokenization
- Stream encoding
- Edge cases
- Binary framing

#### Delta Encoding

Example:

```text
100 101 102 103 -> 100 +1 +1 +1
```

Why it matters:

- Very useful for ordered numeric data
- Common in analytical systems such as ClickHouse-style storage patterns

#### Dictionary Encoding

Example:

```text
ERROR
ERROR
ERROR
WARN
```

Becomes:

```text
1
1
1
2
```

### Phase B: Real Compression

#### Huffman Coding

Must learn.

What it teaches:

- Priority queues
- Tree encoding
- Prefix codes
- Bit packing

#### LZ77

Core idea:

```text
ABCABCABC -> ABC + (copy distance=3, length=6)
```

Must understand:

- Sliding windows
- Match finding
- Backreferences
- Hash chains

#### LZ4

Worth implementing early because it is:

- Simpler than Zstandard
- Highly practical
- Fast
- Close to production-minded engineering

#### Snappy

Useful for studying systems-oriented tradeoffs:

- Low CPU cost
- Acceptable compression ratio
- Simple and practical design

### Phase C: Advanced Compression

#### Arithmetic Coding

Harder than Huffman.

Core concept:

- Encode symbols by narrowing a probability range

#### ANS (Asymmetric Numeral Systems)

Modern compression foundation.

Why it matters:

- Used in modern compressors such as Zstandard internals
- Helps explain how modern entropy coders balance speed and ratio

#### Zstandard Internals

Must-read implementation family.

Study:

- Block format
- Literals
- Sequences
- FSE
- Match finder
- Dictionary trainer

## 4. Systems Engineering

This is what separates a toy compressor from a production-grade engine.

### File Format Design

Need to design:

- Magic bytes
- Versioning
- Checksums
- Metadata
- Blocks
- Footer

Example shape:

```text
CMP1
[header]
[blocks]
[checksum]
```

### Checksums and Integrity

Must understand:

- CRC32
- `xxhash`
- `cityhash`

Why it matters:

- Corruption must be detectable
- Invalid input must fail safely
- A storage format without integrity strategy is incomplete

### Streaming Compression

Often more important than one-shot file compression.

Relevant systems:

- Kafka
- ClickHouse-style ingestion
- WAL
- Network packets

### Parallel Compression

Important later-stage topics:

- Chunk parallelism
- Dictionary sharing
- Thread pools
- Lock-free queues

## 5. Rust and Performance Engineering

Because this project is implemented in Rust, the language model matters as much as the algorithm model.

### Core Rust

Must be solid on:

- Ownership
- Borrowing
- Slices
- Lifetimes
- Enums
- Traits

### Unsafe Rust

High-performance compression engines often need some `unsafe`, especially around:

- Pointer arithmetic
- Manual memory access
- SIMD-heavy fast paths

Constraint for this project:

- Keep `unsafe` isolated and justified
- Prefer safe Rust by default

### SIMD

Important for serious throughput work.

Study:

- SSE
- AVX2
- NEON

Typical uses:

- Compare many bytes at once
- Speed up match finding
- Accelerate scanning and packing loops

## Projects Worth Reading

### Beginner

- `miniz`
- `lz4`

### Intermediate

- `snappy`
- `brotli`

### Advanced

- `zstd`
- `libdeflate`

## Practical Build Roadmap

Given a systems/backend background, this is a reasonable order.

### Stage 1

Build:

- RLE
- Delta
- VarInt
- Bit packing

Expected output:

```text
compact encode file.json
compact decode file.cmp
```

### Stage 2

Build:

- Huffman
- LZ77
- Block format
- Checksum

### Stage 3

Build:

- LZ4-like engine
- Streaming API
- Benchmark tool

### Stage 4

Build:

- Dictionary training
- Parallel compression
- SIMD paths
- Adaptive encoding

### Stage 5

Build production features:

- Go binding
- Python binding
- WASM
- Kafka plugin
- ClickHouse integration
- WAL compression

## Most Important Reminder

Compression is not just algorithms.

It is the combination of:

- Algorithms
- CPU architecture
- Memory layout
- Probability
- File format design
- Streaming
- Systems engineering

People who build strong compression engines usually become good at:

- Low-level programming
- Database internals
- Distributed systems
- Performance engineering
