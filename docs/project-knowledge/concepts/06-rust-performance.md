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

