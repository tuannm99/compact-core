# 13. Practical Rules for compact-core

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

# 14. Most Important Reminder

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
